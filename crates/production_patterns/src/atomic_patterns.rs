//! Atomic Operations Patterns
//!
//! Atomic operations provide lock-free, thread-safe access to shared data.
//! Understanding memory ordering and atomic patterns is crucial for writing
//! correct concurrent code and implementing high-performance synchronization.
//!
//! ## Key Concepts
//!
//! - **Memory Ordering**: Relaxed, Acquire, Release, AcqRel, SeqCst
//! - **Atomic Types**: AtomicUsize, AtomicBool, AtomicPtr, etc.
//! - **CAS Loops**: Building complex operations from compare-and-swap
//! - **Memory Fences**: Compiler and hardware barrier operations
//! - **Sequential Consistency**: Ensuring total order of operations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::atomic_patterns::{AtomicCounter, AtomicFlag};
//!
//! let counter = AtomicCounter::new(0);
//! counter.increment();
//! assert_eq!(counter.get(), 1);
//!
//! let flag = AtomicFlag::new();
//! flag.set();
//! assert!(flag.is_set());
//! ```

use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicPtr, Ordering};
use std::sync::Arc;
use std::ptr;
use tracing::{debug, instrument};

use crate::common::Metrics;

/// High-performance atomic counter with memory ordering control
#[derive(Debug)]
pub struct AtomicCounter {
    value: AtomicUsize,
    metrics: Metrics,
}

impl AtomicCounter {
    pub fn new(initial: usize) -> Self {
        Self {
            value: AtomicUsize::new(initial),
            metrics: Metrics::new(),
        }
    }

    /// Increment and return new value
    #[instrument]
    pub fn increment(&self) -> usize {
        let result = self.value.fetch_add(1, Ordering::AcqRel) + 1;
        self.metrics.record_operation(std::time::Duration::from_nanos(1));
        result
    }

    /// Decrement and return new value
    #[instrument]
    pub fn decrement(&self) -> usize {
        let result = self.value.fetch_sub(1, Ordering::AcqRel) - 1;
        self.metrics.record_operation(std::time::Duration::from_nanos(1));
        result
    }

    /// Add value and return previous value
    pub fn add(&self, delta: usize) -> usize {
        let result = self.value.fetch_add(delta, Ordering::AcqRel);
        self.metrics.record_operation(std::time::Duration::from_nanos(1));
        result
    }

    /// Get current value
    pub fn get(&self) -> usize {
        self.value.load(Ordering::Acquire)
    }

    /// Set new value and return old value
    pub fn set(&self, new_value: usize) -> usize {
        let result = self.value.swap(new_value, Ordering::AcqRel);
        self.metrics.record_operation(std::time::Duration::from_nanos(1));
        result
    }

    /// Compare and exchange
    pub fn compare_exchange(&self, current: usize, new: usize) -> Result<usize, usize> {
        match self.value.compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire) {
            Ok(prev) => {
                self.metrics.record_operation(std::time::Duration::from_nanos(1));
                Ok(prev)
            }
            Err(actual) => {
                self.metrics.record_error();
                Err(actual)
            }
        }
    }

    /// Get statistics
    pub fn stats(&self) -> CounterStats {
        let (ops, errs, avg_duration) = self.metrics.get_stats();
        CounterStats {
            value: self.get(),
            total_operations: ops,
            failed_operations: errs,
            avg_operation_time: avg_duration,
        }
    }
}

impl Default for AtomicCounter {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Statistics for atomic counter
#[derive(Debug, Clone)]
pub struct CounterStats {
    pub value: usize,
    pub total_operations: u64,
    pub failed_operations: u64,
    pub avg_operation_time: std::time::Duration,
}

/// Atomic flag for thread-safe boolean operations
#[derive(Debug)]
pub struct AtomicFlag {
    flag: AtomicBool,
}

impl AtomicFlag {
    pub fn new() -> Self {
        Self {
            flag: AtomicBool::new(false),
        }
    }

    /// Set the flag (returns true if it was already set)
    pub fn set(&self) -> bool {
        self.flag.swap(true, Ordering::AcqRel)
    }

    /// Clear the flag (returns true if it was set)
    pub fn clear(&self) -> bool {
        self.flag.swap(false, Ordering::AcqRel)
    }

    /// Check if flag is set
    pub fn is_set(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// Set only if not already set (returns true if successfully set)
    pub fn test_and_set(&self) -> bool {
        self.flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    /// Clear only if set (returns true if successfully cleared)
    pub fn test_and_clear(&self) -> bool {
        self.flag.compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }
}

impl Default for AtomicFlag {
    fn default() -> Self {
        Self::new()
    }
}

/// Spin lock using atomic operations
#[derive(Debug)]
pub struct SpinLock<T> {
    locked: AtomicBool,
    data: std::sync::UnsafeCell<T>,
}

impl<T> SpinLock<T> {
    pub fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: std::sync::UnsafeCell::new(data),
        }
    }

    /// Acquire the lock by spinning
    pub fn lock(&self) -> SpinLockGuard<T> {
        while !self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            std::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }

    /// Try to acquire the lock without spinning
    pub fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(SpinLockGuard { lock: self })
        } else {
            None
        }
    }

    /// Check if locked
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send> Sync for SpinLock<T> {}

/// RAII guard for spin lock
pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

impl<'a, T> std::ops::Deref for SpinLockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> std::ops::DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

/// Sequence lock for optimistic reading
#[derive(Debug)]
pub struct SeqLock<T> {
    sequence: AtomicUsize,
    data: std::sync::UnsafeCell<T>,
}

impl<T> SeqLock<T> {
    pub fn new(data: T) -> Self {
        Self {
            sequence: AtomicUsize::new(0),
            data: std::sync::UnsafeCell::new(data),
        }
    }

    /// Write to the sequence lock
    pub fn write(&self, new_data: T) {
        // Increment sequence to odd (writing)
        let seq = self.sequence.fetch_add(1, Ordering::Release);

        // Write data
        unsafe { *self.data.get() = new_data; }

        // Increment sequence to even (done writing)
        self.sequence.fetch_add(1, Ordering::Release);
    }

    /// Try to read from the sequence lock
    pub fn try_read(&self) -> Option<T>
    where
        T: Clone,
    {
        loop {
            // Read sequence
            let seq1 = self.sequence.load(Ordering::Acquire);

            // If odd, someone is writing
            if seq1 % 2 != 0 {
                return None;
            }

            // Read data
            let data = unsafe { (*self.data.get()).clone() };

            // Read sequence again
            let seq2 = self.sequence.load(Ordering::Acquire);

            // If sequences match and even, we got a consistent read
            if seq1 == seq2 {
                return Some(data);
            }

            // Sequence changed, try again
        }
    }

    /// Read with spin waiting
    pub fn read(&self) -> T
    where
        T: Clone,
    {
        loop {
            if let Some(data) = self.try_read() {
                return data;
            }
            std::hint::spin_loop();
        }
    }
}

unsafe impl<T: Send> Send for SeqLock<T> {}
unsafe impl<T: Send + Sync> Sync for SeqLock<T> {}

/// Atomic pointer utilities for lock-free programming
pub mod atomic_ptr_utils {
    use super::*;

    /// Atomic swap operation
    pub fn atomic_swap<T>(ptr: &AtomicPtr<T>, new: *mut T) -> *mut T {
        ptr.swap(new, Ordering::AcqRel)
    }

    /// Atomic load with null check
    pub fn load_checked<T>(ptr: &AtomicPtr<T>) -> Option<*mut T> {
        let loaded = ptr.load(Ordering::Acquire);
        if loaded.is_null() {
            None
        } else {
            Some(loaded)
        }
    }

    /// Safe atomic initialization
    pub fn init_once<T, F>(ptr: &AtomicPtr<T>, init: F) -> Result<(), ()>
    where
        F: FnOnce() -> T,
    {
        if ptr.load(Ordering::Acquire).is_null() {
            let new_value = Box::into_raw(Box::new(init()));
            match ptr.compare_exchange(
                ptr::null_mut(),
                new_value,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => Ok(()),
                Err(_) => {
                    // Another thread initialized it, clean up our allocation
                    unsafe { let _ = Box::from_raw(new_value); }
                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }
}

/// Memory ordering examples and patterns
pub mod memory_ordering {
    use super::*;

    /// Example of release-acquire synchronization
    pub fn release_acquire_example() {
        let data = Arc::new(AtomicUsize::new(0));
        let ready = Arc::new(AtomicBool::new(false));

        let data_clone = data.clone();
        let ready_clone = ready.clone();

        std::thread::spawn(move || {
            // Write data first
            data_clone.store(42, Ordering::Relaxed);

            // Then set ready flag with release ordering
            ready_clone.store(true, Ordering::Release);
        });

        // Wait for ready flag with acquire ordering
        while !ready.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }

        // Now we can safely read data
        let value = data.load(Ordering::Relaxed);
        assert_eq!(value, 42);
    }

    /// Sequential consistency example
    pub fn sequential_consistency_example() {
        let x = Arc::new(AtomicUsize::new(0));
        let y = Arc::new(AtomicUsize::new(0));

        let x1 = x.clone();
        let y1 = y.clone();

        let handle1 = std::thread::spawn(move || {
            x1.store(1, Ordering::SeqCst);
            y1.store(1, Ordering::SeqCst);
        });

        let x2 = x.clone();
        let y2 = y.clone();

        let handle2 = std::thread::spawn(move || {
            let y_val = y2.load(Ordering::SeqCst);
            let x_val = x2.load(Ordering::SeqCst);
            (x_val, y_val)
        });

        handle1.join().unwrap();
        let (x_val, y_val) = handle2.join().unwrap();

        // With SeqCst, we should never see (1, 0) - if y is 1, x must also be 1
        assert!(!(x_val == 1 && y_val == 0));
    }

    /// Fence example for synchronization
    pub fn fence_example() {
        let mut data = 0;
        let ready = Arc::new(AtomicBool::new(false));

        let ready_clone = ready.clone();

        std::thread::spawn(move || {
            data = 42;
            // Memory fence ensures data write is visible
            std::sync::atomic::fence(Ordering::Release);
            ready_clone.store(true, Ordering::Relaxed);
        });

        while !ready.load(Ordering::Relaxed) {
            std::hint::spin_loop();
        }

        // Acquire fence ensures we see the data write
        std::sync::atomic::fence(Ordering::Acquire);
        assert_eq!(data, 42);
    }
}

/// Hazard pointer for safe memory reclamation in lock-free structures
#[derive(Debug)]
pub struct HazardPointer {
    pointer: AtomicPtr<()>,
}

impl HazardPointer {
    pub fn new() -> Self {
        Self {
            pointer: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Protect a pointer from being freed
    pub fn protect<T>(&self, ptr: *mut T) {
        self.pointer.store(ptr as *mut (), Ordering::Release);
    }

    /// Unprotect the current pointer
    pub fn unprotect(&self) {
        self.pointer.store(ptr::null_mut(), Ordering::Release);
    }

    /// Check if a pointer is currently protected
    pub fn is_protected(&self, ptr: *mut ()) -> bool {
        self.pointer.load(Ordering::Acquire) == ptr
    }
}

impl Default for HazardPointer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_counter() {
        let counter = AtomicCounter::new(10);

        assert_eq!(counter.increment(), 11);
        assert_eq!(counter.get(), 11);

        assert_eq!(counter.add(5), 11);
        assert_eq!(counter.get(), 16);

        assert_eq!(counter.decrement(), 15);
        assert_eq!(counter.get(), 15);
    }

    #[test]
    fn test_atomic_flag() {
        let flag = AtomicFlag::new();

        assert!(!flag.is_set());

        assert!(!flag.set()); // Was not set, now is set
        assert!(flag.is_set());

        assert!(flag.clear()); // Was set, now is clear
        assert!(!flag.is_set());

        assert!(flag.test_and_set()); // Successfully set
        assert!(!flag.test_and_set()); // Already set

        assert!(flag.test_and_clear()); // Successfully cleared
        assert!(!flag.test_and_clear()); // Already cleared
    }

    #[test]
    fn test_spin_lock() {
        let lock = SpinLock::new(42);

        // Acquire lock
        {
            let guard = lock.lock();
            assert_eq!(*guard, 42);
            *guard = 100;
        }

        // Lock should be released
        let guard = lock.try_lock().unwrap();
        assert_eq!(*guard, 100);
    }

    #[test]
    fn test_seq_lock() {
        let lock = SeqLock::new(42);

        // Read initial value
        assert_eq!(lock.read(), 42);

        // Write new value
        lock.write(100);

        // Read new value
        assert_eq!(lock.read(), 100);
    }

    #[test]
    fn test_hazard_pointer() {
        let hp = HazardPointer::new();

        let ptr = Box::into_raw(Box::new(42));

        // Protect the pointer
        hp.protect(ptr);
        assert!(hp.is_protected(ptr as *mut ()));

        // Unprotect
        hp.unprotect();
        assert!(!hp.is_protected(ptr as *mut ()));

        // Cleanup
        unsafe { let _ = Box::from_raw(ptr); }
    }

    #[test]
    fn test_memory_ordering_examples() {
        memory_ordering::release_acquire_example();
        memory_ordering::sequential_consistency_example();
        memory_ordering::fence_example();
    }

    #[tokio::test]
    async fn test_concurrent_counter() {
        let counter = Arc::new(AtomicCounter::new(0));

        let mut handles = Vec::new();

        // Spawn 10 threads, each incrementing 100 times
        for _ in 0..10 {
            let counter_clone = counter.clone();
            let handle = tokio::task::spawn(async move {
                for _ in 0..100 {
                    counter_clone.increment();
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(counter.get(), 1000);
    }
}
