//! Atomic Operations Performance Benchmarks
//!
//! Benchmarks comparing atomic operations vs traditional locking mechanisms
//! to demonstrate performance characteristics and trade-offs.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::{Arc, Mutex};
use std::thread;

/// Benchmark atomic counter vs mutex counter
fn bench_counters(c: &mut Criterion) {
    c.bench_function("atomic_counter_single_threaded", |b| {
        let counter = crate::atomic_patterns::AtomicCounter::new(0);

        b.iter(|| {
            for _ in 0..1000 {
                black_box(counter.increment());
            }
        });
    });

    c.bench_function("mutex_counter_single_threaded", |b| {
        let counter = Arc::new(Mutex::new(0));

        b.iter(|| {
            for _ in 0..1000 {
                let mut guard = counter.lock().unwrap();
                *guard += 1;
                black_box(*guard);
            }
        });
    });

    c.bench_function("atomic_counter_multi_threaded", |b| {
        let counter = Arc::new(crate::atomic_patterns::AtomicCounter::new(0));

        b.iter(|| {
            let mut handles = Vec::new();

            for _ in 0..4 {
                let counter = counter.clone();
                let handle = thread::spawn(move || {
                    for _ in 0..250 {
                        black_box(counter.increment());
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    c.bench_function("mutex_counter_multi_threaded", |b| {
        let counter = Arc::new(Mutex::new(0));

        b.iter(|| {
            let mut handles = Vec::new();

            for _ in 0..4 {
                let counter = counter.clone();
                let handle = thread::spawn(move || {
                    for _ in 0..250 {
                        let mut guard = counter.lock().unwrap();
                        *guard += 1;
                        black_box(*guard);
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });
}

/// Benchmark atomic flag operations
fn bench_flags(c: &mut Criterion) {
    c.bench_function("atomic_flag_operations", |b| {
        let flag = crate::atomic_patterns::AtomicFlag::new();

        b.iter(|| {
            for _ in 0..500 {
                black_box(flag.set());
                black_box(flag.is_set());
                black_box(flag.clear());
                black_box(flag.test_and_set());
                black_box(flag.test_and_clear());
            }
        });
    });

    c.bench_function("mutex_flag_operations", |b| {
        let flag = Arc::new(Mutex::new(false));

        b.iter(|| {
            for _ in 0..500 {
                {
                    let mut guard = flag.lock().unwrap();
                    *guard = true;
                    black_box(*guard);
                }
                {
                    let guard = flag.lock().unwrap();
                    black_box(*guard);
                }
                {
                    let mut guard = flag.lock().unwrap();
                    *guard = false;
                    black_box(*guard);
                }
            }
        });
    });
}

/// Benchmark spin lock vs regular mutex
fn bench_spin_lock(c: &mut Criterion) {
    c.bench_function("spin_lock_operations", |b| {
        let lock = crate::atomic_patterns::SpinLock::new(0);

        b.iter(|| {
            for _ in 0..1000 {
                let mut guard = lock.lock();
                *guard += 1;
                black_box(*guard);
            }
        });
    });

    c.bench_function("mutex_operations", |b| {
        let lock = Arc::new(Mutex::new(0));

        b.iter(|| {
            for _ in 0..1000 {
                let mut guard = lock.lock().unwrap();
                *guard += 1;
                black_box(*guard);
            }
        });
    });

    c.bench_function("spin_lock_contended", |b| {
        let lock = Arc::new(crate::atomic_patterns::SpinLock::new(0));

        b.iter(|| {
            let mut handles = Vec::new();

            for _ in 0..4 {
                let lock = lock.clone();
                let handle = thread::spawn(move || {
                    for _ in 0..250 {
                        let mut guard = lock.lock();
                        *guard += 1;
                        black_box(*guard);
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    c.bench_function("mutex_contended", |b| {
        let lock = Arc::new(Mutex::new(0));

        b.iter(|| {
            let mut handles = Vec::new();

            for _ in 0..4 {
                let lock = lock.clone();
                let handle = thread::spawn(move || {
                    for _ in 0..250 {
                        let mut guard = lock.lock().unwrap();
                        *guard += 1;
                        black_box(*guard);
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });
}

/// Benchmark sequence lock performance
fn bench_seq_lock(c: &mut Criterion) {
    c.bench_function("seq_lock_read_heavy", |b| {
        let lock = Arc::new(crate::atomic_patterns::SeqLock::new(0));

        b.iter(|| {
            let mut sum = 0;
            for _ in 0..1000 {
                sum += black_box(lock.read());
            }
            black_box(sum);
        });
    });

    c.bench_function("seq_lock_write_heavy", |b| {
        let lock = Arc::new(crate::atomic_patterns::SeqLock::new(0));

        b.iter(|| {
            for i in 0..1000 {
                lock.write(black_box(i));
            }
        });
    });

    c.bench_function("seq_lock_mixed_workload", |b| {
        let lock = Arc::new(crate::atomic_patterns::SeqLock::new(0));

        b.iter(|| {
            let mut handles = Vec::new();

            // Reader threads
            for _ in 0..3 {
                let lock = lock.clone();
                let handle = thread::spawn(move || {
                    for _ in 0..333 {
                        black_box(lock.read());
                    }
                });
                handles.push(handle);
            }

            // Writer thread
            let lock = lock.clone();
            let writer = thread::spawn(move || {
                for i in 0..1000 {
                    lock.write(black_box(i));
                }
            });
            handles.push(writer);

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });
}

/// Benchmark atomic float operations
fn bench_atomic_float(c: &mut Criterion) {
    c.bench_function("atomic_float_operations", |b| {
        let atomic = crate::atomic_patterns::AtomicFloat64::new(1.0);

        b.iter(|| {
            for _ in 0..1000 {
                black_box(atomic.add(1.0));
                black_box(atomic.multiply(1.01));
                black_box(atomic.load(std::sync::atomic::Ordering::Relaxed));
            }
        });
    });

    c.bench_function("mutex_float_operations", |b| {
        let atomic = Arc::new(Mutex::new(1.0));

        b.iter(|| {
            for _ in 0..1000 {
                {
                    let mut guard = atomic.lock().unwrap();
                    *guard += 1.0;
                    black_box(*guard);
                }
                {
                    let mut guard = atomic.lock().unwrap();
                    *guard *= 1.01;
                    black_box(*guard);
                }
                {
                    let guard = atomic.lock().unwrap();
                    black_box(*guard);
                }
            }
        });
    });
}

/// Benchmark memory ordering effects
fn bench_memory_ordering(c: &mut Criterion) {
    c.bench_function("relaxed_ordering", |b| {
        let x = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let y = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        b.iter(|| {
            let x1 = x.clone();
            let y1 = y.clone();

            let handle1 = thread::spawn(move || {
                x1.store(1, std::sync::atomic::Ordering::Relaxed);
                y1.store(1, std::sync::atomic::Ordering::Relaxed);
            });

            let x2 = x.clone();
            let y2 = y.clone();

            let handle2 = thread::spawn(move || {
                let y_val = y2.load(std::sync::atomic::Ordering::Relaxed);
                let x_val = x2.load(std::sync::atomic::Ordering::Relaxed);
                black_box((x_val, y_val));
            });

            handle1.join().unwrap();
            handle2.join().unwrap();

            // Reset
            x.store(0, std::sync::atomic::Ordering::Relaxed);
            y.store(0, std::sync::atomic::Ordering::Relaxed);
        });
    });

    c.bench_function("seq_cst_ordering", |b| {
        let x = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let y = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        b.iter(|| {
            let x1 = x.clone();
            let y1 = y.clone();

            let handle1 = thread::spawn(move || {
                x1.store(1, std::sync::atomic::Ordering::SeqCst);
                y1.store(1, std::sync::atomic::Ordering::SeqCst);
            });

            let x2 = x.clone();
            let y2 = y.clone();

            let handle2 = thread::spawn(move || {
                let y_val = y2.load(std::sync::atomic::Ordering::SeqCst);
                let x_val = x2.load(std::sync::atomic::Ordering::SeqCst);
                black_box((x_val, y_val));
            });

            handle1.join().unwrap();
            handle2.join().unwrap();

            // Reset
            x.store(0, std::sync::atomic::Ordering::SeqCst);
            y.store(0, std::sync::atomic::Ordering::SeqCst);
        });
    });
}

/// Benchmark hazard pointer operations
fn bench_hazard_pointers(c: &mut Criterion) {
    c.bench_function("hazard_pointer_operations", |b| {
        let hp = crate::atomic_patterns::HazardPointer::new();

        b.iter(|| {
            for i in 0..1000 {
                let ptr = black_box(i as *mut u32);
                hp.protect(ptr);
                black_box(hp.is_protected(ptr));
                hp.unprotect();
            }
        });
    });
}

/// Benchmark double-checked locking
fn bench_double_checked_lock(c: &mut Criterion) {
    c.bench_function("double_checked_lock", |b| {
        let init_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let lock = crate::atomic_patterns::DoubleCheckedLock::new(|| {
            init_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            "initialized".to_string()
        });

        b.iter(|| {
            for _ in 0..1000 {
                black_box(lock.get());
            }
        });
    });

    c.bench_function("regular_mutex_initialization", |b| {
        let init_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let lock = Arc::new(Mutex::new(None));

        b.iter(|| {
            for _ in 0..1000 {
                let mut guard = lock.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(init_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
                }
                black_box(guard.as_ref());
            }
        });
    });
}

criterion_group!(
    atomic_benches,
    bench_counters,
    bench_flags,
    bench_spin_lock,
    bench_seq_lock,
    bench_atomic_float,
    bench_memory_ordering,
    bench_hazard_pointers,
    bench_double_checked_lock,
);
criterion_main!(atomic_benches);
