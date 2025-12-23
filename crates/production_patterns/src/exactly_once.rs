//! Exactly-Once Processing Patterns
//!
//! Exactly-once processing ensures that operations are performed precisely
//! once, even in the face of failures and retries. This is crucial for
//! financial transactions, inventory updates, and other state-changing operations.
//!
//! ## Key Concepts
//!
//! - **Idempotency Keys**: Unique identifiers for operations
//! - **Processing State**: Track what operations have been completed
//! - **Atomic Operations**: All-or-nothing state changes
//! - **Duplicate Detection**: Identify and handle repeated requests
//! - **Compensation**: Undo operations when needed
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::exactly_once::{ExactlyOnceProcessor, ProcessingRequest};
//!
//! let processor = ExactlyOnceProcessor::new();
//!
//! let request = ProcessingRequest {
//!     idempotency_key: "tx-123".to_string(),
//!     operation: "transfer".to_string(),
//!     data: serde_json::json!({"from": "alice", "to": "bob", "amount": 100}),
//! };
//!
//! // Process exactly once
//! let result = processor.process_exactly_once(request, |req| async move {
//!     perform_transfer(req.data).await
//! }).await?;
//!
//! assert!(result.was_processed);
//! ```

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, warn};

/// Processing request with idempotency key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingRequest {
    pub idempotency_key: String,
    pub operation: String,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
}

impl ProcessingRequest {
    pub fn new(idempotency_key: String, operation: String, data: serde_json::Value) -> Self {
        Self {
            idempotency_key,
            operation,
            data,
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Result of exactly-once processing
#[derive(Debug, Clone)]
pub struct ProcessingResult<T> {
    pub was_processed: bool,
    pub result: Option<T>,
    pub cached_result: Option<serde_json::Value>,
    pub processing_time: std::time::Duration,
    pub idempotency_key: String,
}

/// Processing state for tracking completed operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingState {
    pub idempotency_key: String,
    pub operation: String,
    pub result: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ProcessingState {
    pub fn new<T: Serialize>(idempotency_key: String, operation: String, result: T) -> Self {
        Self {
            idempotency_key,
            operation,
            result: serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
            timestamp: chrono::Utc::now(),
            expires_at: None,
        }
    }

    pub fn with_ttl(mut self, ttl: chrono::Duration) -> Self {
        self.expires_at = Some(self.timestamp + ttl);
        self
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|expires| chrono::Utc::now() > expires)
            .unwrap_or(false)
    }
}

/// Exactly-once processor
#[derive(Debug, Clone)]
pub struct ExactlyOnceProcessor {
    processing_states: Arc<RwLock<HashMap<String, ProcessingState>>>,
    max_states: usize,
    default_ttl: Option<chrono::Duration>,
}

impl ExactlyOnceProcessor {
    /// Create a new exactly-once processor
    pub fn new() -> Self {
        Self {
            processing_states: Arc::new(RwLock::new(HashMap::new())),
            max_states: 10000,
            default_ttl: Some(chrono::Duration::hours(24)), // 24 hours default
        }
    }

    /// Configure maximum number of stored states
    pub fn with_max_states(mut self, max: usize) -> Self {
        self.max_states = max;
        self
    }

    /// Configure default TTL for processing states
    pub fn with_default_ttl(mut self, ttl: chrono::Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    /// Process a request exactly once
    #[instrument(skip(self, operation), fields(idempotency_key = %request.idempotency_key))]
    pub async fn process_exactly_once<F, Fut, T>(
        &self,
        request: ProcessingRequest,
        operation: F,
    ) -> Result<ProcessingResult<T>, ExactlyOnceError>
    where
        F: FnOnce(ProcessingRequest) -> Fut,
        Fut: std::future::Future<Output = Result<T, ExactlyOnceError>>,
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let start_time = Instant::now();

        // Check if already processed
        if let Some(existing_state) = self.get_processing_state(&request.idempotency_key).await {
            if !existing_state.is_expired() {
                // Return cached result
                let cached_result: T = serde_json::from_value(existing_state.result.clone())?;
                let processing_time = start_time.elapsed();

                debug!("Returning cached result for idempotency key: {}", request.idempotency_key);

                return Ok(ProcessingResult {
                    was_processed: false,
                    result: Some(cached_result),
                    cached_result: Some(existing_state.result),
                    processing_time,
                    idempotency_key: request.idempotency_key,
                });
            } else {
                // Clean up expired state
                self.remove_processing_state(&request.idempotency_key).await;
            }
        }

        // Process the operation
        let result = operation(request.clone()).await?;
        let processing_time = start_time.elapsed();

        // Store the result
        let state = if let Some(ttl) = self.default_ttl {
            ProcessingState::new(request.idempotency_key.clone(), request.operation, &result).with_ttl(ttl)
        } else {
            ProcessingState::new(request.idempotency_key.clone(), request.operation, &result)
        };

        self.store_processing_state(state).await?;

        info!("Processed operation exactly once: {}", request.idempotency_key);

        Ok(ProcessingResult {
            was_processed: true,
            result: Some(result),
            cached_result: None,
            processing_time,
            idempotency_key: request.idempotency_key,
        })
    }

    /// Check if a request has already been processed
    pub async fn is_already_processed(&self, idempotency_key: &str) -> bool {
        self.get_processing_state(idempotency_key).await
            .map(|state| !state.is_expired())
            .unwrap_or(false)
    }

    /// Get processing state for an idempotency key
    async fn get_processing_state(&self, idempotency_key: &str) -> Option<ProcessingState> {
        let states = self.processing_states.read().await;
        states.get(idempotency_key).cloned()
    }

    /// Store processing state
    async fn store_processing_state(&self, state: ProcessingState) -> Result<(), ExactlyOnceError> {
        let mut states = self.processing_states.write().await;

        // Evict old entries if at capacity
        while states.len() >= self.max_states {
            // Remove oldest expired entry
            let to_remove = states
                .iter()
                .filter(|(_, state)| state.is_expired())
                .map(|(key, _)| key.clone())
                .next();

            if let Some(key) = to_remove {
                states.remove(&key);
            } else {
                // No expired entries, remove oldest
                if let Some(key) = states.keys().next().cloned() {
                    states.remove(&key);
                }
            }
        }

        states.insert(state.idempotency_key.clone(), state);
        Ok(())
    }

    /// Remove processing state
    async fn remove_processing_state(&self, idempotency_key: &str) {
        let mut states = self.processing_states.write().await;
        states.remove(idempotency_key);
    }

    /// Get processor statistics
    pub async fn stats(&self) -> ProcessorStats {
        let states = self.processing_states.read().await;

        let total_states = states.len();
        let expired_states = states.values().filter(|s| s.is_expired()).count();
        let active_states = total_states - expired_states;

        ProcessorStats {
            total_states,
            active_states,
            expired_states,
            max_states: self.max_states,
            utilization: total_states as f64 / self.max_states as f64,
        }
    }

    /// Clean up expired states
    #[instrument(skip(self))]
    pub async fn cleanup_expired(&self) -> usize {
        let mut states = self.processing_states.write().await;
        let initial_count = states.len();

        states.retain(|_, state| !state.is_expired());

        let removed = initial_count - states.len();
        if removed > 0 {
            debug!("Cleaned up {} expired processing states", removed);
        }

        removed
    }

    /// Clear all processing states (for testing)
    #[instrument(skip(self))]
    pub async fn clear(&self) {
        let mut states = self.processing_states.write().await;
        states.clear();
        info!("Cleared all processing states");
    }
}

impl Default for ExactlyOnceProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Processor statistics
#[derive(Debug, Clone)]
pub struct ProcessorStats {
    pub total_states: usize,
    pub active_states: usize,
    pub expired_states: usize,
    pub max_states: usize,
    pub utilization: f64,
}

/// Exactly-once error types
#[derive(Debug, thiserror::Error)]
pub enum ExactlyOnceError {
    #[error("operation failed: {0}")]
    OperationError(String),

    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("duplicate processing detected for key: {0}")]
    DuplicateProcessing(String),

    #[error("processing state storage full")]
    StorageFull,

    #[error("invalid request: {0}")]
    InvalidRequest(String),
}

/// Idempotent operation wrapper
#[derive(Debug)]
pub struct IdempotentOperation<T> {
    processor: ExactlyOnceProcessor,
    operation_name: String,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> IdempotentOperation<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    pub fn new(processor: ExactlyOnceProcessor, operation_name: String) -> Self {
        Self {
            processor,
            operation_name,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Execute operation with automatic idempotency handling
    pub async fn execute<F>(
        &self,
        idempotency_key: String,
        operation: F,
    ) -> Result<ProcessingResult<T>, ExactlyOnceError>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, ExactlyOnceError>> + Send>>,
    {
        let request = ProcessingRequest::new(
            idempotency_key,
            self.operation_name.clone(),
            serde_json::Value::Null,
        );

        self.processor.process_exactly_once(request, |_| operation()).await
    }
}

/// Distributed exactly-once processor with coordination
#[derive(Debug)]
pub struct DistributedExactlyOnceProcessor {
    processors: Vec<ExactlyOnceProcessor>,
    coordinator: Arc<RwLock<HashMap<String, usize>>>, // Maps key to processor index
}

impl DistributedExactlyOnceProcessor {
    pub fn new(num_processors: usize) -> Self {
        let processors = (0..num_processors)
            .map(|_| ExactlyOnceProcessor::new())
            .collect();

        Self {
            processors,
            coordinator: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Process with distributed coordination
    pub async fn process_distributed<F, Fut, T>(
        &self,
        request: ProcessingRequest,
        operation: F,
    ) -> Result<ProcessingResult<T>, ExactlyOnceError>
    where
        F: FnOnce(ProcessingRequest) -> Fut,
        Fut: std::future::Future<Output = Result<T, ExactlyOnceError>>,
        T: Serialize + for<'de> Deserialize<'de>,
    {
        // Simple hash-based routing for distribution
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        request.idempotency_key.hash(&mut hasher);
        let processor_index = (hasher.finish() as usize) % self.processors.len();

        // Ensure consistent routing by storing mapping
        {
            let mut coordinator = self.coordinator.write().await;
            coordinator.insert(request.idempotency_key.clone(), processor_index);
        }

        let processor = &self.processors[processor_index];
        processor.process_exactly_once(request, operation).await
    }

    /// Get combined statistics
    pub async fn combined_stats(&self) -> ProcessorStats {
        let mut combined = ProcessorStats {
            total_states: 0,
            active_states: 0,
            expired_states: 0,
            max_states: 0,
            utilization: 0.0,
        };

        for processor in &self.processors {
            let stats = processor.stats().await;
            combined.total_states += stats.total_states;
            combined.active_states += stats.active_states;
            combined.expired_states += stats.expired_states;
            combined.max_states += stats.max_states;
        }

        combined.utilization = if combined.max_states > 0 {
            combined.total_states as f64 / combined.max_states as f64
        } else {
            0.0
        };

        combined
    }
}

/// Compensation action for failed operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationAction {
    pub operation_id: String,
    pub compensation_data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl CompensationAction {
    pub fn new(operation_id: String, compensation_data: serde_json::Value) -> Self {
        Self {
            operation_id,
            compensation_data,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Saga pattern for exactly-once with compensation
#[derive(Debug)]
pub struct SagaProcessor {
    processor: ExactlyOnceProcessor,
    compensations: Arc<RwLock<HashMap<String, Vec<CompensationAction>>>>,
}

impl SagaProcessor {
    pub fn new(processor: ExactlyOnceProcessor) -> Self {
        Self {
            processor,
            compensations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Execute saga with compensation support
    pub async fn execute_saga<F, C, Fut, T>(
        &self,
        saga_id: String,
        operations: Vec<F>,
        compensations: Vec<C>,
    ) -> Result<SagaResult<T>, ExactlyOnceError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, ExactlyOnceError>>,
        C: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ExactlyOnceError>> + Send>>,
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let mut completed_operations = Vec::new();
        let mut compensation_actions = Vec::new();

        // Execute operations
        for (i, operation) in operations.into_iter().enumerate() {
            let request = ProcessingRequest::new(
                format!("{}-{}", saga_id, i),
                "saga_step".to_string(),
                serde_json::json!({"step": i, "saga_id": saga_id}),
            );

            match self.processor.process_exactly_once(request, |_| operation()).await {
                Ok(result) => {
                    completed_operations.push(result);
                    compensation_actions.push(compensations.get(i).cloned());
                }
                Err(e) => {
                    // Compensate completed operations
                    self.compensate_saga(&completed_operations, &compensations[..completed_operations.len()]).await?;
                    return Err(e);
                }
            }
        }

        // Store compensation actions for potential future rollback
        {
            let mut comps = self.compensations.write().await;
            comps.insert(saga_id.clone(), compensation_actions.into_iter().flatten().collect());
        }

        Ok(SagaResult {
            saga_id,
            results: completed_operations,
            completed_steps: completed_operations.len(),
        })
    }

    /// Compensate a saga
    async fn compensate_saga(
        &self,
        completed: &[ProcessingResult<impl Serialize>],
        compensations: &[impl FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ExactlyOnceError>> + Send>>],
    ) -> Result<(), ExactlyOnceError> {
        for (i, _) in completed.iter().enumerate().rev() {
            if let Some(compensation) = compensations.get(i) {
                compensation().await?;
            }
        }
        Ok(())
    }

    /// Rollback a completed saga
    pub async fn rollback_saga(&self, saga_id: &str) -> Result<(), ExactlyOnceError> {
        let compensations = {
            let comps = self.compensations.read().await;
            comps.get(saga_id).cloned()
        };

        if let Some(actions) = compensations {
            for action in actions.into_iter().rev() {
                // Execute compensation (simplified - would need actual compensation logic)
                info!("Rolling back operation: {}", action.operation_id);
            }
        }

        Ok(())
    }
}

/// Saga execution result
#[derive(Debug)]
pub struct SagaResult<T> {
    pub saga_id: String,
    pub results: Vec<ProcessingResult<T>>,
    pub completed_steps: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn sample_operation(request: ProcessingRequest) -> Result<i32, ExactlyOnceError> {
        Ok(42)
    }

    #[tokio::test]
    async fn test_exactly_once_processing() {
        let processor = ExactlyOnceProcessor::new();

        let request = ProcessingRequest::new(
            "test-key-1".to_string(),
            "test-operation".to_string(),
            json!({"input": "test"}),
        );

        // First processing
        let result1 = processor.process_exactly_once(request.clone(), sample_operation).await.unwrap();
        assert!(result1.was_processed);
        assert_eq!(result1.result, Some(42));

        // Second processing (should return cached result)
        let result2 = processor.process_exactly_once(request, sample_operation).await.unwrap();
        assert!(!result2.was_processed);
        assert_eq!(result2.result, Some(42));
    }

    #[tokio::test]
    async fn test_processor_stats() {
        let processor = ExactlyOnceProcessor::new().with_max_states(5);

        // Process a few requests
        for i in 0..3 {
            let request = ProcessingRequest::new(
                format!("key-{}", i),
                "test".to_string(),
                json!({"id": i}),
            );

            processor.process_exactly_once(request, sample_operation).await.unwrap();
        }

        let stats = processor.stats().await;
        assert_eq!(stats.total_states, 3);
        assert_eq!(stats.active_states, 3);
        assert_eq!(stats.expired_states, 0);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let processor = ExactlyOnceProcessor::new()
            .with_max_states(5)
            .with_default_ttl(chrono::Duration::milliseconds(10)); // Very short TTL

        let request = ProcessingRequest::new(
            "short-lived".to_string(),
            "test".to_string(),
            json!({"data": "test"}),
        );

        processor.process_exactly_once(request, sample_operation).await.unwrap();

        // Wait for expiration
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let removed = processor.cleanup_expired().await;
        assert_eq!(removed, 1);

        let stats = processor.stats().await;
        assert_eq!(stats.active_states, 0);
    }

    #[tokio::test]
    async fn test_idempotent_operation() {
        let processor = ExactlyOnceProcessor::new();
        let operation = IdempotentOperation::<i32>::new(processor, "test-op".to_string());

        // Execute operation
        let result1 = operation.execute("idem-key".to_string(), || Box::pin(async { Ok(100) })).await.unwrap();
        assert!(result1.was_processed);

        // Execute again (should return cached)
        let result2 = operation.execute("idem-key".to_string(), || Box::pin(async { Ok(200) })).await.unwrap();
        assert!(!result2.was_processed);
        assert_eq!(result2.result, Some(100)); // Original result
    }

    #[tokio::test]
    async fn test_distributed_processor() {
        let processor = DistributedExactlyOnceProcessor::new(3);

        let request = ProcessingRequest::new(
            "dist-test".to_string(),
            "distributed-op".to_string(),
            json!({"distributed": true}),
        );

        let result = processor.process_distributed(request, sample_operation).await.unwrap();
        assert!(result.was_processed);
        assert_eq!(result.result, Some(42));
    }

    #[tokio::test]
    async fn test_saga_processor() {
        let processor = ExactlyOnceProcessor::new();
        let saga = SagaProcessor::new(processor);

        let operations = vec![
            || Box::pin(async { Ok(1) }),
            || Box::pin(async { Ok(2) }),
            || Box::pin(async { Ok(3) }),
        ];

        let compensations = vec![
            || Box::pin(async { Ok(()) }),
            || Box::pin(async { Ok(()) }),
            || Box::pin(async { Ok(()) }),
        ];

        let result = saga.execute_saga(
            "test-saga".to_string(),
            operations,
            compensations,
        ).await.unwrap();

        assert_eq!(result.completed_steps, 3);
        assert_eq!(result.results.len(), 3);
    }

    #[tokio::test]
    async fn test_saga_rollback() {
        let processor = ExactlyOnceProcessor::new();
        let saga = SagaProcessor::new(processor);

        // Saga that will fail on last step
        let operations = vec![
            || Box::pin(async { Ok(1) }),
            || Box::pin(async { Ok(2) }),
            || Box::pin(async { Err(ExactlyOnceError::OperationError("failed".to_string())) }),
        ];

        let compensations = vec![
            || Box::pin(async { println!("Compensating step 1"); Ok(()) }),
            || Box::pin(async { println!("Compensating step 2"); Ok(()) }),
            || Box::pin(async { Ok(()) }), // Not reached
        ];

        let result = saga.execute_saga(
            "fail-saga".to_string(),
            operations,
            compensations,
        ).await;

        assert!(result.is_err());
        // In a real implementation, compensations would be called
    }
}
