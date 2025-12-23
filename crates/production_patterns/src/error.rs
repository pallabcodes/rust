//! Error types for production patterns

use thiserror::Error;
use std::fmt;

#[derive(Error, Debug)]
pub enum ActorError {
    #[error("actor mailbox is full")]
    MailboxFull,

    #[error("actor is shutting down")]
    ShuttingDown,

    #[error("message handling failed: {0}")]
    HandlerError(String),

    #[error("actor not found: {0}")]
    ActorNotFound(String),

    #[error("supervision error: {0}")]
    SupervisionError(String),
}

#[derive(Error, Debug)]
pub enum WorkerPoolError {
    #[error("worker pool is shutting down")]
    ShuttingDown,

    #[error("task submission failed")]
    TaskSubmissionFailed,

    #[error("worker panicked: {0}")]
    WorkerPanic(String),

    #[error("pool scaling failed: {0}")]
    ScalingError(String),
}

#[derive(Error, Debug)]
pub enum CircuitBreakerError {
    #[error("circuit breaker is open")]
    CircuitOpen,

    #[error("circuit breaker timeout")]
    Timeout,

    #[error("execution failed: {0}")]
    ExecutionError(String),
}

#[derive(Error, Debug)]
pub enum DistributedLockError {
    #[error("lock acquisition failed")]
    AcquisitionFailed,

    #[error("lock expired")]
    LockExpired,

    #[error("lock already held")]
    AlreadyHeld,

    #[error("distributed operation failed: {0}")]
    OperationError(String),
}

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("pipeline stage failed: {0}")]
    StageError(String),

    #[error("pipeline cancelled")]
    Cancelled,

    #[error("backpressure exceeded")]
    BackpressureExceeded,

    #[error("ordering violation")]
    OrderingViolation,
}

#[derive(Error, Debug)]
pub enum LockFreeError {
    #[error("buffer full")]
    BufferFull,

    #[error("buffer empty")]
    BufferEmpty,

    #[error("CAS operation failed")]
    CasFailed,

    #[error("memory ordering violation")]
    MemoryOrderingViolation,
}

#[derive(Error, Debug)]
pub enum AsyncError {
    #[error("async operation timed out")]
    Timeout,

    #[error("backpressure limit exceeded")]
    BackpressureLimit,

    #[error("stream closed")]
    StreamClosed,

    #[error("channel closed")]
    ChannelClosed,
}

#[derive(Error, Debug)]
pub enum ProductionError {
    #[error("resource leak detected: {0}")]
    ResourceLeak(String),

    #[error("checkpoint recovery failed: {0}")]
    CheckpointError(String),

    #[error("exactly-once violation: {0}")]
    ExactlyOnceError(String),

    #[error("health check failed: {0}")]
    HealthCheckError(String),
}
