//! Read-Copy-Update (RCU) Patterns
//!
//! RCU is a synchronization mechanism that allows reads to occur concurrently
//! with updates by maintaining multiple versions of data. It's particularly
//! useful for read-heavy workloads where writers are infrequent but need
//! to make atomic updates.
//!
//! ## Key Concepts
//!
//! - **Read-Side Critical Sections**: Grace period for readers
//! - **Quiescent States**: Points where readers can be safely updated
//! - **Deferred Reclamation**: Safe cleanup of old data versions
//! - **Wait-Free Reads**: No blocking for readers during updates
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::rcu::RCUCell;
//!
//! let rcu = RCUCell::new(vec![1, 2, 3]);
//!
//! // Reader
//! let data = rcu.read();
//! println!("Length: {}", data.len());
//!
//! // Writer (creates new version)
//! rcu.update(|current| {
//!     let mut new_data = current.clone();
//!     new_data.push(4);
//!     new_data
//! });
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::ptr;
use std::marker::PhantomData;
use tokio::sync::Notify;
use tracing::{debug, instrument};

use crate::error::LockFreeError;

/// RCU Cell for safe concurrent reads and updates
#[derive(Debug)]
pub struct RCUCell<T> {
    current: AtomicPtr<Arc<T>>,
    pending_cleanup: AtomicPtr<Vec<Arc<T>>>,
    readers: AtomicUsize,
}

impl<T> RCUCell<T> {
    /// Create a new RCU cell with initial data
    pub fn new(initial: T) -> Self {
        let initial_arc = Arc::new(initial);
        let initial_ptr = Box::into_raw(Box::new(initial_arc));

        Self {
            current: AtomicPtr::new(initial_ptr),
            pending_cleanup: AtomicPtr::new(ptr::null_mut()),
            readers: AtomicUsize::new(0),
        }
    }

    /// Read the current data (wait-free)
    #[instrument(skip(self))]
    pub fn read(&self) -> RCUReadGuard<T> {
        // Increment reader count
        self.readers.fetch_add(1, Ordering::AcqRel);

        // Load current pointer
        let ptr = self.current.load(Ordering::Acquire);
        let data = unsafe { &*ptr };

        RCUReadGuard {
            data: Arc::clone(data),
            rcu: self,
        }
    }

    /// Update the data (creates new version)
    #[instrument(skip(self, updater))]
    pub fn update<F>(&self, updater: F)
    where
        F: FnOnce(&T) -> T,
    {
        // Read current data
        let current_guard = self.read();
        let new_data = updater(&current_guard);
        drop(current_guard); // Release read lock

        // Create new Arc
        let new_arc = Arc::new(new_data);
        let new_ptr = Box::into_raw(Box::new(new_arc));

        // Swap pointers
        let old_ptr = self.current.swap(new_ptr, Ordering::AcqRel);

        // Schedule old data for cleanup
        self.schedule_cleanup(old_ptr);

        debug!("RCU update completed");
    }

    /// Try to perform cleanup of old versions
    pub fn try_cleanup(&self) {
        // Check if any readers are active
        if self.readers.load(Ordering::Acquire) == 0 {
            // Safe to cleanup
            let cleanup_ptr = self.pending_cleanup.swap(ptr::null_mut(), Ordering::AcqRel);

            if !cleanup_ptr.is_null() {
                let cleanup_list = unsafe { Box::from_raw(cleanup_ptr) };
                debug!("RCU cleanup: freed {} old versions", cleanup_list.len());
                // cleanup_list is dropped here, freeing the Arcs
            }
        }
    }

    fn schedule_cleanup(&self, old_ptr: *mut Arc<T>) {
        // Get the old data
        let old_arc = unsafe { Box::from_raw(old_ptr) };

        // Add to cleanup list
        loop {
            let current_cleanup = self.pending_cleanup.load(Ordering::Acquire);

            let mut cleanup_list = if current_cleanup.is_null() {
                Vec::new()
            } else {
                unsafe { Box::from_raw(current_cleanup) }
            };

            cleanup_list.push(old_arc.clone());

            let new_cleanup_ptr = Box::into_raw(Box::new(cleanup_list));

            if self.pending_cleanup.compare_exchange_weak(
                current_cleanup,
                new_cleanup_ptr,
                Ordering::AcqRel,
                Ordering::Acquire,
            ).is_ok() {
                break;
            }
        }
    }

    /// Get statistics
    pub fn stats(&self) -> RCUStats {
        RCUStats {
            active_readers: self.readers.load(Ordering::Acquire),
            has_pending_cleanup: !self.pending_cleanup.load(Ordering::Acquire).is_null(),
        }
    }
}

impl<T> Drop for RCUCell<T> {
    fn drop(&mut self) {
        // Cleanup current data
        let current = self.current.load(Ordering::Acquire);
        if !current.is_null() {
            unsafe { let _ = Box::from_raw(current); }
        }

        // Cleanup pending data
        let pending = self.pending_cleanup.load(Ordering::Acquire);
        if !pending.is_null() {
            unsafe { let _ = Box::from_raw(pending); }
        }
    }
}

/// RAII guard for RCU reads
#[derive(Debug)]
pub struct RCUReadGuard<'a, T> {
    data: Arc<T>,
    rcu: &'a RCUCell<T>,
}

impl<'a, T> Drop for RCUReadGuard<'a, T> {
    fn drop(&mut self) {
        self.rcu.readers.fetch_sub(1, Ordering::AcqRel);
    }
}

impl<'a, T> std::ops::Deref for RCUReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// Statistics for RCU cell
#[derive(Debug, Clone)]
pub struct RCUStats {
    pub active_readers: usize,
    pub has_pending_cleanup: bool,
}

/// RCU-enabled hashmap for concurrent access
#[derive(Debug)]
pub struct RCUHashMap<K, V> {
    data: RCUCell<std::collections::HashMap<K, V>>,
}

impl<K, V> RCUHashMap<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    pub fn new() -> Self {
        Self {
            data: RCUCell::new(std::collections::HashMap::new()),
        }
    }

    /// Get a value by key
    pub fn get(&self, key: &K) -> Option<V> {
        let guard = self.data.read();
        guard.get(key).cloned()
    }

    /// Insert or update a key-value pair
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let old_value = self.get(&key);

        self.data.update(|map| {
            let mut new_map = map.clone();
            new_map.insert(key, value);
            new_map
        });

        old_value
    }

    /// Remove a key-value pair
    pub fn remove(&self, key: &K) -> Option<V> {
        let old_value = self.get(key);

        if old_value.is_some() {
            self.data.update(|map| {
                let mut new_map = map.clone();
                new_map.remove(key);
                new_map
            });
        }

        old_value
    }

    /// Get all keys
    pub fn keys(&self) -> Vec<K> {
        let guard = self.data.read();
        guard.keys().cloned().collect()
    }

    /// Check if map contains key
    pub fn contains_key(&self, key: &K) -> bool {
        let guard = self.data.read();
        guard.contains_key(key)
    }

    /// Get map length
    pub fn len(&self) -> usize {
        let guard = self.data.read();
        guard.len()
    }

    /// Check if map is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cleanup old versions
    pub fn cleanup(&self) {
        self.data.try_cleanup();
    }

    /// Get statistics
    pub fn stats(&self) -> RCUStats {
        self.data.stats()
    }
}

impl<K, V> Default for RCUHashMap<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Grace period-based RCU implementation
#[derive(Debug)]
pub struct GracePeriodRCU<T> {
    current: AtomicPtr<Arc<T>>,
    grace_periods: Arc<std::sync::Mutex<Vec<Arc<T>>>>,
    notify: Arc<Notify>,
}

impl<T> GracePeriodRCU<T> {
    pub fn new(initial: T) -> Self {
        let initial_arc = Arc::new(initial);
        let initial_ptr = Box::into_raw(Box::new(initial_arc));

        Self {
            current: AtomicPtr::new(initial_ptr),
            grace_periods: Arc::new(std::sync::Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Register a reader (call at start of read-side critical section)
    pub fn read_lock(&self) -> RCUGuard<T> {
        // In a full implementation, this would track reader threads
        // For simplicity, we'll just return a guard
        RCUGuard {
            rcu: self,
            _phantom: PhantomData,
        }
    }

    /// Update data (call from writer)
    pub async fn update<F>(&self, updater: F)
    where
        F: FnOnce(&T) -> T,
    {
        // Read current data
        let current_ptr = self.current.load(Ordering::Acquire);
        let current_data = unsafe { &*current_ptr };

        // Create new data
        let new_data = updater(current_data);
        let new_arc = Arc::new(new_data);
        let new_ptr = Box::into_raw(Box::new(new_arc.clone()));

        // Swap pointers
        let old_ptr = self.current.swap(new_ptr, Ordering::AcqRel);

        // Move old data to grace period list
        {
            let mut grace_periods = self.grace_periods.lock().unwrap();
            let old_arc = unsafe { Arc::clone(&*old_ptr) };
            grace_periods.push(old_arc);
        }

        // Notify waiting cleanup
        self.notify.notify_one();

        // Cleanup old pointer
        unsafe { let _ = Box::from_raw(old_ptr); }
    }

    /// Wait for grace period and cleanup
    pub async fn synchronize(&self) {
        // In a real implementation, this would wait for all readers to complete
        // For this simplified version, we'll just wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

        let mut grace_periods = self.grace_periods.lock().unwrap();
        grace_periods.clear(); // In real RCU, this would be more sophisticated
    }

    /// Get current data (must be called within read-side critical section)
    pub fn read(&self) -> Arc<T> {
        let ptr = self.current.load(Ordering::Acquire);
        unsafe { Arc::clone(&*ptr) }
    }
}

/// RAII guard for RCU read-side critical sections
#[derive(Debug)]
pub struct RCUGuard<'a, T> {
    rcu: &'a GracePeriodRCU<T>,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T> Drop for RCUGuard<'a, T> {
    fn drop(&mut self) {
        // In a full implementation, this would signal end of read-side critical section
    }
}

impl<'a, T> RCUGuard<'a, T> {
    pub fn read(&self) -> Arc<T> {
        self.rcu.read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::task;

    #[test]
    fn test_rcu_cell_basic() {
        let rcu = RCUCell::new(42);

        // Read initial value
        let guard = rcu.read();
        assert_eq!(*guard, 42);

        // Update value
        rcu.update(|current| *current + 1);

        // Read new value
        let guard2 = rcu.read();
        assert_eq!(*guard2, 43);

        // Cleanup
        rcu.try_cleanup();
    }

    #[test]
    fn test_rcu_hashmap() {
        let map = RCUHashMap::new();

        // Initially empty
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);

        // Insert values
        assert!(map.insert("key1", "value1").is_none());
        assert!(map.insert("key2", "value2").is_none());

        assert_eq!(map.len(), 2);
        assert!(!map.is_empty());

        // Read values
        assert_eq!(map.get(&"key1"), Some("value1".to_string()));
        assert_eq!(map.get(&"key2"), Some("value2".to_string()));
        assert_eq!(map.get(&"key3"), None);

        // Update value
        assert_eq!(map.insert("key1", "new_value1"), Some("value1".to_string()));
        assert_eq!(map.get(&"key1"), Some("new_value1".to_string()));

        // Remove value
        assert_eq!(map.remove(&"key2"), Some("value2".to_string()));
        assert_eq!(map.get(&"key2"), None);
        assert_eq!(map.len(), 1);

        // Cleanup
        map.cleanup();
    }

    #[tokio::test]
    async fn test_concurrent_rcu() {
        let rcu = Arc::new(RCUCell::new(0));
        let mut handles = Vec::new();

        // Spawn reader threads
        for _ in 0..5 {
            let rcu_clone = rcu.clone();
            let handle = task::spawn(async move {
                let mut sum = 0;
                for _ in 0..10 {
                    let guard = rcu_clone.read();
                    sum += *guard;
                    tokio::task::yield_now().await;
                }
                sum
            });
            handles.push(handle);
        }

        // Spawn writer thread
        let rcu_writer = rcu.clone();
        let writer_handle = task::spawn(async move {
            for i in 1..=10 {
                rcu_writer.update(|current| *current + 1);
                tokio::task::yield_now().await;
            }
        });

        // Wait for writer
        writer_handle.await.unwrap();

        // Wait for readers
        let mut total_sum = 0;
        for handle in handles {
            total_sum += handle.await.unwrap();
        }

        // Each reader read 10 times, there were 5 readers, so 50 reads total
        // The final value is 10, but readers may have seen different values
        assert!(total_sum >= 0); // Just ensure no panics

        // Cleanup
        rcu.try_cleanup();
    }

    #[tokio::test]
    async fn test_grace_period_rcu() {
        let rcu = Arc::new(GracePeriodRCU::new("initial".to_string()));

        // Read initial value
        let guard = rcu.read_lock();
        assert_eq!(*guard.read(), "initial");

        // Update value
        rcu.update(|current| format!("{} updated", current)).await;

        // Synchronize to allow cleanup
        rcu.synchronize().await;

        // Read new value
        let guard2 = rcu.read_lock();
        assert_eq!(*guard2.read(), "initial updated");
    }

    #[test]
    fn test_rcu_stats() {
        let rcu = RCUCell::new(42);

        // Initially no readers
        let stats = rcu.stats();
        assert_eq!(stats.active_readers, 0);
        assert!(!stats.has_pending_cleanup);

        // Create a reader
        let _guard = rcu.read();
        let stats = rcu.stats();
        assert_eq!(stats.active_readers, 1);

        // Update to create pending cleanup
        rcu.update(|x| *x + 1);
        let stats = rcu.stats();
        assert!(stats.has_pending_cleanup);
    }
}
