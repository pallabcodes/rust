//! Graceful Shutdown Patterns
//!
//! Graceful shutdown ensures that systems can terminate cleanly, completing
//! in-flight work and releasing resources properly. This is critical for
//! maintaining data consistency and avoiding resource leaks.
//!
//! ## Key Concepts
//!
//! - **Shutdown Coordinator**: Central coordination of shutdown process
//! - **Phased Shutdown**: Ordered shutdown of components
//! - **Timeout Handling**: Force termination after grace period
//! - **Signal Handling**: Respond to system signals (SIGTERM, SIGINT)
//! - **Resource Cleanup**: Ensure all resources are properly released
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::graceful_shutdown::ShutdownManager;
//!
//! let shutdown = ShutdownManager::new(Duration::from_secs(30));
//!
//! // Register shutdown handlers
//! shutdown.register("database", || async {
//!     // Cleanup database connections
//!     Ok(())
//! });
//!
//! shutdown.register("workers", || async {
//!     // Stop worker pools
//!     Ok(())
//! });
//!
//! // Wait for shutdown signal
//! shutdown.wait().await;
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::{Mutex, Notify, watch};
use tokio::time::{timeout, Instant};
use tracing::{debug, error, info, instrument, warn};

use crate::common::ShutdownCoordinator;

/// Shutdown manager for coordinating graceful shutdown
#[derive(Debug)]
pub struct ShutdownManager {
    coordinator: ShutdownCoordinator,
    handlers: Arc<Mutex<Vec<ShutdownHandler>>>,
    timeout: Duration,
    start_time: Instant,
}

type ShutdownHandler = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), ShutdownError>> + Send>> + Send>;

impl ShutdownManager {
    /// Create a new shutdown manager
    pub fn new(timeout: Duration) -> Self {
        Self {
            coordinator: ShutdownCoordinator::new(),
            handlers: Arc::new(Mutex::new(Vec::new())),
            timeout,
            start_time: Instant::now(),
        }
    }

    /// Register a shutdown handler
    #[instrument(skip(self, handler))]
    pub async fn register<F, Fut>(&self, name: &str, handler: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ShutdownError>> + Send + 'static,
    {
        let mut handlers = self.handlers.lock().await;
        handlers.push(Box::new(move || Box::pin(handler())));
        debug!("Registered shutdown handler: {}", name);
    }

    /// Start signal handling and wait for shutdown
    #[instrument(skip(self))]
    pub async fn wait(self) {
        info!("Shutdown manager started, waiting for shutdown signal...");

        tokio::select! {
            _ = self.coordinator.wait_shutdown() => {
                info!("Received shutdown signal via coordinator");
            }
            _ = self.handle_signals() => {
                info!("Received system shutdown signal");
            }
        }

        self.perform_shutdown().await;
    }

    /// Trigger shutdown manually
    pub fn trigger_shutdown(&self) {
        info!("Triggering manual shutdown");
        self.coordinator.shutdown();
    }

    /// Check if shutdown has been initiated
    pub fn is_shutdown(&self) -> bool {
        self.coordinator.is_shutdown()
    }

    /// Get shutdown timeout
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Handle system signals
    async fn handle_signals() {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl-c");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to listen for SIGTERM")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                info!("Received Ctrl+C signal");
            }
            _ = terminate => {
                info!("Received SIGTERM signal");
            }
        }
    }

    /// Perform the actual shutdown process
    #[instrument(skip(self))]
    async fn perform_shutdown(self) {
        info!("Starting graceful shutdown (timeout: {:?})", self.timeout);

        let shutdown_start = Instant::now();
        let mut results = Vec::new();

        // Execute all shutdown handlers with timeout
        let handlers = self.handlers.lock().await.clone(); // Clone to release lock

        for (i, handler) in handlers.into_iter().enumerate() {
            let handler_name = format!("handler-{}", i);

            let result = timeout(self.timeout, handler()).await;

            match result {
                Ok(Ok(())) => {
                    debug!("Shutdown handler {} completed successfully", handler_name);
                    results.push(Ok(()));
                }
                Ok(Err(e)) => {
                    error!("Shutdown handler {} failed: {}", handler_name, e);
                    results.push(Err(e));
                }
                Err(_) => {
                    warn!("Shutdown handler {} timed out", handler_name);
                    results.push(Err(ShutdownError::Timeout));
                }
            }
        }

        let shutdown_duration = shutdown_start.elapsed();
        info!("Shutdown completed in {:?}", shutdown_duration);

        // Report results
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let failure_count = results.len() - success_count;

        if failure_count > 0 {
            error!("Shutdown completed with {} failures out of {} handlers", failure_count, results.len());
        } else {
            info!("Shutdown completed successfully for all {} handlers", success_count);
        }
    }
}

/// Phased shutdown manager for complex systems
#[derive(Debug)]
pub struct PhasedShutdown {
    phases: Vec<ShutdownPhase>,
    coordinator: ShutdownCoordinator,
    timeout: Duration,
}

#[derive(Debug)]
pub struct ShutdownPhase {
    name: String,
    handlers: Vec<ShutdownHandler>,
    priority: i32, // Higher priority = executed first
}

impl PhasedShutdown {
    pub fn new(timeout: Duration) -> Self {
        Self {
            phases: Vec::new(),
            coordinator: ShutdownCoordinator::new(),
            timeout,
        }
    }

    /// Add a shutdown phase
    pub fn add_phase(&mut self, name: &str, priority: i32) -> &mut ShutdownPhase {
        self.phases.push(ShutdownPhase {
            name: name.to_string(),
            handlers: Vec::new(),
            priority,
        });
        self.phases.last_mut().unwrap()
    }

    /// Register a handler in a specific phase
    pub fn register_in_phase<F, Fut>(&mut self, phase_name: &str, handler: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ShutdownError>> + Send + 'static,
    {
        if let Some(phase) = self.phases.iter_mut().find(|p| p.name == phase_name) {
            phase.handlers.push(Box::new(move || Box::pin(handler())));
        }
    }

    /// Execute phased shutdown
    #[instrument(skip(self))]
    pub async fn shutdown(self) {
        info!("Starting phased shutdown");

        // Sort phases by priority (highest first)
        let mut phases = self.phases;
        phases.sort_by(|a, b| b.priority.cmp(&a.priority));

        for phase in phases {
            info!("Executing shutdown phase: {} (priority: {})", phase.name, phase.priority);

            let mut phase_results = Vec::new();

            // Execute all handlers in this phase concurrently
            let mut tasks = Vec::new();
            for handler in phase.handlers {
                let timeout_duration = self.timeout;
                let task = tokio::spawn(async move {
                    timeout(timeout_duration, handler()).await
                });
                tasks.push(task);
            }

            // Wait for all handlers in this phase to complete
            for task in tasks {
                match task.await {
                    Ok(Ok(Ok(()))) => phase_results.push(Ok(())),
                    Ok(Ok(Err(e))) => phase_results.push(Err(e)),
                    Ok(Err(_)) => phase_results.push(Err(ShutdownError::Timeout)),
                    Err(e) => phase_results.push(Err(ShutdownError::HandlerPanic(e.to_string()))),
                }
            }

            let success_count = phase_results.iter().filter(|r| r.is_ok()).count();
            let failure_count = phase_results.len() - success_count;

            if failure_count > 0 {
                warn!("Phase {} completed with {} failures", phase.name, failure_count);
            } else {
                info!("Phase {} completed successfully", phase.name);
            }
        }

        info!("Phased shutdown completed");
    }
}

impl ShutdownPhase {
    /// Add a handler to this phase
    pub fn add_handler<F, Fut>(&mut self, handler: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ShutdownError>> + Send + 'static,
    {
        self.handlers.push(Box::new(move || Box::pin(handler())));
    }
}

/// Resource manager with automatic cleanup
#[derive(Debug)]
pub struct ResourceManager<T> {
    resource: Arc<Mutex<Option<T>>>,
    cleanup: Option<Box<dyn FnOnce(T) + Send + 'static>>,
}

impl<T> ResourceManager<T> {
    pub fn new(resource: T) -> Self {
        Self {
            resource: Arc::new(Mutex::new(Some(resource))),
            cleanup: None,
        }
    }

    pub fn with_cleanup<F>(mut self, cleanup: F) -> Self
    where
        F: FnOnce(T) + Send + 'static,
    {
        self.cleanup = Some(Box::new(cleanup));
        self
    }

    pub async fn get(&self) -> Option<T>
    where
        T: Clone,
    {
        self.resource.lock().await.clone()
    }

    pub async fn take(&self) -> Option<T> {
        self.resource.lock().await.take()
    }

    pub async fn cleanup(self) {
        if let Some(resource) = self.take().await {
            if let Some(cleanup) = self.cleanup {
                cleanup(resource);
            }
        }
    }
}

/// Shutdown error types
#[derive(Debug, thiserror::Error)]
pub enum ShutdownError {
    #[error("shutdown handler failed: {0}")]
    HandlerError(String),

    #[error("shutdown handler panicked: {0}")]
    HandlerPanic(String),

    #[error("shutdown timed out")]
    Timeout,

    #[error("resource cleanup failed: {0}")]
    ResourceError(String),

    #[error("shutdown coordinator error")]
    CoordinatorError,
}

/// Shutdown guard that triggers cleanup when dropped
#[derive(Debug)]
pub struct ShutdownGuard {
    manager: Arc<ShutdownManager>,
}

impl ShutdownGuard {
    pub fn new(manager: Arc<ShutdownManager>) -> Self {
        Self { manager }
    }
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        if !self.manager.is_shutdown() {
            warn!("ShutdownGuard dropped without proper shutdown");
            self.manager.trigger_shutdown();
        }
    }
}

/// Utility for creating shutdown-aware tasks
pub mod shutdown_utils {
    use super::*;

    /// Spawn a task that will be cancelled on shutdown
    pub fn spawn_with_shutdown<F>(
        shutdown: &ShutdownCoordinator,
        task: F,
    ) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(async move {
            tokio::select! {
                result = task => result,
                _ = shutdown.wait_shutdown() => {
                    debug!("Task cancelled due to shutdown");
                    // Return default value for the task type
                    panic!("Task was cancelled during shutdown")
                }
            }
        })
    }

    /// Create a watch channel for shutdown coordination
    pub fn shutdown_channel() -> (watch::Sender<bool>, watch::Receiver<bool>) {
        watch::channel(false)
    }

    /// Wait for either task completion or shutdown
    pub async fn race_shutdown<F>(
        shutdown: &ShutdownCoordinator,
        task: F,
    ) -> Result<F::Output, ShutdownError>
    where
        F: Future + Send,
    {
        tokio::select! {
            result = task => Ok(result),
            _ = shutdown.wait_shutdown() => Err(ShutdownError::CoordinatorError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_shutdown_manager() {
        let manager = ShutdownManager::new(Duration::from_secs(1));

        static COUNTER: AtomicU64 = AtomicU64::new(0);

        // Register a handler
        manager.register("test", || async {
            COUNTER.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }).await;

        // Trigger shutdown
        manager.trigger_shutdown();

        // Wait should complete quickly
        let result = timeout(Duration::from_millis(100), manager.wait()).await;
        assert!(result.is_ok());

        assert_eq!(COUNTER.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_phased_shutdown() {
        let mut shutdown = PhasedShutdown::new(Duration::from_secs(1));

        // Add phases
        shutdown.add_phase("phase1", 1).add_handler(|| async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(())
        });

        shutdown.add_phase("phase2", 2).add_handler(|| async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok(())
        });

        // Phase 2 (higher priority) should execute first
        let result = timeout(Duration::from_millis(200), shutdown.shutdown()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resource_manager() {
        let resource = ResourceManager::new(42);

        // Get resource
        assert_eq!(resource.get().await, Some(42));

        // Take resource
        assert_eq!(resource.take().await, Some(42));
        assert_eq!(resource.get().await, None);
    }

    #[tokio::test]
    async fn test_shutdown_timeout() {
        let manager = ShutdownManager::new(Duration::from_millis(10));

        // Register slow handler
        manager.register("slow", || async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }).await;

        manager.trigger_shutdown();

        // Should timeout
        let result = timeout(Duration::from_millis(50), manager.wait()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_utils_spawn() {
        let coordinator = ShutdownCoordinator::new();

        let handle = shutdown_utils::spawn_with_shutdown(&coordinator, async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            42
        });

        // Trigger shutdown before task completes
        coordinator.shutdown();

        // Task should be cancelled
        let result = timeout(Duration::from_millis(50), handle).await;
        assert!(result.is_ok()); // Should complete due to cancellation
    }

    #[tokio::test]
    async fn test_race_shutdown() {
        let coordinator = ShutdownCoordinator::new();

        let task = tokio::time::sleep(Duration::from_millis(100));

        // Shutdown should win the race
        coordinator.shutdown();

        let result = shutdown_utils::race_shutdown(&coordinator, task).await;
        assert!(matches!(result, Err(ShutdownError::CoordinatorError)));
    }
}
