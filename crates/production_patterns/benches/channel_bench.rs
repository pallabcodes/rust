//! Channel Performance Benchmarks
//!
//! Detailed benchmarks for different channel implementations and usage patterns
//! to understand performance characteristics under various loads.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

/// Benchmark basic channel operations
fn bench_basic_channels(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("tokio_mpsc_unbounded", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

                let producer = tokio::spawn(async move {
                    for i in 0..10000 {
                        black_box(tx.send(black_box(i)).unwrap());
                    }
                });

                let consumer = tokio::spawn(async move {
                    for _ in 0..10000 {
                        black_box(rx.recv().await);
                    }
                });

                producer.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });

    c.bench_function("tokio_mpsc_bounded_1", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1);

                let producer = tokio::spawn(async move {
                    for i in 0..10000 {
                        black_box(tx.send(black_box(i)).await.unwrap());
                    }
                });

                let consumer = tokio::spawn(async move {
                    for _ in 0..10000 {
                        black_box(rx.recv().await);
                    }
                });

                producer.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });

    c.bench_function("tokio_mpsc_bounded_1000", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1000);

                let producer = tokio::spawn(async move {
                    for i in 0..10000 {
                        black_box(tx.send(black_box(i)).await.unwrap());
                    }
                });

                let consumer = tokio::spawn(async move {
                    for _ in 0..10000 {
                        black_box(rx.recv().await);
                    }
                });

                producer.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });
}

/// Benchmark broadcast channel performance
fn bench_broadcast_channels(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("broadcast_single_consumer", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = tokio::sync::broadcast::channel(1000);

                let producer = tokio::spawn(async move {
                    for i in 0..10000 {
                        black_box(tx.send(black_box(i)).unwrap());
                    }
                });

                let consumer = tokio::spawn(async move {
                    for _ in 0..10000 {
                        black_box(rx.recv().await.unwrap());
                    }
                });

                producer.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });

    c.bench_function("broadcast_multi_consumer_3", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, _) = tokio::sync::broadcast::channel(1000);
                let mut rx1 = tx.subscribe();
                let mut rx2 = tx.subscribe();
                let mut rx3 = tx.subscribe();

                let producer = tokio::spawn(async move {
                    for i in 0..10000 {
                        black_box(tx.send(black_box(i)).unwrap());
                    }
                });

                let consumer1 = tokio::spawn(async move {
                    for _ in 0..10000 {
                        black_box(rx1.recv().await.unwrap());
                    }
                });

                let consumer2 = tokio::spawn(async move {
                    for _ in 0..10000 {
                        black_box(rx2.recv().await.unwrap());
                    }
                });

                let consumer3 = tokio::spawn(async move {
                    for _ in 0..10000 {
                        black_box(rx3.recv().await.unwrap());
                    }
                });

                producer.await.unwrap();
                consumer1.await.unwrap();
                consumer2.await.unwrap();
                consumer3.await.unwrap();
            });
        });
    });
}

/// Benchmark watch channel performance
fn bench_watch_channels(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("watch_single_consumer", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = tokio::sync::watch::channel(0);

                let producer = tokio::spawn(async move {
                    for i in 1..10000 {
                        black_box(tx.send(black_box(i)).unwrap());
                    }
                });

                let consumer = tokio::spawn(async move {
                    for _ in 1..10000 {
                        black_box(rx.changed().await);
                    }
                });

                producer.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });

    c.bench_function("watch_multi_consumer_3", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, _) = tokio::sync::watch::channel(0);
                let mut rx1 = tx.subscribe();
                let mut rx2 = tx.subscribe();
                let mut rx3 = tx.subscribe();

                let producer = tokio::spawn(async move {
                    for i in 1..10000 {
                        black_box(tx.send(black_box(i)).unwrap());
                    }
                });

                let consumer1 = tokio::spawn(async move {
                    for _ in 1..10000 {
                        black_box(rx1.changed().await);
                    }
                });

                let consumer2 = tokio::spawn(async move {
                    for _ in 1..10000 {
                        black_box(rx2.changed().await);
                    }
                });

                let consumer3 = tokio::spawn(async move {
                    for _ in 1..10000 {
                        black_box(rx3.changed().await);
                    }
                });

                producer.await.unwrap();
                consumer1.await.unwrap();
                consumer2.await.unwrap();
                consumer3.await.unwrap();
            });
        });
    });
}

/// Benchmark oneshot channels
fn bench_oneshot_channels(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("oneshot_channels", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();

                for i in 0..1000 {
                    let (tx, rx) = tokio::sync::oneshot::channel();

                    let producer = tokio::spawn(async move {
                        black_box(tx.send(black_box(i)).unwrap());
                    });

                    let consumer = tokio::spawn(async move {
                        black_box(rx.await.unwrap());
                    });

                    handles.push((producer, consumer));
                }

                for (producer, consumer) in handles {
                    producer.await.unwrap();
                    consumer.await.unwrap();
                }
            });
        });
    });
}

/// Benchmark rendezvous channels
fn bench_rendezvous_channels(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("rendezvous_channels", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut channel = crate::async_channels::RendezvousChannel::new();

                let producer = tokio::spawn(async move {
                    for i in 0..1000 {
                        black_box(channel.send(black_box(i)).await.unwrap());
                    }
                });

                let consumer = tokio::spawn(async move {
                    for _ in 0..1000 {
                        black_box(channel.recv().await.unwrap());
                    }
                });

                producer.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });
}

/// Benchmark channel select performance
fn bench_channel_select(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("select_2_channels", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx1, mut rx1) = tokio::sync::mpsc::channel(100);
                let (tx2, mut rx2) = tokio::sync::mpsc::channel(100);

                let producer1 = tokio::spawn(async move {
                    for i in 0..5000 {
                        black_box(tx1.send(black_box(i)).await.unwrap());
                    }
                });

                let producer2 = tokio::spawn(async move {
                    for i in 0..5000 {
                        black_box(tx2.send(black_box(i)).await.unwrap());
                    }
                });

                let consumer = tokio::spawn(async move {
                    let mut count = 0;
                    while count < 10000 {
                        tokio::select! {
                            Some(_) = rx1.recv() => count += 1,
                            Some(_) = rx2.recv() => count += 1,
                            else => break,
                        }
                    }
                });

                producer1.await.unwrap();
                producer2.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });

    c.bench_function("select_4_channels", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx1, mut rx1) = tokio::sync::mpsc::channel(100);
                let (tx2, mut rx2) = tokio::sync::mpsc::channel(100);
                let (tx3, mut rx3) = tokio::sync::mpsc::channel(100);
                let (tx4, mut rx4) = tokio::sync::mpsc::channel(100);

                let producers = vec![
                    tokio::spawn(async move {
                        for i in 0..2500 {
                            black_box(tx1.send(black_box(i)).await.unwrap());
                        }
                    }),
                    tokio::spawn(async move {
                        for i in 0..2500 {
                            black_box(tx2.send(black_box(i)).await.unwrap());
                        }
                    }),
                    tokio::spawn(async move {
                        for i in 0..2500 {
                            black_box(tx3.send(black_box(i)).await.unwrap());
                        }
                    }),
                    tokio::spawn(async move {
                        for i in 0..2500 {
                            black_box(tx4.send(black_box(i)).await.unwrap());
                        }
                    }),
                ];

                let consumer = tokio::spawn(async move {
                    let mut count = 0;
                    while count < 10000 {
                        tokio::select! {
                            Some(_) = rx1.recv() => count += 1,
                            Some(_) = rx2.recv() => count += 1,
                            Some(_) = rx3.recv() => count += 1,
                            Some(_) = rx4.recv() => count += 1,
                            else => break,
                        }
                    }
                });

                for producer in producers {
                    producer.await.unwrap();
                }
                consumer.await.unwrap();
            });
        });
    });
}

/// Benchmark backpressure scenarios
fn bench_backpressure(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("bounded_channel_backpressure", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = tokio::sync::mpsc::channel(10); // Small buffer

                let producer = tokio::spawn(async move {
                    for i in 0..1000 {
                        // This will cause backpressure when buffer fills
                        black_box(tx.send(black_box(i)).await.unwrap());
                    }
                });

                let consumer = tokio::spawn(async move {
                    for _ in 0..1000 {
                        black_box(rx.recv().await);
                        // Slow consumer - causes backpressure
                        tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;
                    }
                });

                producer.await.unwrap();
                consumer.await.unwrap();
            });
        });
    });

    c.bench_function("semaphore_backpressure", |b| {
        b.iter(|| {
            rt.block_on(async {
                let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(10));

                let mut handles = Vec::new();

                for _ in 0..100 {
                    let sem = semaphore.clone();
                    let handle = tokio::spawn(async move {
                        let permit = sem.acquire().await.unwrap();
                        // Hold permit for a while
                        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
                        drop(permit);
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

/// Benchmark channel throughput under contention
fn bench_channel_contention(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("high_contention_mpsc", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1000);

                let mut producers = Vec::new();
                for _ in 0..10 {
                    let tx = tx.clone();
                    let producer = tokio::spawn(async move {
                        for i in 0..1000 {
                            black_box(tx.send(black_box(i)).await.unwrap());
                        }
                    });
                    producers.push(producer);
                }

                let consumer = tokio::spawn(async move {
                    for _ in 0..10000 {
                        black_box(rx.recv().await);
                    }
                });

                for producer in producers {
                    producer.await.unwrap();
                }
                consumer.await.unwrap();
            });
        });
    });
}

criterion_group!(
    channel_benches,
    bench_basic_channels,
    bench_broadcast_channels,
    bench_watch_channels,
    bench_oneshot_channels,
    bench_rendezvous_channels,
    bench_channel_select,
    bench_backpressure,
    bench_channel_contention,
);
criterion_main!(channel_benches);
