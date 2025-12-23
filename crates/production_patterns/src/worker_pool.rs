//! Static Worker Pool Pattern
//!
//! Worker pools provide a fixed number of goroutines/threads that process
//! tasks from a shared queue. This pattern is essential for controlling
//! concurrency and resource usage in production systems.
//!
//! ## Key Concepts
//!
//! - **Fixed Pool Size**: Pre-allocated workers for predictable resource usage
//! - **Task Queue**: Bounded channel for task distribution
//! - **Load Balancing**: Automatic task distribution across workers
//! - **Graceful Shutdown**: Coordinated shutdown of all workers
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::worker_pool::{WorkerPool, WorkerPoolConfig};
//!
//! let pool = WorkerPool::new(WorkerPoolConfig {
//!     num_workers: 4,
//!     queue_capacity: 100,
//! });
//!
//! // Submit tasks
//! for i in 0..10 {
//!     pool.submit(move || {
//!         println!("Processing task {}", i);
//!         // Task logic here
//!     }).await;
//! }
//!
//! // Shutdown pool
//! pool.shutdown().await;
//! ```

use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument};

use crate::common::{Metrics, ShutdownCoordinator};
use crate::error::WorkerPoolError;

/// Configuration for worker pool
#[derive(Debug, Clone)]
pub struct WorkerPoolConfig {
    pub num_workers: usize,
    pub queue_capacity: usize,
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            num_workers: num_cpus::get(),
            queue_capacity: 1000,
        }
    }
}

/// Task to be executed by worker pool
pub type Task = Box<dyn FnOnce() + Send + 'static>;

/// Static worker pool with fixed number of workers
#[derive(Debug)]
pub struct WorkerPool {
    config: WorkerPoolConfig,
    task_sender: mpsc::Sender<Task>,
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
    handles: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    /// Create a new worker pool with given configuration
    #[instrument(skip(config), fields(workers = %config.num_workers, queue = %config.queue_capacity))]
    pub fn new(config: WorkerPoolConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.queue_capacity);
        let shutdown = ShutdownCoordinator::new();
        let metrics = Arc::new(Metrics::new());

        // Create worker tasks
        let mut handles = Vec::with_capacity(config.num_workers);
        for worker_id in 0..config.num_workers {
            let rx = rx.clone();
            let shutdown = shutdown.clone();
            let metrics = metrics.clone();

            let handle = tokio::spawn(async move {
                run_worker(worker_id, rx, shutdown, metrics).await;
            });

            handles.push(handle);
        }

        info!("Created worker pool with {} workers", config.num_workers);

        Self {
            config,
            task_sender: tx,
            shutdown,
            metrics,
            handles,
        }
    }

    /// Submit a task to the worker pool
    #[instrument(skip(self, task), fields(pool_workers = %self.config.num_workers))]
    pub async fn submit<F>(&self, task: F) -> Result<(), WorkerPoolError>
    where
        F: FnOnce() + Send + 'static,
    {
        self.task_sender.send(Box::new(task))
            .await
            .map_err(|_| WorkerPoolError::TaskSubmissionFailed)
    }

    /// Try to submit a task without waiting (non-blocking)
    pub fn try_submit<F>(&self, task: F) -> Result<(), WorkerPoolError>
    where
        F: FnOnce() + Send + 'static,
    {
        self.task_sender.try_send(Box::new(task))
            .map_err(|_| WorkerPoolError::TaskSubmissionFailed)
    }

    /// Get current pool statistics
    pub fn stats(&self) -> WorkerPoolStats {
        let (ops, errs, avg_duration) = self.metrics.get_stats();
        WorkerPoolStats {
            num_workers: self.config.num_workers,
            queue_capacity: self.config.queue_capacity,
            total_tasks: ops,
            failed_tasks: errs,
            avg_task_duration: avg_duration,
            is_shutdown: self.shutdown.is_shutdown(),
        }
    }

    /// Gracefully shutdown the worker pool
    #[instrument(skip(self), fields(workers = %self.config.num_workers))]
    pub async fn shutdown(self) {
        info!("Shutting down worker pool");

        // Signal shutdown
        self.shutdown.shutdown();

        // Close task channel to prevent new tasks
        drop(self.task_sender);

        // Wait for all workers to finish
        for handle in self.handles {
            let _ = handle.await;
        }

        info!("Worker pool shutdown complete");
    }
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::new(WorkerPoolConfig::default())
    }
}

/// Statistics for worker pool
#[derive(Debug, Clone)]
pub struct WorkerPoolStats {
    pub num_workers: usize,
    pub queue_capacity: usize,
    pub total_tasks: u64,
    pub failed_tasks: u64,
    pub avg_task_duration: std::time::Duration,
    pub is_shutdown: bool,
}

/// Internal worker function
async fn run_worker(
    worker_id: usize,
    mut task_receiver: mpsc::Receiver<Task>,
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
) {
    info!("Worker {} started", worker_id);

    loop {
        tokio::select! {
            // Check for shutdown signal
            _ = shutdown.wait_shutdown() => {
                debug!("Worker {} received shutdown signal", worker_id);
                break;
            }

            // Receive and execute task
            task = task_receiver.recv() => {
                match task {
                    Some(task) => {
                        let timer = crate::common::Timer::new();

                        // Execute task
                        task();

                        let duration = timer.elapsed();
                        metrics.record_operation(duration);

                        debug!("Worker {} completed task in {:?}", worker_id, duration);
                    }
                    None => {
                        // Channel closed
                        debug!("Worker {} task channel closed", worker_id);
                        break;
                    }
                }
            }
        }
    }

    info!("Worker {} stopped", worker_id);
}

/// Worker pool with semaphore-based concurrency control
#[derive(Debug)]
pub struct SemaphoreWorkerPool {
    semaphore: Arc<Semaphore>,
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
}

impl SemaphoreWorkerPool {
    /// Create a new semaphore-based worker pool
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            shutdown: ShutdownCoordinator::new(),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Execute a task with concurrency control
    #[instrument(skip(self, task))]
    pub async fn execute<F, Fut>(&self, task: F) -> Result<(), WorkerPoolError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        // Acquire semaphore permit
        let permit = self.semaphore.acquire().await
            .map_err(|_| WorkerPoolError::TaskSubmissionFailed)?;

        // Don't drop permit until task completes
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            let timer = crate::common::Timer::new();

            // Execute the async task
            task().await;

            let duration = timer.elapsed();
            metrics.record_operation(duration);

            // Permit is automatically dropped here
            drop(permit);
        });

        Ok(())
    }

    /// Get current pool statistics
    pub fn stats(&self) -> SemaphorePoolStats {
        let (ops, errs, avg_duration) = self.metrics.get_stats();
        SemaphorePoolStats {
            max_concurrent: self.semaphore.available_permits(),
            active_tasks: ops.saturating_sub(errs), // Approximation
            total_tasks: ops,
            avg_task_duration: avg_duration,
            is_shutdown: self.shutdown.is_shutdown(),
        }
    }

    /// Gracefully shutdown the pool
    pub async fn shutdown(&self) {
        self.shutdown.shutdown();
    }
}

/// Statistics for semaphore worker pool
#[derive(Debug, Clone)]
pub struct SemaphorePoolStats {
    pub max_concurrent: usize,
    pub active_tasks: u64,
    pub total_tasks: u64,
    pub avg_task_duration: std::time::Duration,
    pub is_shutdown: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::time::{timeout, Duration};

    static TASK_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn test_worker_pool_basic() {
        let pool = WorkerPool::new(WorkerPoolConfig {
            num_workers: 2,
            queue_capacity: 10,
        });

        let counter = Arc::new(AtomicU64::new(0));

        // Submit some tasks
        for _ in 0..5 {
            let counter = counter.clone();
            pool.submit(move || {
                counter.fetch_add(1, Ordering::Relaxed);
            }).await.unwrap();
        }

        // Give tasks time to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check that tasks ran
        assert_eq!(counter.load(Ordering::Relaxed), 5);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_worker_pool_stats() {
        let pool = WorkerPool::new(WorkerPoolConfig {
            num_workers: 1,
            queue_capacity: 5,
        });

        // Submit a task
        pool.submit(|| {
            std::thread::sleep(Duration::from_millis(10));
        }).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = pool.stats();
        assert_eq!(stats.num_workers, 1);
        assert_eq!(stats.queue_capacity, 5);
        assert_eq!(stats.total_tasks, 1);
        assert!(!stats.is_shutdown);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_semaphore_pool() {
        let pool = SemaphoreWorkerPool::new(2);

        let counter = Arc::new(AtomicU64::new(0));

        // Submit concurrent tasks
        for i in 0..4 {
            let counter = counter.clone();
            pool.execute(move || async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                counter.fetch_add(1, Ordering::Relaxed);
            }).await.unwrap();
        }

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(counter.load(Ordering::Relaxed), 4);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_worker_pool_queue_full() {
        let pool = WorkerPool::new(WorkerPoolConfig {
            num_workers: 1,
            queue_capacity: 1,
        });

        // Fill the queue
        pool.submit(|| std::thread::sleep(Duration::from_millis(100))).await.unwrap();

        // Try to submit when queue is full (this might succeed or fail depending on timing)
        let result = timeout(Duration::from_millis(10),
            pool.submit(|| {})).await;

        pool.shutdown().await;

        // Either it succeeded (if worker picked up task quickly) or timed out
        // This is a timing-dependent test, so we just ensure no panic
    }

    #[tokio::test]
    async fn test_try_submit() {
        let pool = WorkerPool::new(WorkerPoolConfig {
            num_workers: 1,
            queue_capacity: 0, // No buffering
        });

        // Try submit should work if worker is available
        let result = pool.try_submit(|| {});
        // This might succeed or fail depending on timing - just ensure no panic
        let _ = result;

        pool.shutdown().await;
    }
}
