//! Resource Leak Detection
//!
//! Resource leaks can cause memory exhaustion, file descriptor exhaustion,
//! and other system failures. This module provides automatic detection and
//! reporting of resource leaks in production systems.
//!
//! ## Key Concepts
//!
//! - **Leak Tracking**: Monitor allocation and deallocation of resources
//! - **Goroutine Leak Detection**: Equivalent to Go's leak detection
//! - **Resource Pools**: Track usage of pooled resources
//! - **Automatic Cleanup**: RAII-based resource management
//! - **Leak Reports**: Detailed reporting of detected leaks
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::leak_detector::{LeakDetector, ResourceTracker};
//!
//! let detector = LeakDetector::new();
//!
//! // Track a resource
//! let resource_id = detector.track_resource("connection");
//!
//! // When done, untrack
//! detector.untrack_resource(resource_id);
//!
//! // Check for leaks
//! let leaks = detector.detect_leaks();
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn, instrument};

/// Resource leak detector
#[derive(Debug, Clone)]
pub struct LeakDetector {
    resources: Arc<RwLock<HashMap<String, ResourceInfo>>>,
    start_time: Instant,
}

impl LeakDetector {
    /// Create a new leak detector
    pub fn new() -> Self {
        Self {
            resources: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
        }
    }

    /// Track a resource allocation
    #[instrument(skip(self))]
    pub async fn track_resource(&self, resource_type: &str) -> ResourceId {
        let mut resources = self.resources.write().await;

        let info = resources.entry(resource_type.to_string()).or_insert_with(|| ResourceInfo {
            resource_type: resource_type.to_string(),
            allocated: 0,
            deallocated: 0,
            active_allocations: HashMap::new(),
        });

        let id = ResourceId {
            resource_type: resource_type.to_string(),
            id: info.allocated,
            allocation_time: Instant::now(),
        };

        info.allocated += 1;
        info.active_allocations.insert(id.id, id.clone());

        debug!("Tracked resource: {}:{}", resource_type, id.id);
        id
    }

    /// Untrack a resource (deallocation)
    #[instrument(skip(self))]
    pub async fn untrack_resource(&self, resource_id: ResourceId) {
        let mut resources = self.resources.write().await;

        if let Some(info) = resources.get_mut(&resource_id.resource_type) {
            if info.active_allocations.remove(&resource_id.id).is_some() {
                info.deallocated += 1;
                debug!("Untracked resource: {}:{}", resource_id.resource_type, resource_id.id);
            } else {
                warn!("Attempted to untrack unknown resource: {}:{}", resource_id.resource_type, resource_id.id);
            }
        }
    }

    /// Detect resource leaks
    #[instrument(skip(self))]
    pub async fn detect_leaks(&self) -> LeakReport {
        let resources = self.resources.read().await;
        let mut leaked_resources = Vec::new();
        let mut summary = HashMap::new();

        for (resource_type, info) in resources.iter() {
            let leak_count = info.allocated - info.deallocated;

            if leak_count > 0 {
                summary.insert(resource_type.clone(), leak_count);

                // Add details of leaked resources
                for (_, resource_id) in &info.active_allocations {
                    leaked_resources.push(LeakedResource {
                        resource_type: resource_type.clone(),
                        id: resource_id.id,
                        allocation_time: resource_id.allocation_time,
                        age: resource_id.allocation_time.elapsed(),
                    });
                }
            }
        }

        LeakReport {
            total_leaks: leaked_resources.len(),
            leaks_by_type: summary,
            leaked_resources,
            detection_time: Instant::now(),
            uptime: self.start_time.elapsed(),
        }
    }

    /// Get current resource statistics
    pub async fn stats(&self) -> ResourceStats {
        let resources = self.resources.read().await;
        let mut stats = HashMap::new();

        for (resource_type, info) in resources.iter() {
            stats.insert(resource_type.clone(), ResourceTypeStats {
                allocated: info.allocated,
                deallocated: info.deallocated,
                active: info.active_allocations.len(),
                leaked: info.allocated - info.deallocated,
            });
        }

        ResourceStats {
            uptime: self.start_time.elapsed(),
            resources: stats,
        }
    }

    /// Reset all tracking (for testing)
    #[instrument(skip(self))]
    pub async fn reset(&self) {
        let mut resources = self.resources.write().await;
        resources.clear();
        info!("Leak detector reset");
    }
}

impl Default for LeakDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a tracked resource
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceId {
    pub resource_type: String,
    pub id: u64,
    pub allocation_time: Instant,
}

/// Information about a resource type
#[derive(Debug)]
struct ResourceInfo {
    resource_type: String,
    allocated: u64,
    deallocated: u64,
    active_allocations: HashMap<u64, ResourceId>,
}

/// Report of detected resource leaks
#[derive(Debug, Clone)]
pub struct LeakReport {
    pub total_leaks: usize,
    pub leaks_by_type: HashMap<String, u64>,
    pub leaked_resources: Vec<LeakedResource>,
    pub detection_time: Instant,
    pub uptime: Duration,
}

/// Information about a leaked resource
#[derive(Debug, Clone)]
pub struct LeakedResource {
    pub resource_type: String,
    pub id: u64,
    pub allocation_time: Instant,
    pub age: Duration,
}

/// Resource statistics
#[derive(Debug, Clone)]
pub struct ResourceStats {
    pub uptime: Duration,
    pub resources: HashMap<String, ResourceTypeStats>,
}

/// Statistics for a specific resource type
#[derive(Debug, Clone)]
pub struct ResourceTypeStats {
    pub allocated: u64,
    pub deallocated: u64,
    pub active: usize,
    pub leaked: u64,
}

/// RAII wrapper for automatic resource tracking
#[derive(Debug)]
pub struct TrackedResource<T> {
    resource: Option<T>,
    detector: LeakDetector,
    resource_id: Option<ResourceId>,
}

impl<T> TrackedResource<T> {
    /// Create a new tracked resource
    pub async fn new(resource: T, detector: LeakDetector, resource_type: &str) -> Self {
        let resource_id = detector.track_resource(resource_type).await;

        Self {
            resource: Some(resource),
            detector,
            resource_id: Some(resource_id),
        }
    }

    /// Get a reference to the resource
    pub fn get(&self) -> &T {
        self.resource.as_ref().unwrap()
    }

    /// Get a mutable reference to the resource
    pub fn get_mut(&mut self) -> &mut T {
        self.resource.as_mut().unwrap()
    }

    /// Take ownership of the resource (stops tracking)
    pub fn take(mut self) -> T {
        self.resource.take().unwrap()
    }
}

impl<T> Drop for TrackedResource<T> {
    fn drop(&mut self) {
        if let Some(resource_id) = self.resource_id.take() {
            // Spawn a task to untrack asynchronously to avoid blocking drop
            let detector = self.detector.clone();
            tokio::spawn(async move {
                detector.untrack_resource(resource_id).await;
            });
        }
    }
}

/// Goroutine leak detector (equivalent to Go's leak detector)
#[derive(Debug)]
pub struct GoroutineLeakDetector {
    detector: LeakDetector,
    active_tasks: Arc<RwLock<HashSet<String>>>,
}

impl GoroutineLeakDetector {
    pub fn new() -> Self {
        Self {
            detector: LeakDetector::new(),
            active_tasks: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Track a new goroutine/task
    #[instrument(skip(self))]
    pub async fn track_task(&self, task_name: &str) -> TaskHandle {
        let resource_id = self.detector.track_resource("goroutine").await;

        let mut active_tasks = self.active_tasks.write().await;
        active_tasks.insert(task_name.to_string());

        debug!("Tracking goroutine: {}", task_name);

        TaskHandle {
            name: task_name.to_string(),
            detector: self.detector.clone(),
            resource_id,
            active_tasks: self.active_tasks.clone(),
        }
    }

    /// Detect goroutine leaks
    pub async fn detect_leaks(&self) -> LeakReport {
        self.detector.detect_leaks().await
    }

    /// Get active task names
    pub async fn active_tasks(&self) -> HashSet<String> {
        self.active_tasks.read().await.clone()
    }

    /// Start periodic leak checking
    pub async fn start_periodic_check(&self, interval: Duration) {
        let detector = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);

            loop {
                interval.tick().await;

                let report = detector.detect_leaks().await;
                if report.total_leaks > 0 {
                    warn!("Detected {} resource leaks", report.total_leaks);

                    for (resource_type, count) in &report.leaks_by_type {
                        warn!("  {}: {} leaks", resource_type, count);
                    }

                    // Log details of leaked goroutines
                    for leaked in &report.leaked_resources {
                        if leaked.resource_type == "goroutine" {
                            warn!("  Leaked goroutine id={} age={:?}",
                                  leaked.id, leaked.age);
                        }
                    }
                }
            }
        });
    }
}

impl Default for GoroutineLeakDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for GoroutineLeakDetector {
    fn clone(&self) -> Self {
        Self {
            detector: self.detector.clone(),
            active_tasks: self.active_tasks.clone(),
        }
    }
}

/// Handle for tracking task lifetime
#[derive(Debug)]
pub struct TaskHandle {
    name: String,
    detector: LeakDetector,
    resource_id: ResourceId,
    active_tasks: Arc<RwLock<HashSet<String>>>,
}

impl TaskHandle {
    /// Mark task as completed
    pub async fn complete(self) {
        let mut active_tasks = self.active_tasks.write().await;
        active_tasks.remove(&self.name);

        self.detector.untrack_resource(self.resource_id).await;
        debug!("Completed task: {}", self.name);
    }
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        // If not explicitly completed, log it as a potential leak
        let active_tasks = self.active_tasks.clone();
        let name = self.name.clone();

        tokio::spawn(async move {
            let mut active_tasks = active_tasks.write().await;
            active_tasks.remove(&name);
        });

        // Note: We can't call async untrack here, but the leak detector will catch it
        warn!("TaskHandle dropped without explicit completion: {}", self.name);
    }
}

/// Connection pool leak detector
#[derive(Debug)]
pub struct ConnectionPoolLeakDetector {
    detector: LeakDetector,
    pool_size: usize,
}

impl ConnectionPoolLeakDetector {
    pub fn new(pool_size: usize) -> Self {
        Self {
            detector: LeakDetector::new(),
            pool_size,
        }
    }

    /// Track connection acquisition
    pub async fn acquire_connection(&self, connection_id: &str) -> ConnectionHandle {
        let resource_id = self.detector.track_resource("connection").await;

        ConnectionHandle {
            id: connection_id.to_string(),
            detector: self.detector.clone(),
            resource_id,
        }
    }

    /// Check for connection leaks
    pub async fn check_leaks(&self) -> PoolLeakReport {
        let leak_report = self.detector.detect_leaks().await;

        let connection_leaks = leak_report.leaks_by_type
            .get("connection")
            .copied()
            .unwrap_or(0);

        let utilization = if self.pool_size > 0 {
            (self.pool_size - connection_leaks as usize) as f64 / self.pool_size as f64
        } else {
            0.0
        };

        PoolLeakReport {
            pool_size: self.pool_size,
            active_connections: self.pool_size - connection_leaks as usize,
            leaked_connections: connection_leaks as usize,
            utilization,
            leak_report,
        }
    }

    /// Get pool statistics
    pub async fn stats(&self) -> ConnectionPoolStats {
        let leak_report = self.check_leaks().await;

        ConnectionPoolStats {
            pool_size: self.pool_size,
            active_connections: leak_report.active_connections,
            utilization: leak_report.utilization,
            total_leaks: leak_report.leaked_connections,
        }
    }
}

/// Report for connection pool leaks
#[derive(Debug, Clone)]
pub struct PoolLeakReport {
    pub pool_size: usize,
    pub active_connections: usize,
    pub leaked_connections: usize,
    pub utilization: f64,
    pub leak_report: LeakReport,
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct ConnectionPoolStats {
    pub pool_size: usize,
    pub active_connections: usize,
    pub utilization: f64,
    pub total_leaks: usize,
}

/// RAII handle for connection tracking
#[derive(Debug)]
pub struct ConnectionHandle {
    id: String,
    detector: LeakDetector,
    resource_id: ResourceId,
}

impl ConnectionHandle {
    /// Get connection ID
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        let detector = self.detector.clone();
        let resource_id = self.resource_id.clone();

        tokio::spawn(async move {
            detector.untrack_resource(resource_id).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_leak_detector_basic() {
        let detector = LeakDetector::new();

        // Track some resources
        let id1 = detector.track_resource("connection").await;
        let id2 = detector.track_resource("connection").await;
        let id3 = detector.track_resource("file").await;

        // Untrack one
        detector.untrack_resource(id1).await;

        // Check leaks
        let report = detector.detect_leaks().await;

        assert_eq!(report.total_leaks, 2); // id2 and id3 still tracked
        assert_eq!(report.leaks_by_type["connection"], 1);
        assert_eq!(report.leaks_by_type["file"], 1);
    }

    #[tokio::test]
    async fn test_tracked_resource() {
        let detector = LeakDetector::new();

        {
            let _resource = TrackedResource::new(42, detector.clone(), "test").await;
            // Resource should be tracked here
        }
        // Resource should be untracked after drop

        tokio::time::sleep(Duration::from_millis(10)).await; // Allow async cleanup

        let report = detector.detect_leaks().await;
        assert_eq!(report.total_leaks, 0);
    }

    #[tokio::test]
    async fn test_goroutine_leak_detector() {
        let detector = GoroutineLeakDetector::new();

        // Track a task
        let handle = detector.track_task("test-task").await;

        // Task should be active
        let active = detector.active_tasks().await;
        assert!(active.contains("test-task"));

        // Complete the task
        handle.complete().await;

        // Task should no longer be active
        let active = detector.active_tasks().await;
        assert!(!active.contains("test-task"));

        // No leaks
        let report = detector.detect_leaks().await;
        assert_eq!(report.total_leaks, 0);
    }

    #[tokio::test]
    async fn test_connection_pool_leak_detector() {
        let pool_detector = ConnectionPoolLeakDetector::new(10);

        // Acquire some connections
        let _conn1 = pool_detector.acquire_connection("conn1").await;
        let _conn2 = pool_detector.acquire_connection("conn2").await;

        // Check stats
        let stats = pool_detector.stats().await;
        assert_eq!(stats.pool_size, 10);
        assert_eq!(stats.active_connections, 8); // 2 connections leaked
        assert_eq!(stats.total_leaks, 2);
    }

    #[tokio::test]
    async fn test_resource_stats() {
        let detector = LeakDetector::new();

        let _id1 = detector.track_resource("type1").await;
        let _id2 = detector.track_resource("type1").await;
        let _id3 = detector.track_resource("type2").await;

        detector.untrack_resource(_id1).await;

        let stats = detector.stats().await;

        assert_eq!(stats.resources["type1"].allocated, 2);
        assert_eq!(stats.resources["type1"].deallocated, 1);
        assert_eq!(stats.resources["type1"].active, 1);
        assert_eq!(stats.resources["type1"].leaked, 1);

        assert_eq!(stats.resources["type2"].allocated, 1);
        assert_eq!(stats.resources["type2"].deallocated, 0);
        assert_eq!(stats.resources["type2"].active, 1);
        assert_eq!(stats.resources["type2"].leaked, 1);
    }

    #[tokio::test]
    async fn test_leak_detection_with_drop() {
        let detector = LeakDetector::new();

        // Create a handle but don't explicitly untrack
        let handle = detector.track_resource("test").await;

        // Drop the handle without untracking
        drop(handle);

        // Should detect the leak
        let report = detector.detect_leaks().await;
        assert_eq!(report.total_leaks, 1);
    }
}
