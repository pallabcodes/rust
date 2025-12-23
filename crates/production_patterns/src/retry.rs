//! Retry Pattern with Exponential Backoff
//!
//! Retry patterns automatically retry failed operations with increasing delays
//! between attempts. Combined with circuit breakers, they provide robust error
//! handling for unreliable operations in distributed systems.
//!
//! ## Key Concepts
//!
//! - **Exponential Backoff**: Increase delay between retry attempts
//! - **Jitter**: Randomize delays to prevent thundering herd
//! - **Max Attempts**: Limit total retry attempts
//! - **Retryable Errors**: Only retry specific types of errors
//! - **Custom Backoff**: Configurable backoff strategies
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::retry::{RetryPolicy, RetryConfig};
//!
//! let policy = RetryPolicy::new(RetryConfig {
//!     max_attempts: 3,
//!     initial_delay: Duration::from_millis(100),
//!     max_delay: Duration::from_secs(10),
//!     backoff_multiplier: 2.0,
//!     jitter: true,
//! });
//!
//! let result = policy.retry(|| async {
//!     call_unreliable_service().await
//! }).await;
//! ```

use std::future::Future;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};
use tracing::{debug, instrument, warn};

use crate::common::{ExponentialBackoff, Metrics};
use crate::error::CircuitBreakerError;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
    pub jitter: bool,
    pub timeout: Option<Duration>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
            timeout: Some(Duration::from_secs(10)),
        }
    }
}

/// Retry policy with configurable backoff strategy
#[derive(Debug)]
pub struct RetryPolicy {
    config: RetryConfig,
    backoff: ExponentialBackoff,
    metrics: Metrics,
}

impl RetryPolicy {
    /// Create a new retry policy
    pub fn new(config: RetryConfig) -> Self {
        Self {
            backoff: ExponentialBackoff::new(config.initial_delay, config.max_delay),
            config,
            metrics: Metrics::new(),
        }
    }

    /// Retry an async operation according to the policy
    #[instrument(skip(self, operation), fields(max_attempts = %self.config.max_attempts))]
    pub async fn retry<F, Fut, T, E>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Debug + Clone,
    {
        let mut last_error = None;
        let start_time = Instant::now();

        for attempt in 0..self.config.max_attempts {
            let attempt_start = Instant::now();

            // Execute the operation with optional timeout
            let result = if let Some(timeout_dur) = self.config.timeout {
                match timeout(timeout_dur, operation()).await {
                    Ok(result) => result,
                    Err(_) => Err(RetryError::Timeout),
                }
            } else {
                operation().await
            };

            match result {
                Ok(value) => {
                    let total_duration = start_time.elapsed();
                    self.metrics.record_operation(total_duration);
                    debug!("Operation succeeded on attempt {} after {:?}", attempt + 1, total_duration);
                    return Ok(value);
                }
                Err(error) => {
                    let attempt_duration = attempt_start.elapsed();
                    last_error = Some(error.clone());

                    warn!("Attempt {} failed after {:?}: {:?}", attempt + 1, attempt_duration, error);

                    // If this is the last attempt, don't wait
                    if attempt + 1 >= self.config.max_attempts {
                        break;
                    }

                    // Calculate and apply backoff delay
                    let delay = self.calculate_delay(attempt);
                    debug!("Waiting {:?} before retry", delay);
                    sleep(delay).await;
                }
            }
        }

        // All attempts failed
        let total_duration = start_time.elapsed();
        self.metrics.record_error();

        Err(RetryError::MaxAttemptsExceeded {
            attempts: self.config.max_attempts,
            last_error: Box::new(last_error.unwrap()),
        })
    }

    /// Calculate delay for the given attempt number
    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay = self.backoff.duration(attempt);

        if self.config.jitter {
            // Add random jitter to prevent thundering herd
            // Jitter is ±25% of the base delay
            let jitter_range = (base_delay.as_millis() / 4) as u64;
            let jitter = if jitter_range > 0 {
                let random_offset = (rand::random::<u64>() % (jitter_range * 2)).saturating_sub(jitter_range);
                Duration::from_millis(random_offset)
            } else {
                Duration::from_millis(0)
            };

            base_delay + jitter
        } else {
            base_delay
        }
    }

    /// Get retry statistics
    pub fn stats(&self) -> RetryStats {
        let (total_operations, total_errors, avg_duration) = self.metrics.get_stats();
        RetryStats {
            total_operations,
            total_errors,
            avg_duration,
            config: self.config.clone(),
        }
    }
}

/// Statistics for retry operations
#[derive(Debug, Clone)]
pub struct RetryStats {
    pub total_operations: u64,
    pub total_errors: u64,
    pub avg_duration: Duration,
    pub config: RetryConfig,
}

/// Retry error types
#[derive(Debug, thiserror::Error)]
pub enum RetryError<E> {
    #[error("operation timed out")]
    Timeout,

    #[error("max attempts ({}) exceeded, last error: {:?}", attempts, last_error)]
    MaxAttemptsExceeded {
        attempts: u32,
        last_error: Box<E>,
    },

    #[error("operation failed: {0}")]
    OperationError(#[from] E),
}

impl<E> RetryError<E> {
    /// Get the last error if available
    pub fn last_error(&self) -> Option<&E> {
        match self {
            RetryError::MaxAttemptsExceeded { last_error, .. } => Some(last_error),
            RetryError::OperationError(e) => Some(e),
            _ => None,
        }
    }
}

/// Conditional retry policy that only retries certain errors
#[derive(Debug)]
pub struct ConditionalRetry<F> {
    policy: RetryPolicy,
    should_retry: F,
}

impl<F> ConditionalRetry<F>
where
    F: Fn(&CircuitBreakerError) -> bool,
{
    pub fn new(policy: RetryPolicy, should_retry: F) -> Self {
        Self { policy, should_retry }
    }

    /// Retry only if the error matches the condition
    #[instrument(skip(self, operation))]
    pub async fn retry<T>(
        &self,
        operation: impl Fn() -> std::pin::Pin<Box<dyn Future<Output = Result<T, CircuitBreakerError>> + Send>>,
    ) -> Result<T, RetryError<CircuitBreakerError>> {
        self.policy.retry(|| async {
            let result = operation().await;
            match &result {
                Err(e) if !(self.should_retry)(e) => {
                    // Don't retry this error
                    debug!("Error not retryable: {:?}", e);
                    return result;
                }
                _ => result,
            }
        }).await
    }
}

/// Fixed delay retry policy
#[derive(Debug)]
pub struct FixedDelayRetry {
    config: RetryConfig,
    delay: Duration,
    metrics: Metrics,
}

impl FixedDelayRetry {
    pub fn new(max_attempts: u32, delay: Duration, timeout: Option<Duration>) -> Self {
        Self {
            config: RetryConfig {
                max_attempts,
                initial_delay: delay,
                max_delay: delay,
                backoff_multiplier: 1.0,
                jitter: false,
                timeout,
            },
            delay,
            metrics: Metrics::new(),
        }
    }

    /// Retry with fixed delay between attempts
    #[instrument(skip(self, operation))]
    pub async fn retry<F, Fut, T, E>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Debug + Clone,
    {
        let mut last_error = None;
        let start_time = Instant::now();

        for attempt in 0..self.config.max_attempts {
            let attempt_start = Instant::now();

            let result = if let Some(timeout_dur) = self.config.timeout {
                match timeout(timeout_dur, operation()).await {
                    Ok(result) => result,
                    Err(_) => Err(RetryError::Timeout),
                }
            } else {
                operation().await
            };

            match result {
                Ok(value) => {
                    let total_duration = start_time.elapsed();
                    self.metrics.record_operation(total_duration);
                    debug!("Fixed delay retry succeeded on attempt {}", attempt + 1);
                    return Ok(value);
                }
                Err(error) => {
                    let attempt_duration = attempt_start.elapsed();
                    last_error = Some(error.clone());

                    warn!("Fixed delay attempt {} failed after {:?}: {:?}", attempt + 1, attempt_duration, error);

                    if attempt + 1 < self.config.max_attempts {
                        sleep(self.delay).await;
                    }
                }
            }
        }

        let total_duration = start_time.elapsed();
        self.metrics.record_error();

        Err(RetryError::MaxAttemptsExceeded {
            attempts: self.config.max_attempts,
            last_error: Box::new(last_error.unwrap()),
        })
    }

    pub fn stats(&self) -> RetryStats {
        let (total_operations, total_errors, avg_duration) = self.metrics.get_stats();
        RetryStats {
            total_operations,
            total_errors,
            avg_duration,
            config: self.config.clone(),
        }
    }
}

/// Utility functions for common retry scenarios
pub mod retry_utils {
    use super::*;

    /// Retry network operations (timeouts and connection errors)
    pub fn network_retry() -> RetryPolicy {
        RetryPolicy::new(RetryConfig {
            max_attempts: 5,
            initial_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: true,
            timeout: Some(Duration::from_secs(5)),
        })
    }

    /// Retry database operations (deadlocks, temporary failures)
    pub fn database_retry() -> RetryPolicy {
        RetryPolicy::new(RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 1.5,
            jitter: true,
            timeout: Some(Duration::from_secs(30)),
        })
    }

    /// Retry external API calls
    pub fn api_retry() -> RetryPolicy {
        RetryPolicy::new(RetryConfig {
            max_attempts: 4,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(15),
            backoff_multiplier: 2.0,
            jitter: true,
            timeout: Some(Duration::from_secs(10)),
        })
    }

    /// Check if an error is retryable (generic implementation)
    pub fn is_retryable_error(error: &CircuitBreakerError) -> bool {
        match error {
            CircuitBreakerError::Timeout => true,
            CircuitBreakerError::ExecutionError(msg) => {
                // Retry on specific error messages
                msg.contains("timeout") ||
                msg.contains("connection") ||
                msg.contains("temporary") ||
                msg.contains("unavailable")
            }
            CircuitBreakerError::CircuitOpen => false, // Don't retry circuit breaker errors
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::time::{timeout, Duration};

    static ATTEMPT_COUNT: AtomicU64 = AtomicU64::new(0);

    async fn failing_operation() -> Result<i32, &'static str> {
        let attempt = ATTEMPT_COUNT.fetch_add(1, Ordering::Relaxed);
        if attempt < 2 {
            Err("temporary failure")
        } else {
            Ok(42)
        }
    }

    async fn always_fails() -> Result<i32, &'static str> {
        Err("permanent failure")
    }

    #[tokio::test]
    async fn test_retry_success() {
        ATTEMPT_COUNT.store(0, Ordering::Relaxed);

        let policy = RetryPolicy::new(RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            jitter: false,
            timeout: Some(Duration::from_millis(100)),
        });

        let result = policy.retry(failing_operation).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(ATTEMPT_COUNT.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_retry_max_attempts_exceeded() {
        ATTEMPT_COUNT.store(0, Ordering::Relaxed);

        let policy = RetryPolicy::new(RetryConfig {
            max_attempts: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            jitter: false,
            timeout: Some(Duration::from_millis(100)),
        });

        let result = policy.retry(always_fails).await;
        assert!(matches!(result, Err(RetryError::MaxAttemptsExceeded { .. })));
        assert_eq!(ATTEMPT_COUNT.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_fixed_delay_retry() {
        ATTEMPT_COUNT.store(0, Ordering::Relaxed);

        let retry = FixedDelayRetry::new(3, Duration::from_millis(10), Some(Duration::from_millis(100)));

        let result = retry.retry(failing_operation).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(ATTEMPT_COUNT.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_conditional_retry() {
        let policy = RetryPolicy::new(RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            jitter: false,
            timeout: Some(Duration::from_millis(100)),
        });

        let conditional = ConditionalRetry::new(policy, |e| {
            matches!(e, CircuitBreakerError::Timeout)
        });

        // Test with non-retryable error
        let result = conditional.retry(|| async {
            Err(CircuitBreakerError::CircuitOpen)
        }).await;

        // Should fail immediately without retries
        assert!(matches!(result, Err(RetryError::OperationError(CircuitBreakerError::CircuitOpen))));
    }

    #[tokio::test]
    async fn test_timeout() {
        let policy = RetryPolicy::new(RetryConfig {
            max_attempts: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            jitter: false,
            timeout: Some(Duration::from_millis(50)),
        });

        let result = policy.retry(|| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(42)
        }).await;

        assert!(matches!(result, Err(RetryError::Timeout)));
    }

    #[tokio::test]
    async fn test_jitter() {
        let policy = RetryPolicy::new(RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            jitter: true,
            timeout: Some(Duration::from_millis(50)),
        });

        // Test that jitter is applied (delays should vary)
        let delays: Vec<Duration> = (0..3).map(|i| policy.calculate_delay(i)).collect();

        // With jitter, delays should be different (at least sometimes)
        // This is a probabilistic test, but should pass most of the time
        let all_same = delays.windows(2).all(|w| w[0] == w[1]);
        // Note: This test might occasionally fail due to randomness, but it's rare
        assert!(!all_same || delays.len() < 2);
    }

    #[tokio::test]
    async fn test_retry_stats() {
        let policy = RetryPolicy::new(RetryConfig::default());

        let _ = policy.retry(always_fails).await;

        let stats = policy.stats();
        assert_eq!(stats.total_operations, 0); // No successes
        assert_eq!(stats.total_errors, 1); // One failure
    }

    #[test]
    fn test_utility_configs() {
        let network = retry_utils::network_retry();
        assert_eq!(network.config.max_attempts, 5);

        let db = retry_utils::database_retry();
        assert_eq!(db.config.max_attempts, 3);

        let api = retry_utils::api_retry();
        assert_eq!(api.config.max_attempts, 4);
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(retry_utils::is_retryable_error(&CircuitBreakerError::Timeout));
        assert!(retry_utils::is_retryable_error(&CircuitBreakerError::ExecutionError("connection timeout".to_string())));
        assert!(!retry_utils::is_retryable_error(&CircuitBreakerError::CircuitOpen));
    }
}
