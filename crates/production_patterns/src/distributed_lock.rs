//! Distributed Lock Pattern
//!
//! Distributed locks coordinate access to shared resources across multiple
//! processes or nodes. They prevent race conditions and ensure mutual exclusion
//! in distributed systems, commonly used for leader election, resource allocation,
//! and critical section protection.
//!
//! ## Key Concepts
//!
//! - **TTL-based Locks**: Locks expire automatically to prevent deadlocks
//! - **Non-blocking Acquisition**: Try-lock operations for immediate feedback
//! - **Renewal**: Extend lock lifetime for long-running operations
//! - **Ownership Verification**: Ensure lock holder identity
//! - **Fencing Tokens**: Prevent stale lock effects
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::distributed_lock::{DistributedLock, LockConfig};
//!
//! let lock = DistributedLock::new(LockConfig {
//!     name: "resource-lock".to_string(),
//!     ttl: Duration::from_secs(30),
//!     retry_attempts: 3,
//! });
//!
//! // Try to acquire lock
//! if let Ok(token) = lock.try_acquire().await {
//!     // Critical section
//!     do_work().await;
//!
//!     // Release lock
//!     lock.release(token).await;
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{timeout, sleep};
use tracing::{debug, error, info, instrument, warn};

use crate::common::Metrics;
use crate::error::DistributedLockError;

/// Lock token for ownership verification
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LockToken {
    pub id: String,
    pub owner: String,
    pub acquired_at: Instant,
    pub ttl: Duration,
}

impl LockToken {
    pub fn new(owner: String, ttl: Duration) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            owner,
            acquired_at: Instant::now(),
            ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.acquired_at.elapsed() >= self.ttl
    }

    pub fn time_remaining(&self) -> Duration {
        if self.is_expired() {
            Duration::from_secs(0)
        } else {
            self.ttl - self.acquired_at.elapsed()
        }
    }
}

/// Configuration for distributed lock
#[derive(Debug, Clone)]
pub struct LockConfig {
    pub name: String,
    pub ttl: Duration,
    pub retry_attempts: u32,
    pub retry_delay: Duration,
    pub owner_id: Option<String>,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            name: "default-lock".to_string(),
            ttl: Duration::from_secs(30),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
            owner_id: None,
        }
    }
}

/// In-memory distributed lock implementation
/// (In production, this would be backed by Redis, etcd, ZooKeeper, etc.)
#[derive(Debug)]
pub struct DistributedLock {
    config: LockConfig,
    locks: Arc<RwLock<HashMap<String, LockToken>>>,
    metrics: Arc<Metrics>,
}

impl DistributedLock {
    /// Create a new distributed lock
    pub fn new(config: LockConfig) -> Self {
        Self {
            config,
            locks: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Try to acquire the lock immediately
    #[instrument(skip(self), fields(lock_name = %self.config.name))]
    pub async fn try_acquire(&self) -> Result<LockToken, DistributedLockError> {
        let timer = crate::common::Timer::new();
        let owner = self.owner_id().to_string();

        let mut locks = self.locks.write().await;

        // Check if lock is already held
        if let Some(existing) = locks.get(&self.config.name) {
            if !existing.is_expired() && existing.owner != owner {
                self.metrics.record_error();
                return Err(DistributedLockError::AlreadyHeld);
            }

            // Lock is expired or owned by us, we can take it
            if existing.is_expired() {
                warn!("Acquiring expired lock owned by {}", existing.owner);
            }
        }

        // Acquire the lock
        let token = LockToken::new(owner, self.config.ttl);
        locks.insert(self.config.name.clone(), token.clone());

        let duration = timer.elapsed();
        self.metrics.record_operation(duration);

        debug!("Lock acquired by {}", token.owner);
        Ok(token)
    }

    /// Acquire the lock with retries
    #[instrument(skip(self), fields(lock_name = %self.config.name))]
    pub async fn acquire(&self) -> Result<LockToken, DistributedLockError> {
        let timer = crate::common::Timer::new();

        for attempt in 0..self.config.retry_attempts {
            match self.try_acquire().await {
                Ok(token) => {
                    let duration = timer.elapsed();
                    self.metrics.record_operation(duration);
                    return Ok(token);
                }
                Err(DistributedLockError::AlreadyHeld) => {
                    if attempt + 1 < self.config.retry_attempts {
                        debug!("Lock acquisition attempt {} failed, retrying in {:?}", attempt + 1, self.config.retry_delay);
                        sleep(self.config.retry_delay).await;
                    }
                }
                Err(e) => {
                    self.metrics.record_error();
                    return Err(e);
                }
            }
        }

        self.metrics.record_error();
        Err(DistributedLockError::AcquisitionFailed)
    }

    /// Acquire lock with timeout
    #[instrument(skip(self, timeout_dur), fields(lock_name = %self.config.name))]
    pub async fn acquire_with_timeout(&self, timeout_dur: Duration) -> Result<LockToken, DistributedLockError> {
        match timeout(timeout_dur, self.acquire()).await {
            Ok(result) => result,
            Err(_) => Err(DistributedLockError::AcquisitionFailed),
        }
    }

    /// Release the lock
    #[instrument(skip(self, token), fields(lock_name = %self.config.name))]
    pub async fn release(&self, token: LockToken) -> Result<(), DistributedLockError> {
        let timer = crate::common::Timer::new();

        let mut locks = self.locks.write().await;

        if let Some(existing) = locks.get(&self.config.name) {
            if existing.id != token.id {
                // Lock has been taken by someone else
                self.metrics.record_error();
                return Err(DistributedLockError::LockExpired);
            }

            if existing.owner != token.owner {
                // Ownership mismatch
                self.metrics.record_error();
                return Err(DistributedLockError::LockExpired);
            }

            locks.remove(&self.config.name);
            let duration = timer.elapsed();
            self.metrics.record_operation(duration);

            debug!("Lock released by {}", token.owner);
            Ok(())
        } else {
            // Lock doesn't exist
            Err(DistributedLockError::LockExpired)
        }
    }

    /// Renew the lock (extend TTL)
    #[instrument(skip(self, token), fields(lock_name = %self.config.name))]
    pub async fn renew(&self, token: LockToken) -> Result<LockToken, DistributedLockError> {
        let timer = crate::common::Timer::new();

        let mut locks = self.locks.write().await;

        if let Some(existing) = locks.get(&self.config.name) {
            if existing.id != token.id || existing.owner != token.owner {
                self.metrics.record_error();
                return Err(DistributedLockError::LockExpired);
            }

            // Renew the lock
            let new_token = LockToken::new(token.owner.clone(), self.config.ttl);
            locks.insert(self.config.name.clone(), new_token.clone());

            let duration = timer.elapsed();
            self.metrics.record_operation(duration);

            debug!("Lock renewed by {}", token.owner);
            Ok(new_token)
        } else {
            Err(DistributedLockError::LockExpired)
        }
    }

    /// Check if lock is currently held
    pub async fn is_locked(&self) -> bool {
        let locks = self.locks.read().await;
        locks.get(&self.config.name)
            .map(|token| !token.is_expired())
            .unwrap_or(false)
    }

    /// Get current lock holder information
    pub async fn lock_info(&self) -> Option<LockInfo> {
        let locks = self.locks.read().await;
        locks.get(&self.config.name).map(|token| LockInfo {
            owner: token.owner.clone(),
            acquired_at: token.acquired_at,
            ttl: token.ttl,
            time_remaining: token.time_remaining(),
            is_expired: token.is_expired(),
        })
    }

    /// Force release expired locks (maintenance operation)
    #[instrument(skip(self), fields(lock_name = %self.config.name))]
    pub async fn cleanup_expired(&self) -> usize {
        let mut locks = self.locks.write().await;
        let mut cleaned = 0;

        // Remove expired locks
        locks.retain(|name, token| {
            if token.is_expired() {
                warn!("Cleaning up expired lock {} owned by {}", name, token.owner);
                cleaned += 1;
                false
            } else {
                true
            }
        });

        cleaned
    }

    /// Get lock statistics
    pub fn stats(&self) -> LockStats {
        let (total_operations, total_errors, avg_duration) = self.metrics.get_stats();
        LockStats {
            name: self.config.name.clone(),
            total_operations,
            total_errors,
            avg_operation_time: avg_duration,
            is_locked: futures::executor::block_on(self.is_locked()),
        }
    }

    fn owner_id(&self) -> &str {
        self.config.owner_id.as_deref()
            .unwrap_or(&format!("process-{}", std::process::id()))
    }
}

/// Lock information
#[derive(Debug, Clone)]
pub struct LockInfo {
    pub owner: String,
    pub acquired_at: Instant,
    pub ttl: Duration,
    pub time_remaining: Duration,
    pub is_expired: bool,
}

/// Lock statistics
#[derive(Debug, Clone)]
pub struct LockStats {
    pub name: String,
    pub total_operations: u64,
    pub total_errors: u64,
    pub avg_operation_time: Duration,
    pub is_locked: bool,
}

/// RAII lock guard for automatic release
#[derive(Debug)]
pub struct LockGuard<'a> {
    lock: &'a DistributedLock,
    token: Option<LockToken>,
}

impl<'a> LockGuard<'a> {
    pub async fn new(lock: &'a DistributedLock) -> Result<Self, DistributedLockError> {
        let token = lock.acquire().await?;
        Ok(Self {
            lock,
            token: Some(token),
        })
    }

    pub fn token(&self) -> &LockToken {
        self.token.as_ref().unwrap()
    }
}

impl<'a> Drop for LockGuard<'a> {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            // Spawn a task to release the lock asynchronously
            // This prevents blocking the drop handler
            let lock = self.lock;
            tokio::spawn(async move {
                if let Err(e) = lock.release(token).await {
                    error!("Failed to release lock in guard drop: {}", e);
                }
            });
        }
    }
}

/// Distributed lock registry for managing multiple locks
#[derive(Debug, Default)]
pub struct LockRegistry {
    locks: Arc<Mutex<HashMap<String, Arc<DistributedLock>>>>,
}

impl LockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a lock
    pub async fn register(&self, lock: Arc<DistributedLock>) {
        let mut locks = self.locks.lock().await;
        locks.insert(lock.config.name.clone(), lock);
    }

    /// Get a lock by name
    pub async fn get(&self, name: &str) -> Option<Arc<DistributedLock>> {
        let locks = self.locks.lock().await;
        locks.get(name).cloned()
    }

    /// Get all lock statistics
    pub async fn all_stats(&self) -> HashMap<String, LockStats> {
        let locks = self.locks.lock().await;
        let mut stats = HashMap::new();

        for (name, lock) in locks.iter() {
            stats.insert(name.clone(), lock.stats());
        }

        stats
    }

    /// Cleanup expired locks across all registered locks
    pub async fn cleanup_all_expired(&self) -> HashMap<String, usize> {
        let locks = self.locks.lock().await;
        let mut results = HashMap::new();

        for (name, lock) in locks.iter() {
            let cleaned = lock.cleanup_expired().await;
            if cleaned > 0 {
                results.insert(name.clone(), cleaned);
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_lock_acquisition() {
        let lock = DistributedLock::new(LockConfig {
            name: "test-lock".to_string(),
            ttl: Duration::from_secs(5),
            retry_attempts: 1,
            retry_delay: Duration::from_millis(10),
            owner_id: Some("owner1".to_string()),
        });

        // Should acquire successfully
        let token = lock.try_acquire().await.unwrap();
        assert_eq!(token.owner, "owner1");
        assert!(!token.is_expired());

        // Should be locked
        assert!(lock.is_locked().await);

        // Release
        lock.release(token).await.unwrap();
        assert!(!lock.is_locked().await);
    }

    #[tokio::test]
    async fn test_lock_contention() {
        let lock1 = DistributedLock::new(LockConfig {
            name: "shared-lock".to_string(),
            ttl: Duration::from_secs(5),
            retry_attempts: 1,
            retry_delay: Duration::from_millis(10),
            owner_id: Some("owner1".to_string()),
        });

        let lock2 = DistributedLock::new(LockConfig {
            name: "shared-lock".to_string(),
            ttl: Duration::from_secs(5),
            retry_attempts: 1,
            retry_delay: Duration::from_millis(10),
            owner_id: Some("owner2".to_string()),
        });

        // First owner acquires
        let token1 = lock1.try_acquire().await.unwrap();

        // Second owner should fail
        let result = lock2.try_acquire().await;
        assert!(matches!(result, Err(DistributedLockError::AlreadyHeld)));

        // Release first lock
        lock1.release(token1).await.unwrap();

        // Second owner can now acquire
        let token2 = lock2.try_acquire().await.unwrap();
        assert_eq!(token2.owner, "owner2");
    }

    #[tokio::test]
    async fn test_lock_renewal() {
        let lock = DistributedLock::new(LockConfig {
            name: "renew-test".to_string(),
            ttl: Duration::from_millis(100),
            retry_attempts: 1,
            retry_delay: Duration::from_millis(10),
            owner_id: Some("owner".to_string()),
        });

        let token = lock.acquire().await.unwrap();
        let original_time = token.acquired_at;

        // Wait a bit
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Renew
        let new_token = lock.renew(token).await.unwrap();
        assert!(new_token.acquired_at > original_time);
    }

    #[tokio::test]
    async fn test_lock_expiry() {
        let lock = DistributedLock::new(LockConfig {
            name: "expiry-test".to_string(),
            ttl: Duration::from_millis(50),
            retry_attempts: 1,
            retry_delay: Duration::from_millis(10),
            owner_id: Some("owner".to_string()),
        });

        let token = lock.acquire().await.unwrap();

        // Wait for expiry
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Token should be expired
        assert!(token.is_expired());

        // Should be able to acquire new lock
        let new_token = lock.try_acquire().await.unwrap();
        assert!(!new_token.is_expired());
    }

    #[tokio::test]
    async fn test_lock_guard() {
        let lock = DistributedLock::new(LockConfig {
            name: "guard-test".to_string(),
            ttl: Duration::from_secs(5),
            retry_attempts: 1,
            retry_delay: Duration::from_millis(10),
            owner_id: Some("owner".to_string()),
        });

        {
            let guard = LockGuard::new(&lock).await.unwrap();
            assert!(lock.is_locked().await);

            // Guard goes out of scope, lock should be released
        }

        // Small delay to allow async release to complete
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!lock.is_locked().await);
    }

    #[tokio::test]
    async fn test_acquire_with_timeout() {
        let lock1 = DistributedLock::new(LockConfig {
            name: "timeout-test".to_string(),
            ttl: Duration::from_secs(5),
            retry_attempts: 10, // High retry count
            retry_delay: Duration::from_millis(100), // Slow retry
            owner_id: Some("owner1".to_string()),
        });

        let lock2 = DistributedLock::new(LockConfig {
            name: "timeout-test".to_string(),
            ttl: Duration::from_secs(5),
            retry_attempts: 1,
            retry_delay: Duration::from_millis(10),
            owner_id: Some("owner2".to_string()),
        });

        // Hold lock with first owner
        let _token1 = lock1.acquire().await.unwrap();

        // Second owner tries with short timeout
        let result = lock2.acquire_with_timeout(Duration::from_millis(50)).await;
        assert!(matches!(result, Err(DistributedLockError::AcquisitionFailed)));
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let lock = DistributedLock::new(LockConfig {
            name: "cleanup-test".to_string(),
            ttl: Duration::from_millis(50),
            retry_attempts: 1,
            retry_delay: Duration::from_millis(10),
            owner_id: Some("owner".to_string()),
        });

        let _token = lock.acquire().await.unwrap();

        // Wait for expiry
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Cleanup should remove expired lock
        let cleaned = lock.cleanup_expired().await;
        assert_eq!(cleaned, 1);

        // Should be able to acquire new lock
        let _new_token = lock.try_acquire().await.unwrap();
    }

    #[tokio::test]
    async fn test_registry() {
        let registry = LockRegistry::new();

        let lock = Arc::new(DistributedLock::new(LockConfig {
            name: "registry-test".to_string(),
            ttl: Duration::from_secs(30),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
            owner_id: Some("owner".to_string()),
        }));

        registry.register(lock.clone()).await;

        let retrieved = registry.get("registry-test").await.unwrap();
        assert_eq!(retrieved.config.name, "registry-test");

        let stats = registry.all_stats().await;
        assert_eq!(stats.len(), 1);
        assert!(stats.contains_key("registry-test"));
    }
}
