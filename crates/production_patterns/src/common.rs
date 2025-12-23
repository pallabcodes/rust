//! Common types and utilities used across production patterns

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

/// Metrics counter for production patterns
#[derive(Debug, Default)]
pub struct Metrics {
    pub operations: AtomicU64,
    pub errors: AtomicU64,
    pub duration_total: AtomicU64, // nanoseconds
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_operation(&self, duration: Duration) {
        self.operations.fetch_add(1, Ordering::Relaxed);
        self.duration_total.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> (u64, u64, Duration) {
        let ops = self.operations.load(Ordering::Relaxed);
        let errs = self.errors.load(Ordering::Relaxed);
        let total_ns = self.duration_total.load(Ordering::Relaxed);
        let avg_duration = if ops > 0 {
            Duration::from_nanos(total_ns / ops)
        } else {
            Duration::from_nanos(0)
        };
        (ops, errs, avg_duration)
    }
}

/// Shutdown coordinator for graceful shutdown patterns
#[derive(Clone)]
pub struct ShutdownCoordinator {
    shutdown: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        Self {
            shutdown: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    pub async fn wait_shutdown(&self) {
        while !self.is_shutdown() {
            self.notify.notified().await;
        }
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer for measuring operation durations
pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

/// Exponential backoff calculator
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    initial: Duration,
    max: Duration,
    multiplier: f64,
}

impl ExponentialBackoff {
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max,
            multiplier: 2.0,
        }
    }

    pub fn duration(&self, attempt: u32) -> Duration {
        let base_duration = self.initial.as_millis() as f64;
        let multiplied = base_duration * self.multiplier.powi(attempt as i32);
        let capped = multiplied.min(self.max.as_millis() as f64);
        Duration::from_millis(capped as u64)
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self::new(Duration::from_millis(100), Duration::from_secs(30))
    }
}

/// Utility for bounded counters
#[derive(Debug)]
pub struct BoundedCounter {
    current: AtomicU64,
    max: u64,
}

impl BoundedCounter {
    pub fn new(max: u64) -> Self {
        Self {
            current: AtomicU64::new(0),
            max,
        }
    }

    pub fn increment(&self) -> bool {
        loop {
            let current = self.current.load(Ordering::Acquire);
            if current >= self.max {
                return false;
            }
            if self.current.compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                return true;
            }
        }
    }

    pub fn decrement(&self) {
        self.current.fetch_sub(1, Ordering::Release);
    }

    pub fn get(&self) -> u64 {
        self.current.load(Ordering::Acquire)
    }

    pub fn is_at_limit(&self) -> bool {
        self.current.load(Ordering::Acquire) >= self.max
    }
}
