//! Memory Model Deep Dive
//!
//! Understanding memory ordering and the happens-before relationship is
//! crucial for writing correct concurrent code. This module provides examples
//! and explanations of memory ordering semantics and their implications.
//!
//! ## Key Concepts
//!
//! - **Happens-Before**: Partial order of operations across threads
//! - **Sequential Consistency**: Total order of all operations
//! - **Acquire-Release**: Synchronization without total ordering
//! - **Relaxed Ordering**: No synchronization guarantees
//! - **Memory Fences**: Compiler and hardware barriers
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::memory_model::*;
//!
//! // Example of happens-before relationship
//! demonstrate_happens_before();
//! ```

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Demonstrate happens-before relationship
pub fn demonstrate_happens_before() {
    let x = Arc::new(AtomicUsize::new(0));
    let y = Arc::new(AtomicUsize::new(0));

    let x1 = x.clone();
    let y1 = y.clone();

    let handle1 = thread::spawn(move || {
        x1.store(1, Ordering::Relaxed);
        y1.store(1, Ordering::Release);
    });

    let x2 = x.clone();
    let y2 = y.clone();

    let handle2 = thread::spawn(move || {
        let y_val = y2.load(Ordering::Acquire);
        let x_val = x2.load(Ordering::Relaxed);

        if y_val == 1 {
            // With Acquire/Release, we know x was stored before y
            assert_eq!(x_val, 1);
        }

        (x_val, y_val)
    });

    handle1.join().unwrap();
    let (x_val, y_val) = handle2.join().unwrap();

    println!("Thread 2 observed: x={}, y={}", x_val, y_val);
}

/// Demonstrate sequential consistency
pub fn demonstrate_sequential_consistency() {
    let x = Arc::new(AtomicUsize::new(0));
    let y = Arc::new(AtomicUsize::new(0));

    let mut results = Vec::new();

    for _ in 0..1000 {
        let x1 = x.clone();
        let y1 = y.clone();

        let handle1 = thread::spawn(move || {
            x1.store(1, Ordering::SeqCst);
            y1.store(1, Ordering::SeqCst);
        });

        let x2 = x.clone();
        let y2 = y.clone();

        let handle2 = thread::spawn(move || {
            let y_val = y2.load(Ordering::SeqCst);
            let x_val = x2.load(Ordering::SeqCst);
            (x_val, y_val)
        });

        handle1.join().unwrap();
        let result = handle2.join().unwrap();
        results.push(result);

        // Reset for next iteration
        x.store(0, Ordering::SeqCst);
        y.store(0, Ordering::SeqCst);
    }

    // Count how many times we saw the "impossible" outcome
    let impossible_count = results.iter().filter(|(x_val, y_val)| *x_val == 1 && *y_val == 0).count();

    println!("Sequential consistency test:");
    println!("Total iterations: {}", results.len());
    println!("Impossible outcomes (x=1,y=0): {}", impossible_count);

    // With SeqCst, we should never see (1, 0)
    assert_eq!(impossible_count, 0);
}

/// Demonstrate acquire-release semantics
pub fn demonstrate_acquire_release() {
    let data = Arc::new(AtomicUsize::new(0));
    let ready = Arc::new(AtomicBool::new(false));

    let data_clone = data.clone();
    let ready_clone = ready.clone();

    let producer = thread::spawn(move || {
        // Write data
        data_clone.store(42, Ordering::Relaxed);

        // Release fence ensures data write is visible before ready
        ready_clone.store(true, Ordering::Release);
    });

    let data_clone = data.clone();
    let ready_clone = ready.clone();

    let consumer = thread::spawn(move || {
        // Acquire fence ensures we see all writes before ready
        while !ready_clone.load(Ordering::Acquire) {
            thread::yield_now();
        }

        // Now we can safely read data
        let value = data_clone.load(Ordering::Relaxed);
        value
    });

    producer.join().unwrap();
    let received = consumer.join().unwrap();

    println!("Acquire-release result: {}", received);
    assert_eq!(received, 42);
}

/// Demonstrate relaxed ordering implications
pub fn demonstrate_relaxed_ordering() {
    let x = Arc::new(AtomicUsize::new(0));
    let y = Arc::new(AtomicUsize::new(0));

    let x1 = x.clone();
    let y1 = y.clone();

    let handle1 = thread::spawn(move || {
        x1.store(1, Ordering::Relaxed);
        y1.store(1, Ordering::Relaxed);
    });

    let x2 = x.clone();
    let y2 = y.clone();

    let handle2 = thread::spawn(move || {
        let x_val = x2.load(Ordering::Relaxed);
        let y_val = y2.load(Ordering::Relaxed);
        (x_val, y_val)
    });

    handle1.join().unwrap();
    let (x_val, y_val) = handle2.join().unwrap();

    println!("Relaxed ordering observed: x={}, y={}", x_val, y_val);

    // With relaxed ordering, we might see various outcomes
    // This is non-deterministic and depends on hardware and timing
}

/// Message passing pattern with memory ordering
pub mod message_passing {
    use super::*;

    #[derive(Debug)]
    pub struct Channel<T> {
        data: Arc<AtomicUsize>, // Simplified: using usize to represent data
        ready: Arc<AtomicBool>,
    }

    impl<T> Channel<T> {
        pub fn new() -> Self {
            Self {
                data: Arc::new(AtomicUsize::new(0)),
                ready: Arc::new(AtomicBool::new(false)),
            }
        }

        pub fn send(&self, value: usize) {
            // Store data first
            self.data.store(value, Ordering::Relaxed);

            // Then signal ready with release semantics
            self.ready.store(true, Ordering::Release);
        }

        pub fn receive(&self) -> usize {
            // Wait for ready with acquire semantics
            while !self.ready.load(Ordering::Acquire) {
                thread::yield_now();
            }

            // Now safely load data
            self.data.load(Ordering::Relaxed)
        }
    }

    pub fn demonstrate_channel() {
        let channel = Arc::new(Channel::new());

        let channel_clone = channel.clone();
        let sender = thread::spawn(move || {
            channel_clone.send(42);
        });

        let channel_clone = channel.clone();
        let receiver = thread::spawn(move || {
            channel_clone.receive()
        });

        sender.join().unwrap();
        let received = receiver.join().unwrap();

        println!("Channel received: {}", received);
        assert_eq!(received, 42);
    }
}

/// Memory fence examples
pub mod fences {
    use super::*;

    /// Compiler fence prevents reordering by compiler
    pub fn compiler_fence_example() {
        let mut data = 0;
        let ready = Arc::new(AtomicBool::new(false));

        let ready_clone = ready.clone();

        let producer = thread::spawn(move || {
            data = 42;

            // Compiler fence prevents reordering of data write
            std::sync::atomic::compiler_fence(Ordering::Release);

            ready_clone.store(true, Ordering::Relaxed);
        });

        let ready_clone = ready.clone();

        let consumer = thread::spawn(move || {
            while !ready_clone.load(Ordering::Relaxed) {
                thread::yield_now();
            }

            // Compiler fence ensures we read data after ready check
            std::sync::atomic::compiler_fence(Ordering::Acquire);

            data
        });

        producer.join().unwrap();
        let received = consumer.join().unwrap();

        println!("Compiler fence result: {}", received);
        // Note: This may not work on all architectures without hardware fences
    }

    /// Hardware fence (memory barrier)
    pub fn memory_fence_example() {
        let x = Arc::new(AtomicUsize::new(0));
        let y = Arc::new(AtomicUsize::new(0));

        let x1 = x.clone();
        let y1 = y.clone();

        let handle1 = thread::spawn(move || {
            x1.store(1, Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Release);
            y1.store(1, Ordering::Relaxed);
        });

        let x2 = x.clone();
        let y2 = y.clone();

        let handle2 = thread::spawn(move || {
            let y_val = y2.load(Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Acquire);
            let x_val = x2.load(Ordering::Relaxed);
            (x_val, y_val)
        });

        handle1.join().unwrap();
        let (x_val, y_val) = handle2.join().unwrap();

        println!("Memory fence observed: x={}, y={}", x_val, y_val);

        // With fences, if y=1 then x=1 should be visible
        if y_val == 1 {
            assert_eq!(x_val, 1);
        }
    }
}

/// Cache coherence and visibility examples
pub mod cache_coherence {
    use super::*;

    /// Demonstrate false sharing
    pub fn demonstrate_false_sharing() {
        const NUM_THREADS: usize = 4;
        const ITERATIONS: usize = 100_000;

        // Two counters in the same cache line (potential false sharing)
        #[repr(align(64))] // Cache line alignment
        struct PaddedCounter {
            value: AtomicUsize,
            _padding: [u8; 64 - std::mem::size_of::<AtomicUsize>()],
        }

        let counter1 = Arc::new(PaddedCounter {
            value: AtomicUsize::new(0),
            _padding: [0; 64 - std::mem::size_of::<AtomicUsize>()],
        });

        let counter2 = Arc::new(PaddedCounter {
            value: AtomicUsize::new(0),
            _padding: [0; 64 - std::mem::size_of::<AtomicUsize>()],
        });

        let start = std::time::Instant::now();

        let mut handles = Vec::new();

        // Threads updating counter1
        for _ in 0..NUM_THREADS {
            let counter1 = counter1.clone();
            let handle = thread::spawn(move || {
                for _ in 0..ITERATIONS {
                    counter1.value.fetch_add(1, Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }

        // Threads updating counter2
        for _ in 0..NUM_THREADS {
            let counter2 = counter2.clone();
            let handle = thread::spawn(move || {
                for _ in 0..ITERATIONS {
                    counter2.value.fetch_add(1, Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let duration = start.elapsed();

        println!("False sharing test completed in {:?}", duration);
        println!("Counter1: {}", counter1.value.load(Ordering::Relaxed));
        println!("Counter2: {}", counter2.value.load(Ordering::Relaxed));
    }
}

/// Common memory ordering pitfalls and solutions
pub mod pitfalls {
    use super::*;

    /// Demonstrate the ABA problem
    pub fn demonstrate_aba_problem() {
        let value = Arc::new(AtomicUsize::new(0));

        let value_clone = value.clone();
        let handle1 = thread::spawn(move || {
            // Thread 1: A -> B -> A (ABA)
            value_clone.store(1, Ordering::Relaxed); // A = 0, set to 1
            thread::yield_now();
            value_clone.store(0, Ordering::Relaxed); // Back to 0 (A)
        });

        let value_clone = value.clone();
        let handle2 = thread::spawn(move || {
            thread::yield_now();
            // Thread 2: Compare and exchange
            let result = value_clone.compare_exchange(
                0, // Expected A
                2, // New value
                Ordering::AcqRel,
                Ordering::Acquire,
            );

            match result {
                Ok(_) => println!("CAS succeeded: 0 -> 2"),
                Err(actual) => println!("CAS failed: expected 0, got {}", actual),
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        println!("ABA demonstration completed");
    }

    /// Demonstrate race condition without proper ordering
    pub fn demonstrate_race_condition() {
        let flag = Arc::new(AtomicBool::new(false));
        let data = Arc::new(AtomicUsize::new(0));

        let flag_clone = flag.clone();
        let data_clone = data.clone();

        let writer = thread::spawn(move || {
            data_clone.store(42, Ordering::Relaxed);
            // Missing fence here!
            flag_clone.store(true, Ordering::Relaxed);
        });

        let flag_clone = flag.clone();
        let data_clone = data.clone();

        let reader = thread::spawn(move || {
            while !flag_clone.load(Ordering::Relaxed) {
                thread::yield_now();
            }
            // Missing fence here!
            let value = data_clone.load(Ordering::Relaxed);
            value
        });

        writer.join().unwrap();
        let read_value = reader.join().unwrap();

        println!("Race condition result: {}", read_value);
        // Without proper ordering, this might print 0 instead of 42!
        // (Though on x86 it might work due to stronger memory model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_happens_before() {
        demonstrate_happens_before();
    }

    #[test]
    fn test_sequential_consistency() {
        demonstrate_sequential_consistency();
    }

    #[test]
    fn test_acquire_release() {
        demonstrate_acquire_release();
    }

    #[test]
    fn test_message_passing() {
        message_passing::demonstrate_channel();
    }

    #[test]
    fn test_fences() {
        fences::compiler_fence_example();
        fences::memory_fence_example();
    }

    #[test]
    fn test_cache_coherence() {
        cache_coherence::demonstrate_false_sharing();
    }

    #[test]
    fn test_pitfalls() {
        pitfalls::demonstrate_aba_problem();
        pitfalls::demonstrate_race_condition();
    }

    #[test]
    fn test_channel_operations() {
        let channel = message_passing::Channel::new();

        channel.send(123);
        let received = channel.receive();

        assert_eq!(received, 123);
    }
}
