//! Rate Limiting Patterns
//!
//! Rate limiters control the rate of operations to prevent resource exhaustion
//! and ensure fair usage. They are essential for API protection, resource
//! management, and preventing cascading failures in distributed systems.
//!
//! ## Key Concepts
//!
//! - **Token Bucket**: Smooth rate limiting with burst capacity
//! - **Leaky Bucket**: Constant rate with queue-based overflow
//! - **Fixed Window**: Simple time-based windows
//! - **Sliding Window**: Rolling time windows for smoother limiting
//! - **Adaptive Limiting**: Dynamic rate adjustment based on system load
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::rate_limiter::{RateLimiter, TokenBucket};
//!
//! let limiter = TokenBucket::new(100, 10); // 100 tokens, refill 10 per second
//!
//! if limiter.try_acquire().await {
//!     // Operation allowed
//!     perform_operation().await;
//! } else {
//!     // Rate limited
//!     return Err("rate limited");
//! }
//! ```

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::{interval, sleep};
use tracing::{debug, instrument};

use crate::common::Metrics;

/// Core rate limiter trait
#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    /// Try to acquire permission for an operation
    async fn try_acquire(&self) -> bool;

    /// Try to acquire multiple permissions at once
    async fn try_acquire_n(&self, n: u32) -> bool;

    /// Get current rate limiter statistics
    fn stats(&self) -> RateStats;

    /// Reset the rate limiter
    async fn reset(&self);
}

/// Token bucket rate limiter
#[derive(Debug)]
pub struct TokenBucket {
    capacity: u32,
    refill_rate: u32, // tokens per second
    tokens: Arc<Mutex<f64>>,
    last_refill: Arc<Mutex<Instant>>,
    metrics: Arc<Metrics>,
}

impl TokenBucket {
    /// Create a new token bucket rate limiter
    pub fn new(capacity: u32, refill_rate: u32) -> Self {
        Self {
            capacity,
            refill_rate,
            tokens: Arc::new(Mutex::new(capacity as f64)),
            last_refill: Arc::new(Mutex::new(Instant::now())),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Refill tokens based on elapsed time
    async fn refill(&self) {
        let mut tokens = self.tokens.lock().await;
        let mut last_refill = self.last_refill.lock().await;

        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill).as_secs_f64();
        let tokens_to_add = elapsed * self.refill_rate as f64;

        *tokens = (*tokens + tokens_to_add).min(self.capacity as f64);
        *last_refill = now;
    }
}

#[async_trait::async_trait]
impl RateLimiter for TokenBucket {
    #[instrument(skip(self))]
    async fn try_acquire(&self) -> bool {
        self.try_acquire_n(1).await
    }

    #[instrument(skip(self, n))]
    async fn try_acquire_n(&self, n: u32) -> bool {
        self.refill().await;

        let mut tokens = self.tokens.lock().await;
        if *tokens >= n as f64 {
            *tokens -= n as f64;
            self.metrics.record_operation(Duration::from_nanos(1));
            debug!("Token bucket: acquired {} tokens, remaining: {}", n, *tokens);
            true
        } else {
            self.metrics.record_error();
            debug!("Token bucket: insufficient tokens ({} < {})", *tokens, n);
            false
        }
    }

    fn stats(&self) -> RateStats {
        // Note: In async context, we'd need to make this async
        // For simplicity, returning approximate stats
        RateStats {
            limiter_type: "token_bucket",
            capacity: self.capacity as u64,
            available: 0, // Would need async access
            total_requests: self.metrics.0.load(std::sync::atomic::Ordering::Relaxed),
            total_rejected: self.metrics.2.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    async fn reset(&self) {
        *self.tokens.lock().await = self.capacity as f64;
        *self.last_refill.lock().await = Instant::now();
    }
}

/// Leaky bucket rate limiter
#[derive(Debug)]
pub struct LeakyBucket {
    capacity: u32,
    leak_rate: u32, // requests per second
    queue: Arc<Mutex<VecDeque<Instant>>>,
    metrics: Arc<Metrics>,
}

impl LeakyBucket {
    pub fn new(capacity: u32, leak_rate: u32) -> Self {
        Self {
            capacity,
            leak_rate,
            queue: Arc::new(Mutex::new(VecDeque::with_capacity(capacity as usize))),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Remove leaked requests from the bucket
    async fn leak(&self) {
        let mut queue = self.queue.lock().await;
        let now = Instant::now();
        let leak_interval = Duration::from_secs_f64(1.0 / self.leak_rate as f64);

        while let Some(&front) = queue.front() {
            if now.duration_since(front) >= leak_interval {
                queue.pop_front();
            } else {
                break;
            }
        }
    }
}

#[async_trait::async_trait]
impl RateLimiter for LeakyBucket {
    async fn try_acquire(&self) -> bool {
        self.leak().await;

        let mut queue = self.queue.lock().await;
        if queue.len() < self.capacity as usize {
            queue.push_back(Instant::now());
            self.metrics.record_operation(Duration::from_nanos(1));
            debug!("Leaky bucket: request accepted, queue size: {}", queue.len());
            true
        } else {
            self.metrics.record_error();
            debug!("Leaky bucket: request rejected, queue full");
            false
        }
    }

    async fn try_acquire_n(&self, n: u32) -> bool {
        self.leak().await;

        let mut queue = self.queue.lock().await;
        if queue.len() + n as usize <= self.capacity as usize {
            for _ in 0..n {
                queue.push_back(Instant::now());
            }
            self.metrics.record_operation(Duration::from_nanos(1));
            true
        } else {
            self.metrics.record_error();
            false
        }
    }

    fn stats(&self) -> RateStats {
        RateStats {
            limiter_type: "leaky_bucket",
            capacity: self.capacity as u64,
            available: 0, // Would need async access
            total_requests: self.metrics.0.load(std::sync::atomic::Ordering::Relaxed),
            total_rejected: self.metrics.2.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    async fn reset(&self) {
        self.queue.lock().await.clear();
    }
}

/// Fixed window rate limiter
#[derive(Debug)]
pub struct FixedWindow {
    capacity: u32,
    window_duration: Duration,
    current_window: Arc<Mutex<(Instant, u32)>>, // (window_start, request_count)
    metrics: Arc<Metrics>,
}

impl FixedWindow {
    pub fn new(capacity: u32, window_duration: Duration) -> Self {
        Self {
            capacity,
            window_duration,
            current_window: Arc::new(Mutex::new((Instant::now(), 0))),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Check if current window has expired and reset if needed
    async fn check_window(&self) {
        let mut window = self.current_window.lock().await;
        let now = Instant::now();

        if now.duration_since(window.0) >= self.window_duration {
            window.0 = now;
            window.1 = 0;
        }
    }
}

#[async_trait::async_trait]
impl RateLimiter for FixedWindow {
    async fn try_acquire(&self) -> bool {
        self.check_window().await;

        let mut window = self.current_window.lock().await;
        if window.1 < self.capacity {
            window.1 += 1;
            self.metrics.record_operation(Duration::from_nanos(1));
            debug!("Fixed window: request accepted, count: {}", window.1);
            true
        } else {
            self.metrics.record_error();
            debug!("Fixed window: request rejected, limit reached");
            false
        }
    }

    async fn try_acquire_n(&self, n: u32) -> bool {
        self.check_window().await;

        let mut window = self.current_window.lock().await;
        if window.1 + n <= self.capacity {
            window.1 += n;
            self.metrics.record_operation(Duration::from_nanos(1));
            true
        } else {
            self.metrics.record_error();
            false
        }
    }

    fn stats(&self) -> RateStats {
        RateStats {
            limiter_type: "fixed_window",
            capacity: self.capacity as u64,
            available: 0,
            total_requests: self.metrics.0.load(std::sync::atomic::Ordering::Relaxed),
            total_rejected: self.metrics.2.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    async fn reset(&self) {
        *self.current_window.lock().await = (Instant::now(), 0);
    }
}

/// Sliding window rate limiter
#[derive(Debug)]
pub struct SlidingWindow {
    capacity: u32,
    window_duration: Duration,
    requests: Arc<Mutex<VecDeque<Instant>>>,
    metrics: Arc<Metrics>,
}

impl SlidingWindow {
    pub fn new(capacity: u32, window_duration: Duration) -> Self {
        Self {
            capacity,
            window_duration,
            requests: Arc::new(Mutex::new(VecDeque::with_capacity(capacity as usize))),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Remove requests outside the sliding window
    async fn cleanup_old_requests(&self) {
        let mut requests = self.requests.lock().await;
        let cutoff = Instant::now() - self.window_duration;

        while let Some(&front) = requests.front() {
            if front < cutoff {
                requests.pop_front();
            } else {
                break;
            }
        }
    }
}

#[async_trait::async_trait]
impl RateLimiter for SlidingWindow {
    async fn try_acquire(&self) -> bool {
        self.cleanup_old_requests().await;

        let mut requests = self.requests.lock().await;
        if requests.len() < self.capacity as usize {
            requests.push_back(Instant::now());
            self.metrics.record_operation(Duration::from_nanos(1));
            debug!("Sliding window: request accepted, count: {}", requests.len());
            true
        } else {
            self.metrics.record_error();
            debug!("Sliding window: request rejected, window full");
            false
        }
    }

    async fn try_acquire_n(&self, n: u32) -> bool {
        self.cleanup_old_requests().await;

        let mut requests = self.requests.lock().await;
        if requests.len() + n as usize <= self.capacity as usize {
            for _ in 0..n {
                requests.push_back(Instant::now());
            }
            self.metrics.record_operation(Duration::from_nanos(1));
            true
        } else {
            self.metrics.record_error();
            false
        }
    }

    fn stats(&self) -> RateStats {
        RateStats {
            limiter_type: "sliding_window",
            capacity: self.capacity as u64,
            available: 0,
            total_requests: self.metrics.0.load(std::sync::atomic::Ordering::Relaxed),
            total_rejected: self.metrics.2.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    async fn reset(&self) {
        self.requests.lock().await.clear();
    }
}

/// Rate limiter statistics
#[derive(Debug, Clone)]
pub struct RateStats {
    pub limiter_type: &'static str,
    pub capacity: u64,
    pub available: u64,
    pub total_requests: u64,
    pub total_rejected: u64,
}

/// Rate limiter registry for managing multiple limiters
#[derive(Debug, Default)]
pub struct RateLimiterRegistry {
    limiters: Arc<Mutex<std::collections::HashMap<String, Box<dyn RateLimiter>>>>,
}

impl RateLimiterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(&self, name: String, limiter: Box<dyn RateLimiter>) {
        let mut limiters = self.limiters.lock().await;
        limiters.insert(name, limiter);
    }

    pub async fn get(&self, name: &str) -> Option<Box<dyn RateLimiter>> {
        let limiters = self.limiters.lock().await;
        limiters.get(name).map(|l| dyn_clone::clone_box(l.as_ref()))
    }

    pub async fn try_acquire(&self, name: &str) -> bool {
        if let Some(limiter) = self.get(name).await {
            limiter.try_acquire().await
        } else {
            false
        }
    }
}

// Since we can't easily clone trait objects, we'll use a simpler approach
impl Clone for Box<dyn RateLimiter> {
    fn clone(&self) -> Self {
        // This is a limitation - we can't easily clone trait objects
        // In production, you'd use an enum or different approach
        panic!("RateLimiter trait objects cannot be cloned")
    }
}

/// Adaptive rate limiter that adjusts based on success/failure rates
#[derive(Debug)]
pub struct AdaptiveRateLimiter {
    base_limiter: Box<dyn RateLimiter>,
    success_threshold: f64,
    failure_threshold: f64,
    adjustment_factor: f64,
    recent_requests: Arc<Mutex<VecDeque<bool>>>, // true = success, false = failure
    window_size: usize,
}

impl AdaptiveRateLimiter {
    pub fn new(
        base_limiter: Box<dyn RateLimiter>,
        success_threshold: f64,
        failure_threshold: f64,
        window_size: usize,
    ) -> Self {
        Self {
            base_limiter,
            success_threshold,
            failure_threshold,
            adjustment_factor: 0.1,
            recent_requests: Arc::new(Mutex::new(VecDeque::with_capacity(window_size))),
            window_size,
        }
    }

    /// Calculate success rate from recent requests
    async fn success_rate(&self) -> f64 {
        let requests = self.recent_requests.lock().await;
        if requests.is_empty() {
            return 1.0;
        }

        let successes = requests.iter().filter(|&&success| success).count();
        successes as f64 / requests.len() as f64
    }

    pub async fn record_result(&self, success: bool) {
        let mut requests = self.recent_requests.lock().await;
        requests.push_back(success);
        if requests.len() > self.window_size {
            requests.pop_front();
        }
    }
}

#[async_trait::async_trait]
impl RateLimiter for AdaptiveRateLimiter {
    async fn try_acquire(&self) -> bool {
        // For simplicity, delegate to base limiter
        // In production, you might adjust capacity based on success rate
        self.base_limiter.try_acquire().await
    }

    async fn try_acquire_n(&self, n: u32) -> bool {
        self.base_limiter.try_acquire_n(n).await
    }

    fn stats(&self) -> RateStats {
        self.base_limiter.stats()
    }

    async fn reset(&self) {
        self.base_limiter.reset().await;
        self.recent_requests.lock().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let limiter = TokenBucket::new(10, 10); // 10 tokens, refill 10/sec

        // Should allow initial burst
        for _ in 0..10 {
            assert!(limiter.try_acquire().await);
        }

        // Should reject additional requests
        assert!(!limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let limiter = TokenBucket::new(10, 10);

        // Exhaust tokens
        for _ in 0..10 {
            assert!(limiter.try_acquire().await);
        }

        // Wait for refill
        tokio::time::sleep(Duration::from_millis(600)).await; // Should refill ~6 tokens

        // Should allow some requests
        let mut allowed = 0;
        for _ in 0..10 {
            if limiter.try_acquire().await {
                allowed += 1;
            }
        }
        assert!(allowed > 0 && allowed < 10); // Partial refill
    }

    #[tokio::test]
    async fn test_leaky_bucket() {
        let limiter = LeakyBucket::new(5, 2); // 5 capacity, leak 2/sec

        // Should allow up to capacity
        for i in 0..5 {
            assert!(limiter.try_acquire().await, "Request {} should be allowed", i);
        }

        // Should reject additional
        assert!(!limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn test_fixed_window() {
        let limiter = FixedWindow::new(3, Duration::from_millis(100));

        // Should allow 3 requests immediately
        for i in 0..3 {
            assert!(limiter.try_acquire().await, "Request {} should be allowed", i);
        }

        // Should reject 4th request
        assert!(!limiter.try_acquire().await);

        // Wait for window to reset
        tokio::time::sleep(Duration::from_millis(110)).await;

        // Should allow requests in new window
        assert!(limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn test_sliding_window() {
        let limiter = SlidingWindow::new(3, Duration::from_millis(200));

        // Should allow 3 requests
        for i in 0..3 {
            assert!(limiter.try_acquire().await, "Request {} should be allowed", i);
        }

        // Should reject 4th
        assert!(!limiter.try_acquire().await);

        // Wait for some requests to age out
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should allow 1 more request
        assert!(limiter.try_acquire().await);
        assert!(!limiter.try_acquire().await); // But not 2 more
    }

    #[tokio::test]
    async fn test_try_acquire_n() {
        let limiter = TokenBucket::new(10, 1);

        // Should allow acquiring multiple tokens
        assert!(limiter.try_acquire_n(5).await);
        assert!(limiter.try_acquire_n(3).await);
        assert!(!limiter.try_acquire_n(3).await); // Only 2 left
    }

    #[tokio::test]
    async fn test_registry() {
        let registry = RateLimiterRegistry::new();

        let limiter = Box::new(TokenBucket::new(5, 1));
        registry.register("test".to_string(), limiter).await;

        // Should find limiter
        assert!(registry.try_acquire("test").await);

        // Should return false for unknown limiter
        assert!(!registry.try_acquire("unknown").await);
    }

    #[tokio::test]
    async fn test_adaptive_limiter() {
        let base_limiter = Box::new(TokenBucket::new(10, 1));
        let adaptive = AdaptiveRateLimiter::new(base_limiter, 0.8, 0.2, 10);

        // Should delegate to base limiter
        assert!(adaptive.try_acquire().await);

        // Record some results
        adaptive.record_result(true).await;
        adaptive.record_result(false).await;

        // Success rate should be 50%
        let rate = adaptive.success_rate().await;
        assert!((rate - 0.5).abs() < 0.01);
    }
}
