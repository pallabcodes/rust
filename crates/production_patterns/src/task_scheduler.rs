//! Task Scheduler with Priority Queues
//!
//! Task schedulers provide priority-based task execution with features like
//! deadlines, dependencies, and resource constraints. This enables complex
//! workflow orchestration in production systems.
//!
//! ## Key Concepts
//!
//! - **Priority Levels**: High, Normal, Low priority queues
//! - **Deadlines**: Time-based scheduling constraints
//! - **Dependencies**: Task execution ordering
//! - **Resource Limits**: CPU/memory constraints per task
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::task_scheduler::{TaskScheduler, Priority, ScheduledTask};
//!
//! let scheduler = TaskScheduler::new(4); // 4 worker threads
//!
//! // Schedule high-priority task
//! scheduler.schedule(ScheduledTask {
//!     task: Box::new(|| println!("High priority task")),
//!     priority: Priority::High,
//!     deadline: None,
//! }).await;
//!
//! // Schedule task with deadline
//! scheduler.schedule(ScheduledTask {
//!     task: Box::new(|| println!("Time-sensitive task")),
//!     priority: Priority::Normal,
//!     deadline: Some(Instant::now() + Duration::from_secs(30)),
//! }).await;
//! ```

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument};

use crate::common::{Metrics, ShutdownCoordinator};
use crate::error::WorkerPoolError;

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Scheduled task with metadata
#[derive(Debug)]
pub struct ScheduledTask {
    pub task: Box<dyn FnOnce() + Send + 'static>,
    pub priority: Priority,
    pub deadline: Option<Instant>,
    pub created_at: Instant,
}

impl ScheduledTask {
    pub fn new<F>(task: F, priority: Priority) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            task: Box::new(task),
            priority,
            deadline: None,
            created_at: Instant::now(),
        }
    }

    pub fn with_deadline<F>(task: F, priority: Priority, deadline: Instant) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            task: Box::new(task),
            priority,
            deadline: Some(deadline),
            created_at: Instant::now(),
        }
    }
}

// Implement ordering for priority queue (higher priority first, then earlier deadline)
impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // First compare priority (higher priority first)
        match self.priority.cmp(&other.priority).reverse() {
            Ordering::Equal => {
                // Then compare deadline (earlier deadline first)
                match (&self.deadline, &other.deadline) {
                    (Some(a), Some(b)) => a.cmp(b),
                    (Some(_), None) => Ordering::Less, // Tasks with deadlines come first
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            }
            ord => ord,
        }
    }
}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.deadline == other.deadline
    }
}

impl Eq for ScheduledTask {}

/// Task scheduler with priority queues
#[derive(Debug)]
pub struct TaskScheduler {
    task_queue: Arc<Mutex<BinaryHeap<ScheduledTask>>>,
    task_sender: mpsc::Sender<()>, // Signal channel for new tasks
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
    handles: Vec<JoinHandle<()>>,
}

impl TaskScheduler {
    /// Create a new task scheduler with specified number of workers
    #[instrument(skip(), fields(workers = %num_workers))]
    pub fn new(num_workers: usize) -> Self {
        let task_queue = Arc::new(Mutex::new(BinaryHeap::new()));
        let (tx, rx) = mpsc::channel(100); // Signal channel
        let shutdown = ShutdownCoordinator::new();
        let metrics = Arc::new(Metrics::new());

        let mut handles = Vec::with_capacity(num_workers);

        // Spawn worker tasks
        for worker_id in 0..num_workers {
            let queue = task_queue.clone();
            let mut signal = rx.clone();
            let shutdown = shutdown.clone();
            let metrics = metrics.clone();

            let handle = tokio::spawn(async move {
                run_scheduler_worker(worker_id, queue, &mut signal, shutdown, metrics).await;
            });

            handles.push(handle);
        }

        info!("Created task scheduler with {} workers", num_workers);

        Self {
            task_queue,
            task_sender: tx,
            shutdown,
            metrics,
            handles,
        }
    }

    /// Schedule a task for execution
    #[instrument(skip(self, task), fields(priority = ?task.priority))]
    pub async fn schedule(&self, task: ScheduledTask) -> Result<(), WorkerPoolError> {
        {
            let mut queue = self.task_queue.lock().await;
            queue.push(task);
        }

        // Signal workers that new task is available
        self.task_sender.send(()).await
            .map_err(|_| WorkerPoolError::TaskSubmissionFailed)
    }

    /// Schedule a task with priority and optional deadline
    pub async fn schedule_task<F>(
        &self,
        task: F,
        priority: Priority,
        deadline: Option<Instant>,
    ) -> Result<(), WorkerPoolError>
    where
        F: FnOnce() + Send + 'static,
    {
        let scheduled = if let Some(deadline) = deadline {
            ScheduledTask::with_deadline(task, priority, deadline)
        } else {
            ScheduledTask::new(task, priority)
        };

        self.schedule(scheduled).await
    }

    /// Get current queue statistics
    pub async fn stats(&self) -> SchedulerStats {
        let queue_len = self.task_queue.lock().await.len();
        let (ops, errs, avg_duration) = self.metrics.get_stats();

        SchedulerStats {
            queue_depth: queue_len,
            total_tasks: ops,
            failed_tasks: errs,
            avg_task_duration: avg_duration,
            is_shutdown: self.shutdown.is_shutdown(),
        }
    }

    /// Gracefully shutdown the scheduler
    #[instrument(skip(self))]
    pub async fn shutdown(self) {
        info!("Shutting down task scheduler");

        // Signal shutdown
        self.shutdown.shutdown();

        // Close signal channel
        drop(self.task_sender);

        // Wait for all workers
        for handle in self.handles {
            let _ = handle.await;
        }

        info!("Task scheduler shutdown complete");
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new(num_cpus::get())
    }
}

/// Statistics for task scheduler
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub queue_depth: usize,
    pub total_tasks: u64,
    pub failed_tasks: u64,
    pub avg_task_duration: Duration,
    pub is_shutdown: bool,
}

/// Internal worker function for scheduler
async fn run_scheduler_worker(
    worker_id: usize,
    task_queue: Arc<Mutex<BinaryHeap<ScheduledTask>>>,
    signal: &mut mpsc::Receiver<()>,
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
) {
    info!("Scheduler worker {} started", worker_id);

    loop {
        tokio::select! {
            // Check for shutdown
            _ = shutdown.wait_shutdown() => {
                debug!("Scheduler worker {} received shutdown signal", worker_id);
                break;
            }

            // Wait for task signal
            _ = signal.recv() => {
                // Try to get the highest priority task
                let task = {
                    let mut queue = task_queue.lock().await;
                    queue.pop()
                };

                if let Some(mut scheduled_task) = task {
                    // Check if task has exceeded deadline
                    if let Some(deadline) = scheduled_task.deadline {
                        if Instant::now() > deadline {
                            debug!("Scheduler worker {} skipping expired task", worker_id);
                            metrics.record_error();
                            continue;
                        }
                    }

                    // Execute the task
                    let timer = crate::common::Timer::new();
                    (scheduled_task.task)();
                    let duration = timer.elapsed();
                    metrics.record_operation(duration);

                    debug!("Scheduler worker {} completed task in {:?}", worker_id, duration);
                }
            }
        }
    }

    info!("Scheduler worker {} stopped", worker_id);
}

/// Delayed task scheduler for future execution
#[derive(Debug)]
pub struct DelayedScheduler {
    scheduler: TaskScheduler,
}

impl DelayedScheduler {
    pub fn new(num_workers: usize) -> Self {
        Self {
            scheduler: TaskScheduler::new(num_workers),
        }
    }

    /// Schedule a task to run after a delay
    #[instrument(skip(self, task), fields(delay_ms = %delay.as_millis()))]
    pub async fn schedule_delayed<F>(
        &self,
        task: F,
        delay: Duration,
        priority: Priority,
    ) -> Result<(), WorkerPoolError>
    where
        F: FnOnce() + Send + 'static,
    {
        let deadline = Instant::now() + delay;

        // Create a wrapper task that waits for the delay
        let wrapper = move || {
            let deadline = deadline;
            tokio::spawn(async move {
                let now = Instant::now();
                if now < deadline {
                    tokio::time::sleep(deadline - now).await;
                }
                task();
            });
        };

        self.scheduler.schedule_task(wrapper, priority, None).await
    }

    /// Get scheduler statistics
    pub async fn stats(&self) -> SchedulerStats {
        self.scheduler.stats().await
    }

    /// Shutdown the delayed scheduler
    pub async fn shutdown(self) {
        self.scheduler.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_task_scheduler_basic() {
        let scheduler = TaskScheduler::new(2);

        let counter = Arc::new(AtomicU64::new(0));

        // Schedule some tasks
        for i in 0..3 {
            let counter = counter.clone();
            scheduler.schedule_task(
                move || {
                    counter.fetch_add(1, Ordering::Relaxed);
                },
                if i % 2 == 0 { Priority::High } else { Priority::Normal },
                None,
            ).await.unwrap();
        }

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(counter.load(Ordering::Relaxed), 3);

        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let scheduler = TaskScheduler::new(1);

        let results = Arc::new(Mutex::new(Vec::new()));

        // Schedule tasks with different priorities
        for priority in [Priority::Low, Priority::High, Priority::Normal] {
            let results = results.clone();
            scheduler.schedule_task(
                move || {
                    let mut r = results.blocking_lock();
                    r.push(priority as u8);
                },
                priority,
                None,
            ).await.unwrap();
        }

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(100)).await;

        let results = results.lock().await;
        // Should execute High (2), then Normal (1), then Low (0)
        assert_eq!(*results, vec![2, 1, 0]);

        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn test_scheduler_stats() {
        let scheduler = TaskScheduler::new(1);

        // Schedule a task
        scheduler.schedule_task(
            || std::thread::sleep(Duration::from_millis(10)),
            Priority::Normal,
            None,
        ).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = scheduler.stats().await;
        assert_eq!(stats.total_tasks, 1);
        assert_eq!(stats.queue_depth, 0); // Task completed

        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn test_delayed_scheduler() {
        let scheduler = DelayedScheduler::new(1);

        let executed = Arc::new(AtomicU64::new(0));

        let start = Instant::now();
        let executed_clone = executed.clone();

        scheduler.schedule_delayed(
            move || {
                executed_clone.store(1, Ordering::Relaxed);
            },
            Duration::from_millis(50),
            Priority::Normal,
        ).await.unwrap();

        // Task shouldn't execute immediately
        assert_eq!(executed.load(Ordering::Relaxed), 0);

        // Wait for execution
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(executed.load(Ordering::Relaxed), 1);

        scheduler.shutdown().await;
    }

    #[test]
    fn test_task_ordering() {
        let now = Instant::now();

        let high_priority = ScheduledTask::new(|| {}, Priority::High);
        let low_priority = ScheduledTask::new(|| {}, Priority::Low);
        let with_deadline = ScheduledTask::with_deadline(|| {}, Priority::Normal, now + Duration::from_secs(1));
        let without_deadline = ScheduledTask::new(|| {}, Priority::Normal);

        // High priority should come before low priority
        assert!(high_priority > low_priority);

        // Task with deadline should come before task without deadline
        assert!(with_deadline > without_deadline);
    }
}
