//! Circuit Breaker Pattern
//!
//! Circuit breakers prevent cascading failures by failing fast when downstream
//! services are experiencing problems. They automatically recover when the
//! service becomes healthy again, providing resilience in distributed systems.
//!
//! ## Key Concepts
//!
//! - **Three States**: Closed (normal), Open (failing), Half-Open (testing)
//! - **Failure Threshold**: Open circuit after consecutive failures
//! - **Recovery Timeout**: Time before attempting recovery
//! - **Success Threshold**: Successes needed to close circuit from half-open
//! - **Fallback Support**: Execute alternative logic when circuit is open
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
//!
//! let breaker = CircuitBreaker::new(CircuitBreakerConfig {
//!     failure_threshold: 5,
//!     recovery_timeout: Duration::from_secs(60),
//!     success_threshold: 3,
//! });
//!
//! // Use with fallback
//! let result = breaker.call_with_fallback(
//!     || async { call_unreliable_service().await },
//!     || async { return_fallback_value().await }
//! ).await;
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::{timeout, sleep};
use tracing::{debug, error, info, instrument, warn};

use crate::common::{ExponentialBackoff, Metrics};
use crate::error::CircuitBreakerError;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed,
    /// Circuit is open - requests fail fast
    Open,
    /// Testing recovery - limited requests allowed
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "CLOSED"),
            CircuitState::Open => write!(f, "OPEN"),
            CircuitState::HalfOpen => write!(f, "HALF-OPEN"),
        }
    }
}

/// Configuration for circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,     // Failures to trigger open state
    pub recovery_timeout: Duration, // Time before attempting recovery
    pub success_threshold: u32,     // Successes needed to close from half-open
    pub timeout: Duration,          // Timeout for individual calls
    pub name: Option<String>,       // Optional name for logging
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            success_threshold: 3,
            timeout: Duration::from_secs(10),
            name: None,
        }
    }
}

/// Circuit breaker implementation
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<Mutex<CircuitState>>,
    failures: Arc<Mutex<u32>>,
    successes: Arc<Mutex<u32>>,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
    metrics: Arc<Metrics>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        let name = config.name.clone().unwrap_or_else(|| "circuit-breaker".to_string());
        info!("Created circuit breaker '{}' with failure_threshold={}, recovery_timeout={:?}",
              name, config.failure_threshold, config.recovery_timeout);

        Self {
            config,
            state: Arc::new(Mutex::new(CircuitState::Closed)),
            failures: Arc::new(Mutex::new(0)),
            successes: Arc::new(Mutex::new(0)),
            last_failure_time: Arc::new(Mutex::new(None)),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Execute a function with circuit breaker protection
    #[instrument(skip(self, f), fields(cb_name = %self.config.name.as_deref().unwrap_or("unnamed")))]
    pub async fn call<F, Fut, T>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, CircuitBreakerError>>,
    {
        let timer = crate::common::Timer::new();

        // Check if we should allow the request
        if !self.should_attempt().await {
            self.metrics.record_error();
            return Err(CircuitBreakerError::CircuitOpen);
        }

        // Execute the function with timeout
        let result = match timeout(self.config.timeout, f()).await {
            Ok(result) => result,
            Err(_) => Err(CircuitBreakerError::Timeout),
        };

        let duration = timer.elapsed();
        self.metrics.record_operation(duration);

        // Record the result and update state
        self.record_result(result.is_ok()).await;

        result
    }

    /// Execute with fallback function
    #[instrument(skip(self, f, fallback), fields(cb_name = %self.config.name.as_deref().unwrap_or("unnamed")))]
    pub async fn call_with_fallback<F, Fut, T, Fb, FbFut>(
        &self,
        f: F,
        fallback: Fb,
    ) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, CircuitBreakerError>>,
        Fb: FnOnce() -> FbFut,
        FbFut: std::future::Future<Output = Result<T, CircuitBreakerError>>,
    {
        match self.call(f).await {
            Ok(result) => Ok(result),
            Err(CircuitBreakerError::CircuitOpen) => {
                debug!("Circuit open, executing fallback");
                fallback().await
            }
            Err(e) => Err(e),
        }
    }

    /// Check if a request should be attempted
    async fn should_attempt(&self) -> bool {
        let state = *self.state.lock().await;

        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if recovery timeout has elapsed
                if let Some(last_failure) = *self.last_failure_time.lock().await {
                    if last_failure.elapsed() >= self.config.recovery_timeout {
                        // Transition to half-open
                        *self.state.lock().await = CircuitState::HalfOpen;
                        *self.successes.lock().await = 0;
                        info!("Circuit transitioning to HALF-OPEN");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited requests in half-open state
                // In this simple implementation, we allow all requests in half-open
                // In production, you might want to rate-limit here
                true
            }
        }
    }

    /// Record the result of a call and update circuit state
    async fn record_result(&self, success: bool) {
        let mut state = self.state.lock().await;

        if success {
            match *state {
                CircuitState::Closed => {
                    // Reset failure count on success
                    *self.failures.lock().await = 0;
                }
                CircuitState::HalfOpen => {
                    let mut successes = self.successes.lock().await;
                    *successes += 1;

                    if *successes >= self.config.success_threshold {
                        // Enough successes, close the circuit
                        *state = CircuitState::Closed;
                        *self.failures.lock().await = 0;
                        *self.last_failure_time.lock().await = None;
                        info!("Circuit transitioned to CLOSED after {} successes",
                              self.config.success_threshold);
                    }
                }
                CircuitState::Open => {
                    // This shouldn't happen, but log it
                    warn!("Received success in OPEN state");
                }
            }
        } else {
            // Failure
            match *state {
                CircuitState::Closed => {
                    let mut failures = self.failures.lock().await;
                    *failures += 1;

                    if *failures >= self.config.failure_threshold {
                        *state = CircuitState::Open;
                        *self.last_failure_time.lock().await = Some(Instant::now());
                        warn!("Circuit transitioned to OPEN after {} failures",
                              self.config.failure_threshold);
                    }
                }
                CircuitState::HalfOpen => {
                    // Single failure in half-open sends us back to open
                    *state = CircuitState::Open;
                    *self.last_failure_time.lock().await = Some(Instant::now());
                    *self.successes.lock().await = 0;
                    warn!("Circuit failed in HALF-OPEN, returning to OPEN");
                }
                CircuitState::Open => {
                    // Update last failure time
                    *self.last_failure_time.lock().await = Some(Instant::now());
                }
            }
        }
    }

    /// Get current circuit breaker statistics
    pub async fn stats(&self) -> CircuitBreakerStats {
        let state = *self.state.lock().await;
        let failures = *self.failures.lock().await;
        let successes = *self.successes.lock().await;
        let last_failure = *self.last_failure_time.lock().await;

        let (total_calls, failed_calls, avg_call_time) = self.metrics.get_stats();

        CircuitBreakerStats {
            state,
            failures,
            successes,
            last_failure_time: last_failure,
            total_calls,
            failed_calls,
            avg_call_time,
            config: self.config.clone(),
        }
    }

    /// Manually reset the circuit breaker
    pub async fn reset(&self) {
        *self.state.lock().await = CircuitState::Closed;
        *self.failures.lock().await = 0;
        *self.successes.lock().await = 0;
        *self.last_failure_time.lock().await = None;
        info!("Circuit breaker manually reset");
    }

    /// Get the circuit breaker name
    pub fn name(&self) -> &str {
        self.config.name.as_deref().unwrap_or("unnamed")
    }
}

/// Statistics for circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failures: u32,
    pub successes: u32,
    pub last_failure_time: Option<Instant>,
    pub total_calls: u64,
    pub failed_calls: u64,
    pub avg_call_time: Duration,
    pub config: CircuitBreakerConfig,
}

/// Circuit breaker registry for managing multiple breakers
#[derive(Debug, Default)]
pub struct CircuitBreakerRegistry {
    breakers: Arc<Mutex<std::collections::HashMap<String, Arc<CircuitBreaker>>>>,
}

impl CircuitBreakerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a circuit breaker
    pub async fn register(&self, name: String, breaker: Arc<CircuitBreaker>) {
        let mut breakers = self.breakers.lock().await;
        breakers.insert(name, breaker);
    }

    /// Get a circuit breaker by name
    pub async fn get(&self, name: &str) -> Option<Arc<CircuitBreaker>> {
        let breakers = self.breakers.lock().await;
        breakers.get(name).cloned()
    }

    /// Get all circuit breakers
    pub async fn all_stats(&self) -> std::collections::HashMap<String, CircuitBreakerStats> {
        let breakers = self.breakers.lock().await;
        let mut stats = std::collections::HashMap::new();

        for (name, breaker) in breakers.iter() {
            stats.insert(name.clone(), breaker.stats().await);
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::time::{timeout, Duration};

    static CALL_COUNT: AtomicU64 = AtomicU64::new(0);

    async fn failing_service() -> Result<i32, CircuitBreakerError> {
        let count = CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        if count < 3 {
            Err(CircuitBreakerError::ExecutionError("service failed".to_string()))
        } else {
            Ok(42)
        }
    }

    async fn always_succeeds() -> Result<i32, CircuitBreakerError> {
        Ok(100)
    }

    #[tokio::test]
    async fn test_circuit_breaker_closed_state() {
        CALL_COUNT.store(0, Ordering::Relaxed);

        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_millis(100),
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            name: Some("test".to_string()),
        });

        // Initially closed
        let stats = breaker.stats().await;
        assert_eq!(stats.state, CircuitState::Closed);

        // Should succeed
        let result = breaker.call(always_succeeds).await;
        assert_eq!(result.unwrap(), 100);
    }

    #[tokio::test]
    async fn test_circuit_breaker_open_state() {
        CALL_COUNT.store(0, Ordering::Relaxed);

        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            recovery_timeout: Duration::from_millis(200),
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            name: Some("test".to_string()),
        });

        // Cause failures to open circuit
        for _ in 0..2 {
            let _ = breaker.call(failing_service).await;
        }

        // Circuit should be open
        let stats = breaker.stats().await;
        assert_eq!(stats.state, CircuitState::Open);

        // Calls should fail fast
        let result = breaker.call(always_succeeds).await;
        assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        CALL_COUNT.store(0, Ordering::Relaxed);

        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            recovery_timeout: Duration::from_millis(50),
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            name: Some("test".to_string()),
        });

        // Open the circuit
        for _ in 0..2 {
            let _ = breaker.call(failing_service).await;
        }
        assert_eq!(breaker.stats().await.state, CircuitState::Open);

        // Wait for recovery timeout
        sleep(Duration::from_millis(60)).await;

        // Next call should transition to half-open and succeed
        CALL_COUNT.store(3, Ordering::Relaxed); // Make service succeed
        let result = breaker.call(always_succeeds).await;
        assert_eq!(result.unwrap(), 100);

        // With enough successes, should close
        let result = breaker.call(always_succeeds).await;
        assert_eq!(result.unwrap(), 100);

        let stats = breaker.stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_call_with_fallback() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout: Duration::from_millis(100),
            success_threshold: 1,
            timeout: Duration::from_millis(50),
            name: Some("test".to_string()),
        });

        // Open the circuit
        let _ = breaker.call(failing_service).await;
        assert_eq!(breaker.stats().await.state, CircuitState::Open);

        // Call with fallback should use fallback
        let result = breaker.call_with_fallback(
            failing_service,
            always_succeeds,
        ).await;

        assert_eq!(result.unwrap(), 100);
    }

    #[tokio::test]
    async fn test_timeout() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            success_threshold: 3,
            timeout: Duration::from_millis(10),
            name: Some("test".to_string()),
        });

        // Slow function that exceeds timeout
        let result = breaker.call(|| async {
            sleep(Duration::from_millis(50)).await;
            Ok(42)
        }).await;

        assert!(matches!(result, Err(CircuitBreakerError::Timeout)));
    }

    #[tokio::test]
    async fn test_registry() {
        let registry = CircuitBreakerRegistry::new();

        let breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(60),
            success_threshold: 2,
            timeout: Duration::from_secs(10),
            name: Some("test-breaker".to_string()),
        }));

        registry.register("test".to_string(), breaker.clone()).await;

        let retrieved = registry.get("test").await.unwrap();
        assert_eq!(retrieved.name(), "test-breaker");

        let all_stats = registry.all_stats().await;
        assert_eq!(all_stats.len(), 1);
        assert!(all_stats.contains_key("test"));
    }
}
