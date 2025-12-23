//! Production-grade concurrency patterns for Rust.
//!
//! This crate implements advanced concurrency patterns required for SDE-3 level
//! backend engineering, including distributed systems, actor patterns, and
//! production observability.
//!
//! ## Patterns Covered
//!
//! - **Actor Pattern**: State ownership and message passing
//! - **Worker Pools**: Static and adaptive scaling pools
//! - **Production Async**: Streams, backpressure, advanced channels
//! - **Distributed Systems**: Circuit breakers, retries, coordination
//! - **Advanced Sync**: Lock-free data structures and atomics
//! - **Production Systems**: Observability, crash recovery, exactly-once processing
//! - **Pipeline Patterns**: Fan-out/in, batching, ordered processing

pub mod actor;
pub mod sharded_actor;
pub mod supervision;
pub mod worker_pool;
pub mod adaptive_pool;
pub mod task_scheduler;
pub mod async_streams;
pub mod backpressure;
pub mod async_channels;
pub mod graceful_shutdown;
pub mod circuit_breaker;
pub mod retry;
pub mod distributed_lock;
pub mod rate_limiter;
pub mod bulkhead;
pub mod lock_free;
pub mod atomic_patterns;
pub mod rcu;
pub mod memory_model;
pub mod leak_detector;
pub mod checkpoint;
pub mod exactly_once;
pub mod metrics;
pub mod health;
pub mod pipeline;
pub mod fan_out_fan_in;
pub mod batching;

pub mod error;
pub mod common;
