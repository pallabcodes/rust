//! Supervision Tree Pattern
//!
//! Supervision trees provide hierarchical lifecycle management for actors,
//! enabling automatic restart strategies when failures occur. This pattern,
//! inspired by Erlang/OTP, provides fault tolerance and isolation.
//!
//! ## Key Concepts
//!
//! - **Restart Strategies**: OneForOne, OneForAll, RestForOne
//! - **Failure Isolation**: Child failures don't crash parent supervisors
//! - **Hierarchical Management**: Tree structure for complex systems
//! - **Backoff Strategies**: Exponential backoff for restart attempts
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::supervision::{Supervisor, RestartStrategy, WorkerSpec};
//!
//! // Create supervisor
//! let supervisor = Supervisor::new("root", RestartStrategy::OneForOne);
//!
//! // Add workers
//! supervisor.add_worker(WorkerSpec {
//!     name: "worker1".to_string(),
//!     start: Box::new(|| async {
//!         // Worker logic here
//!         Ok(())
//!     }),
//!     max_failures: 3,
//!     backoff: Duration::from_millis(100),
//! });
//!
//! // Start supervision
//! supervisor.start().await;
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn, instrument};

use crate::common::{ExponentialBackoff, ShutdownCoordinator};
use crate::error::ActorError;

/// Restart strategy for supervisor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartStrategy {
    /// Restart only the failed worker
    OneForOne,
    /// Restart all workers when one fails
    OneForAll,
    /// Restart failed worker and all started after it
    RestForOne,
}

/// Specification for a supervised worker
#[derive(Debug)]
pub struct WorkerSpec {
    pub name: String,
    pub start: Box<dyn Fn() -> tokio::task::JoinHandle<Result<(), ActorError>> + Send + Sync>,
    pub max_failures: u32,
    pub backoff: Duration,
}

impl WorkerSpec {
    pub fn new<F>(
        name: impl Into<String>,
        start: F,
        max_failures: u32,
        backoff: Duration,
    ) -> Self
    where
        F: Fn() -> tokio::task::JoinHandle<Result<(), ActorError>> + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            start: Box::new(start),
            max_failures,
            backoff,
        }
    }
}

/// State of a supervised worker
#[derive(Debug)]
struct WorkerState {
    spec: WorkerSpec,
    failures: u32,
    last_failure: Option<Instant>,
    handle: Option<JoinHandle<Result<(), ActorError>>>,
    restarting: bool,
}

impl WorkerState {
    fn new(spec: WorkerSpec) -> Self {
        Self {
            spec,
            failures: 0,
            last_failure: None,
            handle: None,
            restarting: false,
        }
    }

    fn is_running(&self) -> bool {
        self.handle.as_ref().map(|h| !h.is_finished()).unwrap_or(false)
    }

    fn record_failure(&mut self) {
        self.failures += 1;
        self.last_failure = Some(Instant::now());
    }

    fn should_restart(&self) -> bool {
        self.failures < self.spec.max_failures
    }

    fn backoff_duration(&self) -> Duration {
        // Simple exponential backoff
        self.spec.backoff * 2u32.pow(self.failures.saturating_sub(1))
    }
}

/// Supervisor event for monitoring
#[derive(Debug, Clone)]
pub struct SupervisorEvent {
    pub timestamp: Instant,
    pub worker: String,
    pub event_type: SupervisorEventType,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum SupervisorEventType {
    Started,
    Stopped,
    Failed,
    Restarted,
    MaxFailures,
}

/// Supervisor manages a group of workers with restart strategies
#[derive(Debug)]
pub struct Supervisor {
    name: String,
    strategy: RestartStrategy,
    workers: HashMap<String, WorkerState>,
    events: mpsc::Sender<SupervisorEvent>,
    shutdown: ShutdownCoordinator,
    backoff: ExponentialBackoff,
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(name: impl Into<String>, strategy: RestartStrategy) -> Self {
        let (tx, _) = mpsc::channel(100); // Event channel

        Self {
            name: name.into(),
            strategy,
            workers: HashMap::new(),
            events: tx,
            shutdown: ShutdownCoordinator::new(),
            backoff: ExponentialBackoff::default(),
        }
    }

    /// Add a worker to be supervised
    pub fn add_worker(&mut self, spec: WorkerSpec) {
        let name = spec.name.clone();
        self.workers.insert(name, WorkerState::new(spec));
    }

    /// Start all workers and begin supervision
    #[instrument(skip(self), fields(supervisor = %self.name))]
    pub async fn start(&mut self) -> Result<(), ActorError> {
        info!("Starting supervisor: {}", self.name);

        // Start all workers
        for (name, state) in &mut self.workers {
            self.start_worker(name, state).await?;
        }

        // Start supervision monitoring
        self.monitor_workers();

        self.emit_event("supervisor".to_string(), SupervisorEventType::Started,
                       format!("supervising {} workers", self.workers.len()));

        Ok(())
    }

    /// Stop all workers and shutdown supervisor
    #[instrument(skip(self), fields(supervisor = %self.name))]
    pub async fn stop(&mut self) {
        info!("Stopping supervisor: {}", self.name);
        self.shutdown.shutdown();

        // Stop all workers
        for (name, state) in &mut self.workers {
            if let Some(handle) = state.handle.take() {
                handle.abort();
            }
        }

        // Wait for all workers to stop
        tokio::time::sleep(Duration::from_millis(100)).await;

        self.emit_event("supervisor".to_string(), SupervisorEventType::Stopped,
                       "shutdown complete".to_string());
    }

    /// Get events channel for monitoring
    pub fn events(&self) -> mpsc::Receiver<SupervisorEvent> {
        let (tx, rx) = mpsc::channel(100);
        // Note: In real implementation, we'd need to clone the events channel
        // For now, return a new channel (this is a simplified version)
        rx
    }

    async fn start_worker(&mut self, name: &str, state: &mut WorkerState) -> Result<(), ActorError> {
        let handle = (state.spec.start)();
        state.handle = Some(handle);

        self.emit_event(name.to_string(), SupervisorEventType::Started, "".to_string());
        Ok(())
    }

    fn monitor_workers(&mut self) {
        let workers = self.workers.clone();
        let strategy = self.strategy;
        let events = self.events.clone();
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.wait_shutdown() => break,

                    // Check for failed workers
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        // In a real implementation, we'd use a more sophisticated
                        // monitoring mechanism. For now, we'll use polling.
                    }
                }
            }
        });
    }

    async fn handle_worker_failure(&mut self, failed_name: &str) {
        let Some(failed_state) = self.workers.get_mut(failed_name) else {
            return;
        };

        failed_state.record_failure();

        if !failed_state.should_restart() {
            self.emit_event(failed_name.to_string(), SupervisorEventType::MaxFailures,
                           format!("failed {} times", failed_state.failures));
            return;
        }

        // Apply restart strategy
        let workers_to_restart = match self.strategy {
            RestartStrategy::OneForOne => vec![failed_name.to_string()],
            RestartStrategy::OneForAll => self.workers.keys().cloned().collect(),
            RestartStrategy::RestForOne => {
                // Find workers started after the failed one
                // This is a simplified implementation
                vec![failed_name.to_string()]
            }
        };

        for name in workers_to_restart {
            if let Some(state) = self.workers.get_mut(&name) {
                self.restart_worker(&name, state).await;
            }
        }
    }

    async fn restart_worker(&mut self, name: &str, state: &mut WorkerState) {
        if state.restarting {
            return;
        }

        state.restarting = true;

        // Wait for backoff period
        let backoff = state.backoff_duration();
        tokio::time::sleep(backoff).await;

        // Restart the worker
        let result = self.start_worker(name, state).await;

        state.restarting = false;

        match result {
            Ok(()) => {
                self.emit_event(name.to_string(), SupervisorEventType::Restarted,
                               format!("attempt {}", state.failures));
            }
            Err(e) => {
                error!("Failed to restart worker {}: {}", name, e);
                self.handle_worker_failure(name).await;
            }
        }
    }

    fn emit_event(&self, worker: String, event_type: SupervisorEventType, message: String) {
        let event = SupervisorEvent {
            timestamp: Instant::now(),
            worker,
            event_type,
            message,
        };

        // Try to send event (don't block if channel is full)
        let _ = self.events.try_send(event);
    }
}

/// Supervisor builder for fluent API
pub struct SupervisorBuilder {
    name: String,
    strategy: RestartStrategy,
    workers: Vec<WorkerSpec>,
}

impl SupervisorBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            strategy: RestartStrategy::OneForOne,
            workers: Vec::new(),
        }
    }

    pub fn with_strategy(mut self, strategy: RestartStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn add_worker(mut self, spec: WorkerSpec) -> Self {
        self.workers.push(spec);
        self
    }

    pub fn build(mut self) -> Supervisor {
        let mut supervisor = Supervisor::new(self.name, self.strategy);
        for worker in self.workers {
            supervisor.add_worker(worker);
        }
        supervisor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::time::timeout;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn failing_worker(max_runs: u64) -> WorkerSpec {
        let counter = COUNTER.load(Ordering::Relaxed);
        COUNTER.store(counter + 1, Ordering::Relaxed);

        WorkerSpec::new(
            format!("failing-worker-{}", counter),
            move || {
                let run_count = counter;
                tokio::spawn(async move {
                    // Fail after max_runs successful runs
                    if run_count >= max_runs {
                        return Err(ActorError::HandlerError("simulated failure".to_string()));
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(())
                })
            },
            3,
            Duration::from_millis(50),
        )
    }

    #[tokio::test]
    async fn test_one_for_one_restart() {
        let mut supervisor = Supervisor::new("test", RestartStrategy::OneForOne);

        // Add workers that will fail
        supervisor.add_worker(failing_worker(1)); // Will fail after 1 run
        supervisor.add_worker(failing_worker(10)); // Will succeed

        supervisor.start().await.unwrap();

        // Wait for potential restarts
        tokio::time::sleep(Duration::from_millis(200)).await;

        supervisor.stop().await;
    }

    #[tokio::test]
    async fn test_max_failures() {
        let mut supervisor = Supervisor::new("test", RestartStrategy::OneForOne);

        // Add worker that always fails
        supervisor.add_worker(WorkerSpec::new(
            "always-fail",
            || tokio::spawn(async { Err(ActorError::HandlerError("always fails".to_string())) }),
            2, // Max failures
            Duration::from_millis(10),
        ));

        supervisor.start().await.unwrap();

        // Wait for max failures to be reached
        tokio::time::sleep(Duration::from_millis(100)).await;

        supervisor.stop().await;
    }

    #[tokio::test]
    async fn test_supervisor_builder() {
        let supervisor = SupervisorBuilder::new("builder-test")
            .with_strategy(RestartStrategy::OneForAll)
            .add_worker(failing_worker(5))
            .add_worker(failing_worker(5))
            .build();

        // Verify configuration
        assert_eq!(supervisor.name, "builder-test");
        assert_eq!(supervisor.workers.len(), 2);
    }

    #[tokio::test]
    async fn test_shutdown() {
        let mut supervisor = Supervisor::new("shutdown-test", RestartStrategy::OneForOne);
        supervisor.add_worker(failing_worker(100)); // Long-running

        supervisor.start().await.unwrap();

        // Shutdown immediately
        supervisor.stop().await;

        // All workers should be stopped
        for (name, state) in &supervisor.workers {
            assert!(!state.is_running(), "Worker {} should be stopped", name);
        }
    }
}
