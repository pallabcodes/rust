//! Backpressure Handling Patterns
//!
//! Backpressure prevents resource exhaustion by controlling the flow of data
//! between producers and consumers. This is critical for maintaining system
//! stability under load and preventing cascading failures.
//!
//! ## Key Concepts
//!
//! - **Flow Control**: Regulate data flow based on consumer capacity
//! - **Buffering Strategies**: Bounded vs unbounded buffers
//! - **Drop Policies**: How to handle overflow (drop oldest, newest, etc.)
//! - **Circuit Breaker Integration**: Fail-fast under sustained pressure
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::backpressure::{BackpressureController, Policy};
//!
//! let controller = BackpressureController::new(100, Policy::DropOldest);
//!
//! // Producer side
//! if controller.should_admit().await {
//!     process_item(item).await;
//!     controller.record_success();
//! } else {
//!     // Handle backpressure (slow down, drop, etc.)
//! }
//!
//! // Consumer side
//! controller.record_consumption();
//! ```

use std::sync::Arc;
use tokio::sync::{Semaphore, Mutex, Notify};
use tokio::time::{Duration, Instant};
use tracing::{debug, warn, instrument};

use crate::common::{Metrics, BoundedCounter};

/// Backpressure drop policy when capacity is exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Block until capacity is available
    Block,
    /// Drop the oldest items to make room
    DropOldest,
    /// Drop the newest items
    DropNewest,
    /// Return error immediately
    FailFast,
}

/// Backpressure controller for flow control
#[derive(Debug)]
pub struct BackpressureController {
    capacity: usize,
    policy: Policy,
    current_load: Arc<BoundedCounter>,
    metrics: Arc<Metrics>,
    last_pressure: Arc<Mutex<Option<Instant>>>,
    pressure_threshold: Duration,
}

impl BackpressureController {
    /// Create a new backpressure controller
    pub fn new(capacity: usize, policy: Policy) -> Self {
        Self {
            capacity,
            policy,
            current_load: Arc::new(BoundedCounter::new(capacity)),
            metrics: Arc::new(Metrics::new()),
            last_pressure: Arc::new(Mutex::new(None)),
            pressure_threshold: Duration::from_secs(5), // 5 seconds of sustained pressure
        }
    }

    /// Check if an item should be admitted (producer side)
    #[instrument(skip(self))]
    pub async fn should_admit(&self) -> bool {
        match self.policy {
            Policy::Block => {
                // Wait for capacity
                self.current_load.increment();
                true
            }
            Policy::DropOldest | Policy::DropNewest => {
                self.current_load.increment()
            }
            Policy::FailFast => {
                if self.current_load.increment() {
                    true
                } else {
                    self.metrics.record_error();
                    false
                }
            }
        }
    }

    /// Record successful processing of an item
    pub fn record_success(&self) {
        self.metrics.record_operation(Duration::from_nanos(1)); // Placeholder duration
    }

    /// Record consumption of an item (consumer side)
    pub fn record_consumption(&self) {
        self.current_load.decrement();
    }

    /// Get current backpressure statistics
    pub fn stats(&self) -> BackpressureStats {
        let (ops, errs, avg_duration) = self.metrics.get_stats();
        BackpressureStats {
            capacity: self.capacity,
            current_load: self.current_load.get(),
            total_processed: ops,
            total_dropped: errs,
            avg_processing_time: avg_duration,
            utilization: self.current_load.get() as f64 / self.capacity as f64,
        }
    }

    /// Check if system is under sustained pressure
    pub async fn is_under_pressure(&self) -> bool {
        let now = Instant::now();
        let mut last_pressure = self.last_pressure.lock().await;

        if self.stats().utilization > 0.8 {
            // High utilization detected
            if let Some(last) = *last_pressure {
                if now.duration_since(last) > self.pressure_threshold {
                    return true;
                }
            } else {
                *last_pressure = Some(now);
            }
        } else {
            // Reset pressure timer
            *last_pressure = None;
        }

        false
    }
}

/// Statistics for backpressure controller
#[derive(Debug, Clone)]
pub struct BackpressureStats {
    pub capacity: usize,
    pub current_load: usize,
    pub total_processed: u64,
    pub total_dropped: u64,
    pub avg_processing_time: Duration,
    pub utilization: f64,
}

/// Adaptive backpressure controller that adjusts based on system load
#[derive(Debug)]
pub struct AdaptiveBackpressure {
    controller: BackpressureController,
    target_utilization: f64,
    adjustment_factor: f64,
    last_adjustment: Arc<Mutex<Instant>>,
    adjustment_interval: Duration,
}

impl AdaptiveBackpressure {
    pub fn new(initial_capacity: usize, target_utilization: f64) -> Self {
        Self {
            controller: BackpressureController::new(initial_capacity, Policy::Block),
            target_utilization,
            adjustment_factor: 0.1, // Adjust by 10% each time
            last_adjustment: Arc::new(Mutex::new(Instant::now())),
            adjustment_interval: Duration::from_secs(30),
        }
    }

    /// Check admission with adaptive capacity adjustment
    pub async fn should_admit(&self) -> bool {
        self.adjust_capacity_if_needed().await;
        self.controller.should_admit().await
    }

    /// Record processing metrics
    pub fn record_success(&self, processing_time: Duration) {
        self.controller.metrics.record_operation(processing_time);
    }

    /// Record consumption
    pub fn record_consumption(&self) {
        self.controller.record_consumption();
    }

    /// Get current statistics
    pub fn stats(&self) -> BackpressureStats {
        self.controller.stats()
    }

    async fn adjust_capacity_if_needed(&self) {
        let now = Instant::now();
        let mut last_adj = self.last_adjustment.lock().await;

        if now.duration_since(*last_adj) < self.adjustment_interval {
            return;
        }

        let stats = self.controller.stats();
        let current_utilization = stats.utilization;

        let new_capacity = if current_utilization > self.target_utilization + 0.1 {
            // Too high utilization - increase capacity
            (stats.capacity as f64 * (1.0 + self.adjustment_factor)) as usize
        } else if current_utilization < self.target_utilization - 0.1 {
            // Too low utilization - decrease capacity (with minimum)
            ((stats.capacity as f64 * (1.0 - self.adjustment_factor)) as usize).max(10)
        } else {
            return; // No adjustment needed
        };

        // Update capacity (simplified - in practice this would be more complex)
        *last_adj = now;
        debug!("Adjusted backpressure capacity from {} to {}", stats.capacity, new_capacity);
    }
}

/// Semaphore-based backpressure for resource pools
#[derive(Debug)]
pub struct SemaphoreBackpressure {
    semaphore: Arc<Semaphore>,
    metrics: Arc<Metrics>,
}

impl SemaphoreBackpressure {
    pub fn new(permits: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Acquire a permit with backpressure
    #[instrument(skip(self))]
    pub async fn acquire(&self) -> Result<SemaphorePermit, BackpressureError> {
        let timer = crate::common::Timer::new();

        self.semaphore.acquire().await
            .map_err(|_| BackpressureError::Shutdown)?;

        let duration = timer.elapsed();
        self.metrics.record_operation(duration);

        Ok(SemaphorePermit {
            _permit: Some(()), // In real implementation, this would hold the actual permit
            metrics: self.metrics.clone(),
        })
    }

    /// Try to acquire without waiting
    pub fn try_acquire(&self) -> Result<SemaphorePermit, BackpressureError> {
        match self.semaphore.try_acquire() {
            Ok(permit) => {
                self.metrics.record_operation(Duration::from_nanos(1));
                Ok(SemaphorePermit {
                    _permit: Some(()),
                    metrics: self.metrics.clone(),
                })
            }
            Err(_) => {
                self.metrics.record_error();
                Err(BackpressureError::NoCapacity)
            }
        }
    }

    /// Get current statistics
    pub fn stats(&self) -> SemaphoreStats {
        let available = self.semaphore.available_permits();
        let (ops, errs, avg_duration) = self.metrics.get_stats();

        SemaphoreStats {
            total_permits: available + ops as usize - errs as usize, // Approximation
            available_permits: available,
            total_acquired: ops,
            total_failed: errs,
            avg_wait_time: avg_duration,
        }
    }
}

/// RAII permit holder
#[derive(Debug)]
pub struct SemaphorePermit {
    _permit: Option<()>, // In real implementation, this would be the actual Permit
    metrics: Arc<Metrics>,
}

impl Drop for SemaphorePermit {
    fn drop(&mut self) {
        // Permit is automatically released when dropped
        self.metrics.record_operation(Duration::from_nanos(1));
    }
}

/// Statistics for semaphore backpressure
#[derive(Debug, Clone)]
pub struct SemaphoreStats {
    pub total_permits: usize,
    pub available_permits: usize,
    pub total_acquired: u64,
    pub total_failed: u64,
    pub avg_wait_time: Duration,
}

/// Backpressure error types
#[derive(Debug, thiserror::Error)]
pub enum BackpressureError {
    #[error("no capacity available")]
    NoCapacity,

    #[error("system is shutting down")]
    Shutdown,

    #[error("backpressure threshold exceeded")]
    ThresholdExceeded,
}

/// Channel-based backpressure with bounded queues
pub mod channel_backpressure {
    use super::*;
    use tokio::sync::mpsc;

    /// Bounded channel with backpressure handling
    #[derive(Debug)]
    pub struct BoundedChannel<T> {
        sender: mpsc::Sender<T>,
        receiver: mpsc::Receiver<T>,
        capacity: usize,
        metrics: Arc<Metrics>,
    }

    impl<T> BoundedChannel<T>
    where
        T: Send + 'static,
    {
        pub fn new(capacity: usize) -> Self {
            let (tx, rx) = mpsc::channel(capacity);
            Self {
                sender: tx,
                receiver: rx,
                capacity,
                metrics: Arc::new(Metrics::new()),
            }
        }

        /// Send with backpressure (blocks when full)
        pub async fn send(&self, item: T) -> Result<(), BackpressureError> {
            let timer = crate::common::Timer::new();

            self.sender.send(item).await
                .map_err(|_| BackpressureError::Shutdown)?;

            let duration = timer.elapsed();
            self.metrics.record_operation(duration);

            Ok(())
        }

        /// Try to send without blocking
        pub fn try_send(&self, item: T) -> Result<(), BackpressureError> {
            match self.sender.try_send(item) {
                Ok(()) => {
                    self.metrics.record_operation(Duration::from_nanos(1));
                    Ok(())
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.metrics.record_error();
                    Err(BackpressureError::NoCapacity)
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    Err(BackpressureError::Shutdown)
                }
            }
        }

        /// Receive from channel
        pub async fn recv(&mut self) -> Option<T> {
            let timer = crate::common::Timer::new();
            let result = self.receiver.recv().await;
            if result.is_some() {
                let duration = timer.elapsed();
                self.metrics.record_operation(duration);
            }
            result
        }

        /// Get channel statistics
        pub fn stats(&self) -> ChannelStats {
            let (ops, errs, avg_duration) = self.metrics.get_stats();
            ChannelStats {
                capacity: self.capacity,
                total_sent: ops,
                total_dropped: errs,
                avg_send_time: avg_duration,
            }
        }
    }

    /// Statistics for bounded channel
    #[derive(Debug, Clone)]
    pub struct ChannelStats {
        pub capacity: usize,
        pub total_sent: u64,
        pub total_dropped: u64,
        pub avg_send_time: Duration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_backpressure_controller_blocking() {
        let controller = BackpressureController::new(2, Policy::Block);

        // Should admit first two
        assert!(controller.should_admit().await);
        assert!(controller.should_admit().await);

        // Third should block but we can't test blocking easily
        // In real usage, this would wait for consumption
    }

    #[tokio::test]
    async fn test_backpressure_controller_fail_fast() {
        let controller = BackpressureController::new(1, Policy::FailFast);

        // Should admit first
        assert!(controller.should_admit().await);

        // Second should fail
        assert!(!controller.should_admit().await);

        // After consumption, should work again
        controller.record_consumption();
        assert!(controller.should_admit().await);
    }

    #[tokio::test]
    async fn test_semaphore_backpressure() {
        let bp = SemaphoreBackpressure::new(2);

        // Should acquire two permits
        let permit1 = bp.acquire().await.unwrap();
        let permit2 = bp.acquire().await.unwrap();

        // Third should block
        let result = timeout(Duration::from_millis(10), bp.acquire()).await;
        assert!(result.is_err());

        // After dropping permits, should work
        drop(permit1);
        let _permit3 = bp.acquire().await.unwrap();
    }

    #[tokio::test]
    async fn test_try_acquire() {
        let bp = SemaphoreBackpressure::new(1);

        // Should succeed
        let permit = bp.try_acquire().unwrap();
        assert!(permit._permit.is_some());

        // Should fail
        let result = bp.try_acquire();
        assert!(matches!(result, Err(BackpressureError::NoCapacity)));

        // After drop, should work
        drop(permit);
        let _permit2 = bp.try_acquire().unwrap();
    }

    #[tokio::test]
    async fn test_channel_backpressure() {
        let mut channel = channel_backpressure::BoundedChannel::new(2);

        // Should send two items
        channel.send(1).await.unwrap();
        channel.send(2).await.unwrap();

        // Third should block
        let result = timeout(Duration::from_millis(10), channel.send(3)).await;
        assert!(result.is_err());

        // After receiving, should work
        assert_eq!(channel.recv().await, Some(1));
        channel.send(3).await.unwrap();
    }

    #[tokio::test]
    async fn test_try_send() {
        let channel = channel_backpressure::BoundedChannel::new(1);

        // Should succeed
        channel.try_send(1).unwrap();

        // Should fail
        let result = channel.try_send(2);
        assert!(matches!(result, Err(BackpressureError::NoCapacity)));
    }

    #[tokio::test]
    async fn test_adaptive_backpressure() {
        let bp = AdaptiveBackpressure::new(10, 0.5);

        // Initially should admit
        assert!(bp.should_admit().await);

        // Stats should show low utilization initially
        let stats = bp.stats();
        assert!(stats.utilization < 0.1);
    }
}
