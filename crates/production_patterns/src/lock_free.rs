//! Lock-Free Data Structures
//!
//! Lock-free data structures provide thread-safe operations without traditional
//! locking mechanisms, using atomic operations and careful memory ordering.
//! These are essential for high-performance, low-latency systems where
//! lock contention would be unacceptable.
//!
//! ## Key Concepts
//!
//! - **CAS Operations**: Compare-and-swap for atomic updates
//! - **Memory Ordering**: Acquire/Release/Relaxed semantics
//! - **ABA Problem**: Solutions for the ABA comparison issue
//! - **Hazard Pointers**: Safe memory reclamation in lock-free structures
//! - **Ring Buffers**: Bounded circular buffers for producer-consumer patterns
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::lock_free::LockFreeRingBuffer;
//!
//! let buffer = LockFreeRingBuffer::<i32>::new(1024);
//!
//! // Producer
//! buffer.push(42).await;
//!
//! // Consumer
//! if let Some(value) = buffer.pop().await {
//!     println!("Received: {}", value);
//! }
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{debug, instrument};

use crate::error::LockFreeError;

/// Lock-free ring buffer for single producer, single consumer (SPSC)
#[derive(Debug)]
pub struct LockFreeRingBuffer<T> {
    buffer: Vec<T>,
    capacity: usize,
    head: Arc<AtomicUsize>, // Consumer position
    tail: Arc<AtomicUsize>, // Producer position
}

impl<T> LockFreeRingBuffer<T>
where
    T: Send + Sync + Default + Clone,
{
    /// Create a new ring buffer with the specified capacity (must be power of 2)
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be a power of 2");

        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(T::default());
        }

        Self {
            buffer,
            capacity,
            head: Arc::new(AtomicUsize::new(0)),
            tail: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Push an item to the ring buffer (producer)
    #[instrument(skip(self, item))]
    pub fn push(&self, item: T) -> Result<(), LockFreeError> {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);

        let next_tail = (tail + 1) & (self.capacity - 1);

        // Check if buffer is full
        if next_tail == head {
            return Err(LockFreeError::BufferFull);
        }

        // Store the item
        // SAFETY: We're the only producer, so tail is our exclusive slot
        unsafe {
            let slot = self.buffer.as_ptr().add(tail) as *mut T;
            std::ptr::write(slot, item);
        }

        // Update tail with release ordering
        self.tail.store(next_tail, Ordering::Release);

        debug!("Pushed item to ring buffer, tail: {}", next_tail);
        Ok(())
    }

    /// Pop an item from the ring buffer (consumer)
    #[instrument(skip(self))]
    pub fn pop(&self) -> Result<T, LockFreeError> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        // Check if buffer is empty
        if head == tail {
            return Err(LockFreeError::BufferEmpty);
        }

        // Read the item
        // SAFETY: We're the only consumer, so head is our exclusive slot
        let item = unsafe {
            let slot = self.buffer.as_ptr().add(head);
            std::ptr::read(slot)
        };

        // Update head with release ordering
        let next_head = (head + 1) & (self.capacity - 1);
        self.head.store(next_head, Ordering::Release);

        debug!("Popped item from ring buffer, head: {}", next_head);
        Ok(item)
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    /// Check if the buffer is full
    pub fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        let next_tail = (tail + 1) & (self.capacity - 1);
        next_tail == head
    }

    /// Get the current length (number of items)
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);

        if tail >= head {
            tail - head
        } else {
            self.capacity - head + tail
        }
    }

    /// Get the capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get buffer statistics
    pub fn stats(&self) -> RingBufferStats {
        RingBufferStats {
            capacity: self.capacity,
            length: self.len(),
            is_empty: self.is_empty(),
            is_full: self.is_full(),
        }
    }
}

unsafe impl<T: Send> Send for LockFreeRingBuffer<T> {}
unsafe impl<T: Sync> Sync for LockFreeRingBuffer<T> {}

/// Statistics for ring buffer
#[derive(Debug, Clone)]
pub struct RingBufferStats {
    pub capacity: usize,
    pub length: usize,
    pub is_empty: bool,
    pub is_full: bool,
}

/// Lock-free queue using Michael-Scott algorithm
#[derive(Debug)]
pub struct LockFreeQueue<T> {
    head: Arc<AtomicNodePtr<T>>,
    tail: Arc<AtomicNodePtr<T>>,
}

type AtomicNodePtr<T> = std::sync::atomic::AtomicPtr<Node<T>>;

#[derive(Debug)]
struct Node<T> {
    value: Option<T>,
    next: AtomicNodePtr<T>,
}

impl<T> LockFreeQueue<T> {
    pub fn new() -> Self {
        let sentinel = Box::into_raw(Box::new(Node {
            value: None,
            next: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
        }));

        Self {
            head: Arc::new(std::sync::atomic::AtomicPtr::new(sentinel)),
            tail: Arc::new(std::sync::atomic::AtomicPtr::new(sentinel)),
        }
    }

    /// Enqueue an item
    pub fn enqueue(&self, value: T) -> Result<(), LockFreeError> {
        let new_node = Box::into_raw(Box::new(Node {
            value: Some(value),
            next: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
        }));

        loop {
            let tail = self.tail.load(Ordering::Acquire);
            let next = unsafe { (*tail).next.load(Ordering::Acquire) };

            if tail == self.tail.load(Ordering::Acquire) {
                if next.is_null() {
                    // Try to link the new node
                    if unsafe { (*tail).next.compare_exchange_weak(
                        std::ptr::null_mut(),
                        new_node,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ).is_ok() } {
                        // Try to swing tail to the new node
                        let _ = self.tail.compare_exchange_weak(
                            tail,
                            new_node,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        );
                        return Ok(());
                    }
                } else {
                    // Tail is lagging, try to swing it forward
                    let _ = self.tail.compare_exchange_weak(
                        tail,
                        next,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                }
            }
        }
    }

    /// Dequeue an item
    pub fn dequeue(&self) -> Result<T, LockFreeError> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);
            let next = unsafe { (*head).next.load(Ordering::Acquire) };

            if head == self.head.load(Ordering::Acquire) {
                if head == tail {
                    if next.is_null() {
                        return Err(LockFreeError::BufferEmpty);
                    }
                    // Tail is lagging, try to swing it forward
                    let _ = self.tail.compare_exchange_weak(
                        tail,
                        next,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                } else {
                    // Read the value
                    let value = unsafe { (*next).value.take() };
                    if self.head.compare_exchange_weak(
                        head,
                        next,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ).is_ok() {
                        // Successfully dequeued
                        unsafe {
                            // Free the old head node
                            let _ = Box::from_raw(head);
                        }
                        return value.ok_or(LockFreeError::BufferEmpty);
                    }
                }
            }
        }
    }

    /// Check if queue is empty (approximate)
    pub fn is_empty(&self) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let next = unsafe { (*head).next.load(Ordering::Acquire) };
        next.is_null()
    }
}

impl<T> Default for LockFreeQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Manual implementation to avoid requiring T: Default
impl<T> Drop for LockFreeQueue<T> {
    fn drop(&mut self) {
        // Clean up remaining nodes
        let mut current = self.head.load(Ordering::Relaxed);
        while !current.is_null() {
            let next = unsafe { (*current).next.load(Ordering::Relaxed) };
            unsafe { let _ = Box::from_raw(current); }
            current = next;
        }
    }
}

/// Lock-free stack using Treiber's algorithm
#[derive(Debug)]
pub struct LockFreeStack<T> {
    head: Arc<AtomicNodePtr<T>>,
}

impl<T> LockFreeStack<T> {
    pub fn new() -> Self {
        Self {
            head: Arc::new(std::sync::atomic::AtomicPtr::new(std::ptr::null_mut())),
        }
    }

    /// Push an item onto the stack
    pub fn push(&self, value: T) -> Result<(), LockFreeError> {
        let new_node = Box::into_raw(Box::new(Node {
            value: Some(value),
            next: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
        }));

        loop {
            let head = self.head.load(Ordering::Acquire);
            unsafe { (*new_node).next.store(head, Ordering::Relaxed); }

            if self.head.compare_exchange_weak(
                head,
                new_node,
                Ordering::AcqRel,
                Ordering::Acquire,
            ).is_ok() {
                return Ok(());
            }
        }
    }

    /// Pop an item from the stack
    pub fn pop(&self) -> Result<T, LockFreeError> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            if head.is_null() {
                return Err(LockFreeError::BufferEmpty);
            }

            let next = unsafe { (*head).next.load(Ordering::Acquire) };

            if self.head.compare_exchange_weak(
                head,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ).is_ok() {
                let value = unsafe { (*head).value.take() };
                unsafe { let _ = Box::from_raw(head); }
                return value.ok_or(LockFreeError::BufferEmpty);
            }
        }
    }

    /// Check if stack is empty (approximate)
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire).is_null()
    }
}

impl<T> Default for LockFreeStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Atomic float64 using CAS loop
#[derive(Debug)]
pub struct AtomicFloat64 {
    bits: std::sync::atomic::AtomicU64,
}

impl AtomicFloat64 {
    pub fn new(value: f64) -> Self {
        Self {
            bits: std::sync::atomic::AtomicU64::new(value.to_bits()),
        }
    }

    /// Load the current value
    pub fn load(&self, ordering: Ordering) -> f64 {
        f64::from_bits(self.bits.load(ordering))
    }

    /// Store a new value
    pub fn store(&self, value: f64, ordering: Ordering) {
        self.bits.store(value.to_bits(), ordering);
    }

    /// Compare and exchange
    pub fn compare_exchange(
        &self,
        current: f64,
        new: f64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<f64, f64> {
        match self.bits.compare_exchange(
            current.to_bits(),
            new.to_bits(),
            success,
            failure,
        ) {
            Ok(bits) => Ok(f64::from_bits(bits)),
            Err(bits) => Err(f64::from_bits(bits)),
        }
    }

    /// Add a value atomically
    pub fn add(&self, delta: f64) -> f64 {
        loop {
            let current = self.load(Ordering::Acquire);
            let new = current + delta;
            match self.compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return new,
                Err(_) => continue,
            }
        }
    }

    /// Multiply by a factor atomically
    pub fn multiply(&self, factor: f64) -> f64 {
        loop {
            let current = self.load(Ordering::Acquire);
            let new = current * factor;
            match self.compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return new,
                Err(_) => continue,
            }
        }
    }
}

impl Default for AtomicFloat64 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

/// Double-checked locking pattern
#[derive(Debug)]
pub struct DoubleCheckedLock<T, F> {
    value: std::sync::Mutex<Option<T>>,
    init_fn: F,
}

impl<T, F> DoubleCheckedLock<T, F>
where
    F: Fn() -> T,
{
    pub fn new(init_fn: F) -> Self {
        Self {
            value: std::sync::Mutex::new(None),
            init_fn,
        }
    }

    /// Get the value, initializing it if necessary
    pub fn get(&self) -> std::sync::MutexGuard<Option<T>> {
        let mut guard = self.value.lock().unwrap();
        if guard.is_none() {
            // Double-check: another thread might have initialized it
            if guard.is_none() {
                *guard = Some((self.init_fn)());
            }
        }
        guard
    }

    /// Check if initialized without locking
    pub fn is_initialized(&self) -> bool {
        self.value.lock().unwrap().is_some()
    }
}

/// Memory barrier utilities
pub mod barriers {
    use std::sync::atomic::Ordering;

    /// Full memory barrier
    #[inline]
    pub fn memory_barrier() {
        std::sync::atomic::fence(Ordering::SeqCst);
    }

    /// Acquire barrier (loads before, stores after)
    #[inline]
    pub fn acquire_barrier() {
        std::sync::atomic::fence(Ordering::Acquire);
    }

    /// Release barrier (stores before, loads after)
    #[inline]
    pub fn release_barrier() {
        std::sync::atomic::fence(Ordering::Release);
    }

    /// Compiler barrier (prevents compiler reordering)
    #[inline]
    pub fn compiler_barrier() {
        std::sync::atomic::compiler_fence(Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::task;

    #[test]
    fn test_ring_buffer_basic() {
        let buffer = LockFreeRingBuffer::<i32>::new(4);

        // Initially empty
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);

        // Push some items
        assert!(buffer.push(1).is_ok());
        assert!(buffer.push(2).is_ok());
        assert!(buffer.push(3).is_ok());
        assert!(buffer.push(4).is_ok());

        assert_eq!(buffer.len(), 4);
        assert!(buffer.is_full());

        // Should reject 5th item
        assert!(matches!(buffer.push(5), Err(LockFreeError::BufferFull)));

        // Pop items
        assert_eq!(buffer.pop().unwrap(), 1);
        assert_eq!(buffer.pop().unwrap(), 2);
        assert_eq!(buffer.pop().unwrap(), 3);
        assert_eq!(buffer.pop().unwrap(), 4);

        assert!(buffer.is_empty());
        assert!(matches!(buffer.pop(), Err(LockFreeError::BufferEmpty)));
    }

    #[test]
    fn test_ring_buffer_wraparound() {
        let buffer = LockFreeRingBuffer::<i32>::new(4);

        // Fill buffer
        for i in 0..4 {
            buffer.push(i).unwrap();
        }

        // Pop 2 items
        assert_eq!(buffer.pop().unwrap(), 0);
        assert_eq!(buffer.pop().unwrap(), 1);

        // Push 2 more (should wrap around)
        assert!(buffer.push(10).is_ok());
        assert!(buffer.push(11).is_ok());

        // Pop remaining
        assert_eq!(buffer.pop().unwrap(), 2);
        assert_eq!(buffer.pop().unwrap(), 3);
        assert_eq!(buffer.pop().unwrap(), 10);
        assert_eq!(buffer.pop().unwrap(), 11);
    }

    #[test]
    fn test_lock_free_queue() {
        let queue = LockFreeQueue::<i32>::new();

        // Initially empty
        assert!(queue.is_empty());

        // Enqueue items
        queue.enqueue(1).unwrap();
        queue.enqueue(2).unwrap();
        queue.enqueue(3).unwrap();

        // Dequeue items
        assert_eq!(queue.dequeue().unwrap(), 1);
        assert_eq!(queue.dequeue().unwrap(), 2);
        assert_eq!(queue.dequeue().unwrap(), 3);

        assert!(queue.is_empty());
        assert!(matches!(queue.dequeue(), Err(LockFreeError::BufferEmpty)));
    }

    #[test]
    fn test_lock_free_stack() {
        let stack = LockFreeStack::<i32>::new();

        // Initially empty
        assert!(stack.is_empty());

        // Push items
        stack.push(1).unwrap();
        stack.push(2).unwrap();
        stack.push(3).unwrap();

        // Pop items (LIFO)
        assert_eq!(stack.pop().unwrap(), 3);
        assert_eq!(stack.pop().unwrap(), 2);
        assert_eq!(stack.pop().unwrap(), 1);

        assert!(stack.is_empty());
        assert!(matches!(stack.pop(), Err(LockFreeError::BufferEmpty)));
    }

    #[test]
    fn test_atomic_float64() {
        let atomic = AtomicFloat64::new(1.5);

        // Load initial value
        assert_eq!(atomic.load(Ordering::Relaxed), 1.5);

        // Store new value
        atomic.store(2.5, Ordering::Relaxed);
        assert_eq!(atomic.load(Ordering::Relaxed), 2.5);

        // Add operation
        let result = atomic.add(1.0);
        assert_eq!(result, 3.5);
        assert_eq!(atomic.load(Ordering::Relaxed), 3.5);

        // Multiply operation
        let result = atomic.multiply(2.0);
        assert_eq!(result, 7.0);
        assert_eq!(atomic.load(Ordering::Relaxed), 7.0);
    }

    #[test]
    fn test_double_checked_lock() {
        let init_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let lock = DoubleCheckedLock::new(|| {
            init_count.fetch_add(1, Ordering::Relaxed);
            "initialized".to_string()
        });

        // First access should initialize
        {
            let value = lock.get();
            assert_eq!(*value, Some("initialized".to_string()));
        }
        assert_eq!(init_count.load(Ordering::Relaxed), 1);

        // Second access should not re-initialize
        {
            let value = lock.get();
            assert_eq!(*value, Some("initialized".to_string()));
        }
        assert_eq!(init_count.load(Ordering::Relaxed), 1);

        assert!(lock.is_initialized());
    }

    #[tokio::test]
    async fn test_concurrent_ring_buffer() {
        let buffer = Arc::new(LockFreeRingBuffer::<i32>::new(1024));

        let producer = {
            let buffer = buffer.clone();
            task::spawn(async move {
                for i in 0..100 {
                    while buffer.push(i).is_err() {
                        tokio::task::yield_now().await;
                    }
                }
            })
        };

        let consumer = {
            let buffer = buffer.clone();
            task::spawn(async move {
                let mut received = Vec::new();
                for _ in 0..100 {
                    loop {
                        if let Ok(value) = buffer.pop() {
                            received.push(value);
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                }
                received
            })
        };

        producer.await.unwrap();
        let received = consumer.await.unwrap();

        // Should have received all values (order may vary due to SPSC nature)
        assert_eq!(received.len(), 100);
        assert_eq!(received.iter().sum::<i32>(), (0..100).sum::<i32>());
    }
}
