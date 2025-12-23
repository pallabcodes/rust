//! Concurrency Pattern Benchmarks
//!
//! Performance benchmarks for various concurrency patterns to measure
//! throughput, latency, and resource usage under different workloads.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;
use std::sync::Arc;
use std::time::Duration;

/// Benchmark actor pattern performance
fn bench_actor_pattern(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("actor_pattern_sequential", |b| {
        b.iter(|| {
            rt.block_on(async {
                let actor = crate::actor::SpawnExt::spawn(crate::actor::TestActor { value: 0 });

                for i in 0..100 {
                    let _ = actor.send(crate::actor::TestMsg::Increment(1)).await;
                }
            });
        });
    });

    c.bench_function("actor_pattern_parallel", |b| {
        b.iter(|| {
            rt.block_on(async {
                let actor = crate::actor::SpawnExt::spawn(crate::actor::TestActor { value: 0 });

                let mut handles = Vec::new();
                for _ in 0..10 {
                    let actor_clone = actor.clone();
                    let handle = tokio::spawn(async move {
                        for _ in 0..10 {
                            let _ = actor_clone.send(crate::actor::TestMsg::Increment(1)).await;
                        }
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    handle.await.unwrap();
                }
            });
        });
    });
}

/// Benchmark worker pool performance
fn bench_worker_pool(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("worker_pool_static", |b| {
        b.iter(|| {
            rt.block_on(async {
                let pool = crate::worker_pool::WorkerPool::new(
                    crate::worker_pool::WorkerPoolConfig {
                        num_workers: 4,
                        queue_capacity: 1000,
                    }
                );

                let mut handles = Vec::new();
                for i in 0..100 {
                    let pool = &pool;
                    let handle = tokio::spawn(async move {
                        pool.submit(move || {
                            black_box(fibonacci(black_box(20)));
                        }).await.unwrap();
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    handle.await.unwrap();
                }

                pool.shutdown().await;
            });
        });
    });

    c.bench_function("worker_pool_semaphore", |b| {
        b.iter(|| {
            rt.block_on(async {
                let pool = crate::worker_pool::SemaphoreWorkerPool::new(4);

                let mut handles = Vec::new();
                for _ in 0..100 {
                    let pool = &pool;
                    let handle = tokio::spawn(async move {
                        pool.execute(|| async {
                            black_box(fibonacci(black_box(20)));
                        }).await.unwrap();
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    handle.await.unwrap();
                }

                pool.shutdown().await;
            });
        });
    });
}

/// Benchmark lock-free data structures
fn bench_lock_free(c: &mut Criterion) {
    c.bench_function("lock_free_ring_buffer", |b| {
        b.iter(|| {
            let buffer = crate::lock_free::LockFreeRingBuffer::<u64>::new(1024);

            for i in 0..1000 {
                while buffer.push(black_box(i)).is_err() {
                    // Buffer full, try to consume
                    let _ = buffer.pop();
                }
            }

            // Consume remaining
            while buffer.pop().is_ok() {}
        });
    });

    c.bench_function("lock_free_queue", |b| {
        b.iter(|| {
            let queue = crate::lock_free::LockFreeQueue::<u64>::new();

            // Producer
            for i in 0..1000 {
                while queue.enqueue(black_box(i)).is_err() {
                    std::thread::yield_now();
                }
            }

            // Consumer
            for _ in 0..1000 {
                while queue.dequeue().is_err() {
                    std::thread::yield_now();
                }
            }
        });
    });

    c.bench_function("lock_free_stack", |b| {
        b.iter(|| {
            let stack = crate::lock_free::LockFreeStack::<u64>::new();

            // Push
            for i in 0..1000 {
                while stack.push(black_box(i)).is_err() {
                    std::thread::yield_now();
                }
            }

            // Pop
            for _ in 0..1000 {
                while stack.pop().is_err() {
                    std::thread::yield_now();
                }
            }
        });
    });
}

/// Benchmark atomic operations
fn bench_atomic_operations(c: &mut Criterion) {
    c.bench_function("atomic_counter_increment", |b| {
        let counter = crate::atomic_patterns::AtomicCounter::new(0);

        b.iter(|| {
            for _ in 0..1000 {
                black_box(counter.increment());
            }
        });
    });

    c.bench_function("atomic_counter_cas_loop", |b| {
        let counter = crate::atomic_patterns::AtomicCounter::new(0);

        b.iter(|| {
            for _ in 0..1000 {
                let mut current = counter.get();
                loop {
                    let new_value = current + 1;
                    match counter.compare_exchange(current, new_value) {
                        Ok(_) => break,
                        Err(actual) => current = actual,
                    }
                }
            }
        });
    });

    c.bench_function("atomic_flag_operations", |b| {
        let flag = crate::atomic_patterns::AtomicFlag::new();

        b.iter(|| {
            for _ in 0..500 {
                black_box(flag.set());
                black_box(flag.clear());
            }
        });
    });
}

/// Benchmark channel performance
fn bench_channels(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("mpsc_channel_throughput", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1000);

                let producer = tokio::spawn(async move {
                    for i in 0..1000 {
                        tx.send(black_box(i)).await.unwrap();
                    }
                });

                let consumer = tokio::spawn(async move {
                    for _ in 0..1000 {
                        black_box(rx.recv().await);
                    }
                });

                producer.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });

    c.bench_function("broadcast_channel_throughput", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, _) = tokio::sync::broadcast::channel(1000);
                let mut rx1 = tx.subscribe();
                let mut rx2 = tx.subscribe();

                let producer = tokio::spawn(async move {
                    for i in 0..1000 {
                        black_box(tx.send(black_box(i)).unwrap());
                    }
                });

                let consumer1 = tokio::spawn(async move {
                    for _ in 0..1000 {
                        black_box(rx1.recv().await);
                    }
                });

                let consumer2 = tokio::spawn(async move {
                    for _ in 0..1000 {
                        black_box(rx2.recv().await);
                    }
                });

                producer.await.unwrap();
                consumer1.await.unwrap();
                consumer2.await.unwrap();
            });
        });
    });

    c.bench_function("watch_channel_throughput", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, _) = tokio::sync::watch::channel(0);
                let mut rx1 = tx.subscribe();
                let mut rx2 = tx.subscribe();

                let producer = tokio::spawn(async move {
                    for i in 0..1000 {
                        black_box(tx.send(black_box(i)).unwrap());
                    }
                });

                let consumer1 = tokio::spawn(async move {
                    for _ in 0..1000 {
                        black_box(rx1.changed().await);
                    }
                });

                let consumer2 = tokio::spawn(async move {
                    for _ in 0..1000 {
                        black_box(rx2.changed().await);
                    }
                });

                producer.await.unwrap();
                consumer1.await.unwrap();
                consumer2.await.unwrap();
            });
        });
    });
}

/// Benchmark pipeline patterns
fn bench_pipelines(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("processing_pipeline_sequential", |b| {
        b.iter(|| {
            rt.block_on(async {
                let pipeline = crate::pipeline::PipelineBuilder::new()
                    .add_stage("stage1", |x: i32| Box::pin(async move { Ok(black_box(x * 2)) }))
                    .add_stage("stage2", |x: i32| Box::pin(async move { Ok(black_box(x + 1)) }))
                    .add_stage("stage3", |x: i32| Box::pin(async move { Ok(black_box(x / 2)) }))
                    .build();

                for i in 0..100 {
                    black_box(pipeline.process_item(black_box(i)).await.unwrap());
                }
            });
        });
    });

    c.bench_function("fan_out_fan_in_pipeline", |b| {
        b.iter(|| {
            rt.block_on(async {
                let pipeline = crate::fan_out_fan_in::FanOutFanIn::new(
                    4,
                    1000,
                    |x: i32| Box::pin(async move { black_box(x * 2) }),
                );

                for i in 0..100 {
                    pipeline.submit(black_box(i)).await.unwrap();
                }

                let _results = pipeline.complete_and_collect().await;
            });
        });
    });

    c.bench_function("batch_pipeline", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut pipeline = crate::batching::BatchPipeline::new(
                    10,
                    Duration::from_millis(100),
                    |batch: Vec<i32>| Box::pin(async move {
                        black_box(batch.into_iter().map(|x| x * 2).collect())
                    }),
                );

                for i in 0..100 {
                    pipeline.submit(black_box(i)).await.unwrap();
                }

                let _results = pipeline.collect_results().await;
            });
        });
    });
}

/// Benchmark circuit breaker and retry patterns
fn bench_resilience_patterns(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("circuit_breaker_fast_path", |b| {
        b.iter(|| {
            rt.block_on(async {
                let breaker = crate::circuit_breaker::CircuitBreaker::new(
                    crate::circuit_breaker::CircuitBreakerConfig {
                        failure_threshold: 5,
                        recovery_timeout: Duration::from_secs(60),
                        success_threshold: 3,
                        timeout: Duration::from_millis(100),
                        name: Some("bench-breaker".to_string()),
                    }
                );

                for _ in 0..100 {
                    let _ = breaker.call(|| async { Ok(black_box(42)) }).await;
                }
            });
        });
    });

    c.bench_function("retry_policy_success", |b| {
        b.iter(|| {
            rt.block_on(async {
                let policy = crate::retry::RetryPolicy::new(crate::retry::RetryConfig {
                    max_attempts: 3,
                    initial_delay: Duration::from_micros(100),
                    max_delay: Duration::from_millis(10),
                    backoff_multiplier: 2.0,
                    jitter: false,
                    timeout: Some(Duration::from_millis(50)),
                });

                for _ in 0..50 {
                    let _ = policy.retry(|| async { Ok(black_box(42)) }).await;
                }
            });
        });
    });
}

/// Benchmark distributed locks
fn bench_distributed_locks(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("distributed_lock_single_threaded", |b| {
        b.iter(|| {
            rt.block_on(async {
                let lock = crate::distributed_lock::DistributedLock::new(
                    crate::distributed_lock::LockConfig {
                        name: "bench-lock".to_string(),
                        ttl: Duration::from_secs(30),
                        retry_attempts: 1,
                        retry_delay: Duration::from_millis(1),
                        owner_id: Some("bench-owner".to_string()),
                    }
                );

                for _ in 0..100 {
                    let token = lock.try_acquire().await.unwrap();
                    black_box(token);
                    lock.release(token).await.unwrap();
                }
            });
        });
    });
}

/// Benchmark rate limiters
fn bench_rate_limiters(c: &mut Criterion) {
    c.bench_function("token_bucket_limiter", |b| {
        b.iter(|| {
            let limiter = crate::rate_limiter::TokenBucket::new(1000, 1000);

            for _ in 0..1000 {
                black_box(limiter.try_acquire());
            }
        });
    });

    c.bench_function("leaky_bucket_limiter", |b| {
        b.iter(|| {
            let limiter = crate::rate_limiter::LeakyBucket::new(1000, 1000);

            for _ in 0..1000 {
                black_box(limiter.try_acquire());
            }
        });
    });
}

/// Fibonacci function for CPU-bound work simulation
fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

criterion_group!(
    benches,
    bench_actor_pattern,
    bench_worker_pool,
    bench_lock_free,
    bench_atomic_operations,
    bench_channels,
    bench_pipelines,
    bench_resilience_patterns,
    bench_distributed_locks,
    bench_rate_limiters,
);
criterion_main!(benches);
