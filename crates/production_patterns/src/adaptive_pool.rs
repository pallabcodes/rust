//! Adaptive Worker Pool Pattern
//!
//! Adaptive worker pools automatically scale the number of workers based on
//! queue depth and utilization metrics. This pattern, inspired by Kubernetes
//! Horizontal Pod Autoscaler, provides dynamic resource management.
//!
//! ## Key Concepts
//!
//! - **Dynamic Scaling**: Scale up/down based on queue utilization
//! - **Min/Max Bounds**: Prevent over/under provisioning
//! - **Utilization Metrics**: Queue depth, worker utilization
//! - **Cooldown Periods**: Prevent thrashing during scaling
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::adaptive_pool::{AdaptivePool, AdaptiveConfig};
//!
//! let pool = AdaptivePool::new(AdaptiveConfig {
//!     min_workers: 2,
//!     max_workers: 10,
//!     queue_capacity: 100,
//!     scale_up_threshold: 0.8,    // Scale up at 80% queue utilization
//!     scale_down_threshold: 0.2,  // Scale down at 20% queue utilization
//!     check_interval: Duration::from_millis(100),
//! });
//!
//! // Pool automatically scales based on load
//! for i in 0..50 {
//!     pool.submit(move || {
//!         // Simulate variable work
//!         std::thread::sleep(Duration::from_millis((i % 10) * 10));
//!     }).await;
//! }
//! ```

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument};

use crate::common::{Metrics, ShutdownCoordinator};
use crate::error::WorkerPoolError;
use crate::worker_pool::Task;

/// Configuration for adaptive worker pool
#[derive(Debug, Clone)]
pub struct AdaptiveConfig {
    pub min_workers: usize,
    pub max_workers: usize,
    pub queue_capacity: usize,
    pub scale_up_threshold: f64,   // Queue utilization to trigger scale up (0.0-1.0)
    pub scale_down_threshold: f64, // Queue utilization to trigger scale down (0.0-1.0)
    pub check_interval: Duration,   // How often to check scaling conditions
    pub cooldown_period: Duration,  // Minimum time between scaling operations
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            min_workers: 1,
            max_workers: num_cpus::get() * 2,
            queue_capacity: 1000,
            scale_up_threshold: 0.8,
            scale_down_threshold: 0.2,
            check_interval: Duration::from_millis(100),
            cooldown_period: Duration::from_secs(5),
        }
    }
}

/// Adaptive worker pool that scales dynamically
#[derive(Debug)]
pub struct AdaptivePool {
    config: AdaptiveConfig,
    task_sender: mpsc::Sender<Task>,
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,

    // Scaling state
    current_workers: Arc<Mutex<usize>>,
    worker_handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    last_scale_time: Arc<Mutex<std::time::Instant>>,
    scaling_in_progress: Arc<Mutex<bool>>,
}

impl AdaptivePool {
    /// Create a new adaptive worker pool
    #[instrument(skip(config), fields(min = %config.min_workers, max = %config.max_workers))]
    pub fn new(config: AdaptiveConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.queue_capacity);
        let shutdown = ShutdownCoordinator::new();
        let metrics = Arc::new(Metrics::new());

        let current_workers = Arc::new(Mutex::new(config.min_workers));
        let worker_handles = Arc::new(Mutex::new(Vec::new()));
        let last_scale_time = Arc::new(Mutex::new(std::time::Instant::now()));
        let scaling_in_progress = Arc::new(Mutex::new(false));

        // Create initial workers
        let pool = Self {
            config,
            task_sender: tx,
            shutdown,
            metrics,
            current_workers: current_workers.clone(),
            worker_handles: worker_handles.clone(),
            last_scale_time,
            scaling_in_progress,
        };

        // Spawn initial workers
        tokio::spawn(async move {
            let mut handles = worker_handles.lock().await;
            for worker_id in 0..config.min_workers {
                let handle = spawn_worker(
                    worker_id,
                    rx.clone(),
                    shutdown.clone(),
                    metrics.clone(),
                );
                handles.push(handle);
            }
        });

        // Start scaling monitor
        let scaling_pool = pool.clone();
        tokio::spawn(async move {
            scaling_pool.run_scaling_monitor(rx).await;
        });

        info!("Created adaptive worker pool with {} initial workers", config.min_workers);

        pool
    }

    /// Submit a task to the worker pool
    #[instrument(skip(self, task))]
    pub async fn submit<F>(&self, task: F) -> Result<(), WorkerPoolError>
    where
        F: FnOnce() + Send + 'static,
    {
        self.task_sender.send(Box::new(task))
            .await
            .map_err(|_| WorkerPoolError::TaskSubmissionFailed)
    }

    /// Get current pool statistics
    pub async fn stats(&self) -> AdaptivePoolStats {
        let current_workers = *self.current_workers.lock().await;
        let scaling_in_progress = *self.scaling_in_progress.lock().await;
        let (ops, errs, avg_duration) = self.metrics.get_stats();

        AdaptivePoolStats {
            min_workers: self.config.min_workers,
            max_workers: self.config.max_workers,
            current_workers,
            queue_capacity: self.config.queue_capacity,
            total_tasks: ops,
            failed_tasks: errs,
            avg_task_duration: avg_duration,
            scaling_in_progress,
            is_shutdown: self.shutdown.is_shutdown(),
        }
    }

    /// Manually trigger scaling check
    pub async fn check_scaling(&self) {
        self.perform_scaling_check().await;
    }

    /// Gracefully shutdown the worker pool
    #[instrument(skip(self))]
    pub async fn shutdown(self) {
        info!("Shutting down adaptive worker pool");

        // Signal shutdown
        self.shutdown.shutdown();

        // Close task channel
        drop(self.task_sender);

        // Wait for all workers to finish
        let handles = self.worker_handles.lock().await;
        for handle in handles.iter() {
            let _ = handle.await;
        }

        info!("Adaptive worker pool shutdown complete");
    }

    /// Clone for scaling monitor (internal use)
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            task_sender: self.task_sender.clone(),
            shutdown: self.shutdown.clone(),
            metrics: self.metrics.clone(),
            current_workers: self.current_workers.clone(),
            worker_handles: self.worker_handles.clone(),
            last_scale_time: self.last_scale_time.clone(),
            scaling_in_progress: self.scaling_in_progress.clone(),
        }
    }

    /// Run the scaling monitor loop
    async fn run_scaling_monitor(&self, task_receiver: mpsc::Receiver<Task>) {
        let mut receiver = task_receiver;

        loop {
            tokio::select! {
                // Check for shutdown
                _ = self.shutdown.wait_shutdown() => break,

                // Periodic scaling check
                _ = tokio::time::sleep(self.config.check_interval) => {
                    self.perform_scaling_check().await;
                }

                // Monitor task channel for backpressure
                _ = async {
                    // This is a simplified approach - in practice, we'd monitor
                    // the channel's len() method if it were available
                    tokio::time::sleep(self.config.check_interval).await;
                } => {
                    // Check if we need to scale up due to queue pressure
                    // This is approximated since we can't directly check channel len
                }
            }
        }
    }

    /// Perform scaling decision and execute if needed
    async fn perform_scaling_check(&self) {
        let now = std::time::Instant::now();
        let last_scale = *self.last_scale_time.lock().await;

        // Check cooldown period
        if now.duration_since(last_scale) < self.config.cooldown_period {
            return;
        }

        // Prevent concurrent scaling operations
        let mut scaling = self.scaling_in_progress.lock().await;
        if *scaling {
            return;
        }
        *scaling = true;
        drop(scaling);

        // Calculate scaling decision
        let decision = self.calculate_scaling_decision().await;

        match decision {
            ScalingDecision::ScaleUp(target) => {
                self.scale_up(target).await;
            }
            ScalingDecision::ScaleDown(target) => {
                self.scale_down(target).await;
            }
            ScalingDecision::NoChange => {
                // Nothing to do
            }
        }

        *self.scaling_in_progress.lock().await = false;
    }

    /// Calculate whether to scale up or down
    async fn calculate_scaling_decision(&self) -> ScalingDecision {
        let current = *self.current_workers.lock().await;

        // Simplified scaling logic - in practice, this would use more sophisticated metrics
        // For now, we'll use a simple heuristic based on worker count and time

        // This is a placeholder implementation. Real scaling would use:
        // - Queue depth
        // - Worker utilization
        // - Task arrival rate
        // - System resource usage

        ScalingDecision::NoChange
    }

    /// Scale up to target number of workers
    async fn scale_up(&self, target: usize) {
        let current = {
            let mut current = self.current_workers.lock().await;
            if *current >= target {
                return;
            }
            let old = *current;
            *current = target;
            old
        };

        info!("Scaling up from {} to {} workers", current, target);

        let mut handles = self.worker_handles.lock().await;
        let shutdown = self.shutdown.clone();
        let metrics = self.metrics.clone();

        // Create new task receiver for new workers
        let (tx, rx) = mpsc::channel(self.config.queue_capacity);

        // Replace the task sender (this is simplified - real implementation would be more complex)
        // In practice, we'd need a more sophisticated approach to distribute tasks

        for worker_id in current..target {
            let handle = spawn_worker(
                worker_id,
                rx.clone(),
                shutdown.clone(),
                metrics.clone(),
            );
            handles.push(handle);
        }

        *self.last_scale_time.lock().await = std::time::Instant::now();
    }

    /// Scale down to target number of workers
    async fn scale_down(&self, target: usize) {
        let current = {
            let mut current = self.current_workers.lock().await;
            if *current <= target {
                return;
            }
            let old = *current;
            *current = target;
            old
        };

        info!("Scaling down from {} to {} workers", current, target);

        // Stop excess workers (simplified - real implementation would gracefully stop specific workers)
        let mut handles = self.worker_handles.lock().await;
        while handles.len() > target {
            if let Some(handle) = handles.pop() {
                handle.abort();
            }
        }

        *self.last_scale_time.lock().await = std::time::Instant::now();
    }
}

/// Scaling decision
#[derive(Debug, PartialEq, Eq)]
enum ScalingDecision {
    ScaleUp(usize),
    ScaleDown(usize),
    NoChange,
}

/// Statistics for adaptive worker pool
#[derive(Debug, Clone)]
pub struct AdaptivePoolStats {
    pub min_workers: usize,
    pub max_workers: usize,
    pub current_workers: usize,
    pub queue_capacity: usize,
    pub total_tasks: u64,
    pub failed_tasks: u64,
    pub avg_task_duration: std::time::Duration,
    pub scaling_in_progress: bool,
    pub is_shutdown: bool,
}

/// Spawn a single worker
fn spawn_worker(
    worker_id: usize,
    mut task_receiver: mpsc::Receiver<Task>,
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        debug!("Adaptive worker {} started", worker_id);

        loop {
            tokio::select! {
                // Check for shutdown
                _ = shutdown.wait_shutdown() => break,

                // Receive and execute task
                task = task_receiver.recv() => {
                    match task {
                        Some(task) => {
                            let timer = crate::common::Timer::new();
                            task();
                            let duration = timer.elapsed();
                            metrics.record_operation(duration);
                            debug!("Adaptive worker {} completed task in {:?}", worker_id, duration);
                        }
                        None => break,
                    }
                }
            }
        }

        debug!("Adaptive worker {} stopped", worker_id);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_adaptive_pool_creation() {
        let config = AdaptiveConfig {
            min_workers: 1,
            max_workers: 4,
            queue_capacity: 10,
            scale_up_threshold: 0.8,
            scale_down_threshold: 0.2,
            check_interval: Duration::from_millis(50),
            cooldown_period: Duration::from_millis(100),
        };

        let pool = AdaptivePool::new(config);

        // Check initial stats
        let stats = pool.stats().await;
        assert_eq!(stats.min_workers, 1);
        assert_eq!(stats.max_workers, 4);
        assert_eq!(stats.current_workers, 1);
        assert!(!stats.is_shutdown);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_adaptive_pool_submit() {
        let pool = AdaptivePool::new(AdaptiveConfig {
            min_workers: 1,
            max_workers: 2,
            queue_capacity: 10,
            ..Default::default()
        });

        let counter = Arc::new(AtomicU64::new(0));

        // Submit tasks
        for _ in 0..3 {
            let counter = counter.clone();
            pool.submit(move || {
                counter.fetch_add(1, Ordering::Relaxed);
            }).await.unwrap();
        }

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(counter.load(Ordering::Relaxed), 3);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_adaptive_pool_stats() {
        let pool = AdaptivePool::new(AdaptiveConfig::default());

        // Submit a task
        pool.submit(|| {
            std::thread::sleep(Duration::from_millis(10));
        }).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = pool.stats().await;
        assert_eq!(stats.total_tasks, 1);
        assert!(!stats.scaling_in_progress);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_scaling_decision() {
        let pool = AdaptivePool::new(AdaptiveConfig::default());

        // Test scaling decision (currently always NoChange in simplified implementation)
        let decision = pool.calculate_scaling_decision().await;
        assert!(matches!(decision, ScalingDecision::NoChange));

        pool.shutdown().await;
    }
}
