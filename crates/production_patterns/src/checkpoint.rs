//! Crash Recovery and Checkpointing
//!
//! Checkpointing enables systems to recover from crashes by saving progress
//! periodically and resuming from the last checkpoint. This is crucial for
//! long-running, stateful operations that cannot afford to restart from scratch.
//!
//! ## Key Concepts
//!
//! - **Checkpoint Storage**: Durable storage of system state
//! - **Recovery Points**: Safe points to resume processing
//! - **Incremental Checkpoints**: Only save changed state
//! - **Crash Recovery**: Automatic resumption after failures
//! - **Idempotent Operations**: Safe replay of operations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::checkpoint::{CheckpointManager, Checkpoint};
//!
//! let manager = CheckpointManager::new();
//!
//! // Create a checkpoint
//! let checkpoint = Checkpoint {
//!     id: "batch-123".to_string(),
//!     data: serde_json::to_value(processed_items)?,
//!     sequence_number: 1000,
//! };
//!
//! manager.save_checkpoint(checkpoint).await?;
//!
//! // Later, recover from checkpoint
//! if let Some(recovered) = manager.load_checkpoint("batch-123").await? {
//!     resume_processing(recovered.data);
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument};

/// Checkpoint data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub data: serde_json::Value,
    pub sequence_number: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
}

impl Checkpoint {
    pub fn new(id: String, data: serde_json::Value, sequence_number: u64) -> Self {
        Self {
            id,
            data,
            sequence_number,
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Checkpoint manager for saving and loading checkpoints
#[derive(Debug, Clone)]
pub struct CheckpointManager {
    checkpoints: Arc<RwLock<HashMap<String, Checkpoint>>>,
    max_checkpoints: usize,
    auto_save_interval: Option<std::time::Duration>,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new() -> Self {
        Self {
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
            max_checkpoints: 1000,
            auto_save_interval: None,
        }
    }

    /// Set maximum number of checkpoints to keep
    pub fn with_max_checkpoints(mut self, max: usize) -> Self {
        self.max_checkpoints = max;
        self
    }

    /// Enable auto-save with interval
    pub fn with_auto_save(mut self, interval: std::time::Duration) -> Self {
        self.auto_save_interval = Some(interval);
        self
    }

    /// Save a checkpoint
    #[instrument(skip(self, checkpoint), fields(checkpoint_id = %checkpoint.id))]
    pub async fn save_checkpoint(&self, checkpoint: Checkpoint) -> Result<(), CheckpointError> {
        let mut checkpoints = self.checkpoints.write().await;

        // Check if we're at capacity and need to evict old checkpoints
        if checkpoints.len() >= self.max_checkpoints {
            // Simple LRU eviction - remove oldest
            let oldest_key = checkpoints
                .iter()
                .min_by_key(|(_, cp)| cp.timestamp)
                .map(|(k, _)| k.clone());

            if let Some(key) = oldest_key {
                checkpoints.remove(&key);
                debug!("Evicted old checkpoint: {}", key);
            }
        }

        checkpoints.insert(checkpoint.id.clone(), checkpoint.clone());

        info!("Saved checkpoint: {} (seq: {})", checkpoint.id, checkpoint.sequence_number);
        Ok(())
    }

    /// Load a checkpoint by ID
    #[instrument(skip(self), fields(checkpoint_id = %id))]
    pub async fn load_checkpoint(&self, id: &str) -> Result<Option<Checkpoint>, CheckpointError> {
        let checkpoints = self.checkpoints.read().await;
        Ok(checkpoints.get(id).cloned())
    }

    /// List all checkpoint IDs
    pub async fn list_checkpoints(&self) -> Vec<String> {
        let checkpoints = self.checkpoints.read().await;
        checkpoints.keys().cloned().collect()
    }

    /// Delete a checkpoint
    #[instrument(skip(self), fields(checkpoint_id = %id))]
    pub async fn delete_checkpoint(&self, id: &str) -> Result<bool, CheckpointError> {
        let mut checkpoints = self.checkpoints.write().await;
        let removed = checkpoints.remove(id).is_some();

        if removed {
            debug!("Deleted checkpoint: {}", id);
        }

        Ok(removed)
    }

    /// Get checkpoint statistics
    pub async fn stats(&self) -> CheckpointStats {
        let checkpoints = self.checkpoints.read().await;

        let total_checkpoints = checkpoints.len();
        let oldest = checkpoints.values().min_by_key(|cp| cp.timestamp);
        let newest = checkpoints.values().max_by_key(|cp| cp.timestamp);

        let total_size_bytes = checkpoints.values()
            .map(|cp| serde_json::to_string(cp).map(|s| s.len()).unwrap_or(0))
            .sum::<usize>();

        CheckpointStats {
            total_checkpoints,
            max_checkpoints: self.max_checkpoints,
            oldest_checkpoint: oldest.map(|cp| cp.timestamp),
            newest_checkpoint: newest.map(|cp| cp.timestamp),
            total_size_bytes,
            utilization: total_checkpoints as f64 / self.max_checkpoints as f64,
        }
    }

    /// Clear all checkpoints
    #[instrument(skip(self))]
    pub async fn clear(&self) {
        let mut checkpoints = self.checkpoints.write().await;
        let count = checkpoints.len();
        checkpoints.clear();
        info!("Cleared {} checkpoints", count);
    }

    /// Start auto-save task if configured
    pub async fn start_auto_save<F>(&self, mut save_fn: F)
    where
        F: FnMut() -> Checkpoint + Send + 'static,
    {
        if let Some(interval) = self.auto_save_interval {
            let manager = self.clone();

            tokio::spawn(async move {
                let mut interval_timer = tokio::time::interval(interval);

                loop {
                    interval_timer.tick().await;

                    let checkpoint = save_fn();
                    if let Err(e) = manager.save_checkpoint(checkpoint).await {
                        error!("Auto-save failed: {}", e);
                    }
                }
            });
        }
    }
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Checkpoint statistics
#[derive(Debug, Clone)]
pub struct CheckpointStats {
    pub total_checkpoints: usize,
    pub max_checkpoints: usize,
    pub oldest_checkpoint: Option<chrono::DateTime<chrono::Utc>>,
    pub newest_checkpoint: Option<chrono::DateTime<chrono::Utc>>,
    pub total_size_bytes: usize,
    pub utilization: f64,
}

/// Checkpoint error types
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("checkpoint not found: {0}")]
    NotFound(String),

    #[error("checkpoint serialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("checkpoint storage full")]
    StorageFull,

    #[error("invalid checkpoint data: {0}")]
    InvalidData(String),
}

/// Resumable operation with checkpointing
#[derive(Debug)]
pub struct ResumableOperation<T, S> {
    manager: CheckpointManager,
    operation_id: String,
    state: Arc<RwLock<S>>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T, S> ResumableOperation<T, S>
where
    S: Serialize + for<'de> Deserialize<'de> + Send + Sync + Clone,
    T: Send + 'static,
{
    /// Create a new resumable operation
    pub fn new(manager: CheckpointManager, operation_id: String, initial_state: S) -> Self {
        Self {
            manager,
            operation_id,
            state: Arc::new(RwLock::new(initial_state)),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Try to resume from existing checkpoint
    pub async fn try_resume(&self) -> Result<Option<S>, CheckpointError> {
        if let Some(checkpoint) = self.manager.load_checkpoint(&self.operation_id).await? {
            let state: S = serde_json::from_value(checkpoint.data)?;
            *self.state.write().await = state.clone();
            info!("Resumed operation {} from sequence {}", self.operation_id, checkpoint.sequence_number);
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    /// Update state and create checkpoint
    #[instrument(skip(self, new_state), fields(operation_id = %self.operation_id))]
    pub async fn update_and_checkpoint(&self, new_state: S, sequence_number: u64) -> Result<(), CheckpointError> {
        *self.state.write().await = new_state.clone();

        let checkpoint = Checkpoint::new(
            self.operation_id.clone(),
            serde_json::to_value(&new_state)?,
            sequence_number,
        );

        self.manager.save_checkpoint(checkpoint).await?;
        debug!("Created checkpoint at sequence {}", sequence_number);

        Ok(())
    }

    /// Get current state
    pub async fn current_state(&self) -> S {
        self.state.read().await.clone()
    }

    /// Get last checkpoint sequence number
    pub async fn last_checkpoint_sequence(&self) -> Option<u64> {
        self.manager.load_checkpoint(&self.operation_id).await
            .ok()
            .flatten()
            .map(|cp| cp.sequence_number)
    }
}

/// Batch processor with checkpointing
#[derive(Debug)]
pub struct CheckpointedBatchProcessor<T> {
    manager: CheckpointManager,
    batch_id: String,
    processed_items: Arc<RwLock<Vec<T>>>,
    batch_size: usize,
}

impl<T> CheckpointedBatchProcessor<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Clone + Send + Sync,
{
    pub fn new(manager: CheckpointManager, batch_id: String, batch_size: usize) -> Self {
        Self {
            manager,
            batch_id,
            processed_items: Arc::new(RwLock::new(Vec::new())),
            batch_size,
        }
    }

    /// Try to resume from checkpoint
    pub async fn try_resume(&self) -> Result<Option<Vec<T>>, CheckpointError> {
        if let Some(checkpoint) = self.manager.load_checkpoint(&self.batch_id).await? {
            let items: Vec<T> = serde_json::from_value(checkpoint.data)?;
            *self.processed_items.write().await = items.clone();
            info!("Resumed batch {} with {} items", self.batch_id, items.len());
            Ok(Some(items))
        } else {
            Ok(None)
        }
    }

    /// Process an item and checkpoint periodically
    #[instrument(skip(self, item), fields(batch_id = %self.batch_id))]
    pub async fn process_item(&self, item: T) -> Result<(), CheckpointError> {
        let mut processed = self.processed_items.write().await;
        processed.push(item);

        // Checkpoint every batch_size items
        if processed.len() % self.batch_size == 0 {
            let checkpoint = Checkpoint::new(
                self.batch_id.clone(),
                serde_json::to_value(&*processed)?,
                processed.len() as u64,
            );

            self.manager.save_checkpoint(checkpoint).await?;
            debug!("Checkpointed batch {} at {} items", self.batch_id, processed.len());
        }

        Ok(())
    }

    /// Finalize processing and save final checkpoint
    #[instrument(skip(self), fields(batch_id = %self.batch_id))]
    pub async fn finalize(&self) -> Result<(), CheckpointError> {
        let processed = self.processed_items.read().await;

        let checkpoint = Checkpoint::new(
            self.batch_id.clone(),
            serde_json::to_value(&*processed)?,
            processed.len() as u64,
        ).with_metadata("final".to_string(), "true".to_string());

        self.manager.save_checkpoint(checkpoint).await?;
        info!("Finalized batch {} with {} items", self.batch_id, processed.len());

        Ok(())
    }

    /// Get current progress
    pub async fn progress(&self) -> usize {
        self.processed_items.read().await.len()
    }

    /// Get processed items
    pub async fn processed_items(&self) -> Vec<T> {
        self.processed_items.read().await.clone()
    }
}

/// Crash recovery coordinator
#[derive(Debug)]
pub struct CrashRecoveryCoordinator {
    manager: CheckpointManager,
    recovery_handlers: Arc<RwLock<HashMap<String, Box<dyn Fn(Checkpoint) -> Result<(), CheckpointError> + Send + Sync>>>>,
}

impl CrashRecoveryCoordinator {
    pub fn new(manager: CheckpointManager) -> Self {
        Self {
            manager,
            recovery_handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a recovery handler for a checkpoint type
    pub async fn register_handler<F>(&self, checkpoint_type: &str, handler: F)
    where
        F: Fn(Checkpoint) -> Result<(), CheckpointError> + Send + Sync + 'static,
    {
        let mut handlers = self.recovery_handlers.write().await;
        handlers.insert(checkpoint_type.to_string(), Box::new(handler));
    }

    /// Perform crash recovery for all checkpoints
    #[instrument(skip(self))]
    pub async fn recover_from_crash(&self) -> Result<RecoveryReport, CheckpointError> {
        let checkpoints = self.manager.list_checkpoints().await;
        let mut recovered = Vec::new();
        let mut failed = Vec::new();

        for checkpoint_id in checkpoints {
            if let Some(checkpoint) = self.manager.load_checkpoint(&checkpoint_id).await? {
                // Try to find a handler based on checkpoint metadata
                let checkpoint_type = checkpoint.metadata
                    .get("type")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string());

                let handlers = self.recovery_handlers.read().await;
                if let Some(handler) = handlers.get(&checkpoint_type) {
                    match handler(checkpoint.clone()) {
                        Ok(()) => {
                            recovered.push(RecoveryResult {
                                checkpoint_id: checkpoint.id,
                                checkpoint_type,
                                status: RecoveryStatus::Success,
                                sequence_number: checkpoint.sequence_number,
                            });
                        }
                        Err(e) => {
                            error!("Recovery failed for checkpoint {}: {}", checkpoint.id, e);
                            failed.push(RecoveryResult {
                                checkpoint_id: checkpoint.id,
                                checkpoint_type,
                                status: RecoveryStatus::Failed(e.to_string()),
                                sequence_number: checkpoint.sequence_number,
                            });
                        }
                    }
                } else {
                    warn!("No recovery handler for checkpoint type: {}", checkpoint_type);
                    failed.push(RecoveryResult {
                        checkpoint_id: checkpoint.id,
                        checkpoint_type,
                        status: RecoveryStatus::NoHandler,
                        sequence_number: checkpoint.sequence_number,
                    });
                }
            }
        }

        Ok(RecoveryReport {
            total_checkpoints: checkpoints.len(),
            recovered: recovered.len(),
            failed: failed.len(),
            recovery_results: recovered.into_iter().chain(failed).collect(),
            recovery_time: Instant::now(),
        })
    }
}

/// Recovery report
#[derive(Debug, Clone)]
pub struct RecoveryReport {
    pub total_checkpoints: usize,
    pub recovered: usize,
    pub failed: usize,
    pub recovery_results: Vec<RecoveryResult>,
    pub recovery_time: Instant,
}

/// Individual recovery result
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    pub checkpoint_id: String,
    pub checkpoint_type: String,
    pub status: RecoveryStatus,
    pub sequence_number: u64,
}

/// Recovery status
#[derive(Debug, Clone)]
pub enum RecoveryStatus {
    Success,
    Failed(String),
    NoHandler,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_checkpoint_save_load() {
        let manager = CheckpointManager::new();

        let checkpoint = Checkpoint::new(
            "test-1".to_string(),
            json!({"processed": 100, "status": "running"}),
            42,
        );

        // Save checkpoint
        manager.save_checkpoint(checkpoint.clone()).await.unwrap();

        // Load checkpoint
        let loaded = manager.load_checkpoint("test-1").await.unwrap().unwrap();

        assert_eq!(loaded.id, "test-1");
        assert_eq!(loaded.sequence_number, 42);
        assert_eq!(loaded.data, json!({"processed": 100, "status": "running"}));
    }

    #[tokio::test]
    async fn test_checkpoint_eviction() {
        let manager = CheckpointManager::new().with_max_checkpoints(2);

        // Save 3 checkpoints
        for i in 0..3 {
            let checkpoint = Checkpoint::new(
                format!("test-{}", i),
                json!({"id": i}),
                i as u64,
            );
            manager.save_checkpoint(checkpoint).await.unwrap();
        }

        // Should only have 2 checkpoints (oldest evicted)
        let checkpoints = manager.list_checkpoints().await;
        assert_eq!(checkpoints.len(), 2);
        assert!(!checkpoints.contains(&"test-0".to_string())); // First one evicted
    }

    #[tokio::test]
    async fn test_resumable_operation() {
        let manager = CheckpointManager::new();

        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct ProcessingState {
            items_processed: u64,
            current_offset: u64,
        }

        let operation = ResumableOperation::new(
            manager,
            "batch-process".to_string(),
            ProcessingState { items_processed: 0, current_offset: 0 },
        );

        // Initially no checkpoint
        assert!(operation.try_resume().await.unwrap().is_none());

        // Update and checkpoint
        let new_state = ProcessingState { items_processed: 50, current_offset: 100 };
        operation.update_and_checkpoint(new_state, 50).await.unwrap();

        // Should be able to resume
        let resumed = operation.try_resume().await.unwrap().unwrap();
        assert_eq!(resumed.items_processed, 50);
        assert_eq!(resumed.current_offset, 100);
    }

    #[tokio::test]
    async fn test_batch_processor() {
        let manager = CheckpointManager::new();
        let processor = CheckpointedBatchProcessor::<i32>::new(manager, "test-batch".to_string(), 3);

        // Process items
        for i in 0..5 {
            processor.process_item(i).await.unwrap();
        }

        // Should have checkpointed at item 3
        let progress = processor.progress().await;
        assert_eq!(progress, 5);

        // Finalize
        processor.finalize().await.unwrap();

        // Should be able to resume
        let resumed = processor.try_resume().await.unwrap().unwrap();
        assert_eq!(resumed, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_crash_recovery() {
        let manager = CheckpointManager::new();
        let coordinator = CrashRecoveryCoordinator::new(manager.clone());

        // Register recovery handler
        coordinator.register_handler("batch", |checkpoint| {
            println!("Recovering batch: {}", checkpoint.id);
            Ok(())
        }).await;

        // Save a checkpoint
        let checkpoint = Checkpoint::new(
            "batch-1".to_string(),
            json!({"status": "processing"}),
            1,
        ).with_metadata("type".to_string(), "batch".to_string());

        manager.save_checkpoint(checkpoint).await.unwrap();

        // Perform recovery
        let report = coordinator.recover_from_crash().await.unwrap();

        assert_eq!(report.total_checkpoints, 1);
        assert_eq!(report.recovered, 1);
        assert_eq!(report.failed, 0);
    }

    #[tokio::test]
    async fn test_checkpoint_stats() {
        let manager = CheckpointManager::new();

        // Save a few checkpoints
        for i in 0..3 {
            let checkpoint = Checkpoint::new(
                format!("cp-{}", i),
                json!({"data": i}),
                i as u64,
            );
            manager.save_checkpoint(checkpoint).await.unwrap();
        }

        let stats = manager.stats().await;
        assert_eq!(stats.total_checkpoints, 3);
        assert!(stats.total_size_bytes > 0);
        assert_eq!(stats.utilization, 3.0 / 1000.0);
    }
}
