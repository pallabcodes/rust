//! Comprehensive Pipeline Patterns
//!
//! Pipelines combine multiple processing stages with flow control,
//! error handling, and monitoring. This module provides high-level
//! pipeline composition patterns for building complex data processing systems.
//!
//! ## Key Concepts
//!
//! - **Pipeline Composition**: Chain processing stages
//! - **Error Propagation**: Handle errors across stages
//! - **Backpressure**: Flow control between stages
//! - **Metrics Integration**: Monitor pipeline performance
//! - **Graceful Shutdown**: Coordinated pipeline termination
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::pipeline::{Pipeline, PipelineStage};
//!
//! let pipeline = Pipeline::new()
//!     .add_stage("validate", |input| async move {
//!         validate(input).await
//!     })
//!     .add_stage("process", |input| async move {
//!         process(input).await
//!     })
//!     .add_stage("store", |input| async move {
//!         store(input).await
//!     })
//!     .build();
//!
//! // Process items through pipeline
//! pipeline.process_items(items).await;
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, instrument, warn};

use crate::common::{Metrics, ShutdownCoordinator};
use crate::fan_out_fan_in::{FanOutFanIn, ProcessingResult};
use crate::batching::BatchProcessor;

/// Processing stage in a pipeline
#[derive(Debug)]
pub struct PipelineStage<I, O> {
    name: String,
    processor: Box<dyn Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<O, PipelineError>> + Send>> + Send + Sync>,
    metrics: Arc<Metrics>,
}

impl<I, O> PipelineStage<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    pub fn new<F>(name: impl Into<String>, processor: F) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<O, PipelineError>> + Send>> + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            processor: Box::new(processor),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Process a single item through this stage
    async fn process(&self, input: I) -> Result<O, PipelineError> {
        let start_time = Instant::now();
        let result = (self.processor)(input).await;
        let duration = start_time.elapsed();

        match &result {
            Ok(_) => self.metrics.record_operation(duration),
            Err(_) => self.metrics.record_error(),
        }

        result
    }
}

/// Multi-stage processing pipeline
#[derive(Debug)]
pub struct ProcessingPipeline<I, O> {
    stages: Vec<PipelineStage<I, O>>,
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
}

impl<I, O> ProcessingPipeline<I, O>
where
    I: Send + Clone + 'static,
    O: Send + 'static,
{
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            shutdown: ShutdownCoordinator::new(),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Add a processing stage to the pipeline
    pub fn add_stage<F>(mut self, name: impl Into<String>, processor: F) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<O, PipelineError>> + Send>> + Send + Sync + 'static,
    {
        let stage = PipelineStage::new(name, processor);
        self.stages.push(stage);
        self
    }

    /// Process a single item through all stages
    #[instrument(skip(self, input), fields(stages = %self.stages.len()))]
    pub async fn process_item(&self, input: I) -> Result<O, PipelineError> {
        let mut current_input = input;

        for (i, stage) in self.stages.iter().enumerate() {
            if self.shutdown.is_shutdown() {
                return Err(PipelineError::Shutdown);
            }

            debug!("Processing stage {}: {}", i, stage.name);

            match stage.process(current_input).await {
                Ok(output) => {
                    current_input = output;
                }
                Err(e) => {
                    error!("Pipeline stage {} failed: {}", stage.name, e);
                    return Err(e);
                }
            }
        }

        Ok(current_input)
    }

    /// Process multiple items through the pipeline
    #[instrument(skip(self, items), fields(item_count = %items.len()))]
    pub async fn process_items(&self, items: Vec<I>) -> Vec<Result<O, PipelineError>> {
        let mut results = Vec::with_capacity(items.len());

        for item in items {
            let result = self.process_item(item).await;
            results.push(result);
        }

        results
    }

    /// Get pipeline statistics
    pub fn stats(&self) -> PipelineStats {
        let mut stage_stats = HashMap::new();

        for stage in &self.stages {
            let (ops, errs, avg_time) = stage.metrics.get_stats();
            stage_stats.insert(stage.name.clone(), StageStats {
                operations: ops,
                errors: errs,
                avg_processing_time: avg_time,
            });
        }

        let (total_ops, total_errs, total_time) = self.metrics.get_stats();

        PipelineStats {
            stages: stage_stats,
            total_operations: total_ops,
            total_errors: total_errs,
            avg_total_time: total_time,
            is_shutdown: self.shutdown.is_shutdown(),
        }
    }

    /// Shutdown the pipeline
    pub async fn shutdown(&self) {
        self.shutdown.shutdown();
        info!("Pipeline shutdown initiated");
    }
}

impl<I, O> Default for ProcessingPipeline<I, O>
where
    I: Send + Clone + 'static,
    O: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Parallel processing pipeline with fan-out/fan-in
#[derive(Debug)]
pub struct ParallelPipeline<I, O> {
    fan_out_fan_in: FanOutFanIn<I, O>,
    pre_processing: Option<PipelineStage<I, I>>,
    post_processing: Option<PipelineStage<O, O>>,
}

impl<I, O> ParallelPipeline<I, O>
where
    I: Send + Clone + 'static,
    O: Send + 'static,
{
    /// Create a new parallel pipeline
    pub fn new<F>(num_workers: usize, buffer_size: usize, processor: F) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = O> + Send>> + Send + Sync + Clone + 'static,
    {
        Self {
            fan_out_fan_in: FanOutFanIn::new(num_workers, buffer_size, processor),
            pre_processing: None,
            post_processing: None,
        }
    }

    /// Add pre-processing stage
    pub fn with_pre_processing<F>(mut self, name: impl Into<String>, processor: F) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<I, PipelineError>> + Send>> + Send + Sync + 'static,
    {
        self.pre_processing = Some(PipelineStage::new(name, processor));
        self
    }

    /// Add post-processing stage
    pub fn with_post_processing<F>(mut self, name: impl Into<String>, processor: F) -> Self
    where
        F: Fn(O) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<O, PipelineError>> + Send>> + Send + Sync + 'static,
    {
        self.post_processing = Some(PipelineStage::new(name, processor));
        self
    }

    /// Process items through the parallel pipeline
    pub async fn process_items(mut self, items: Vec<I>) -> Result<Vec<ProcessingResult<I, O>>, PipelineError> {
        // Apply pre-processing if configured
        let processed_items = if let Some(pre_proc) = &self.pre_processing {
            let mut processed = Vec::new();
            for item in items {
                match pre_proc.process(item).await {
                    Ok(processed_item) => processed.push(processed_item),
                    Err(e) => return Err(e),
                }
            }
            processed
        } else {
            items
        };

        // Submit to parallel processing
        for item in processed_items {
            self.fan_out_fan_in.submit(item).await?;
        }

        // Collect parallel results
        let mut results = self.fan_out_fan_in.complete_and_collect().await;

        // Apply post-processing if configured
        if let Some(post_proc) = &self.post_processing {
            for result in &mut results {
                if let Ok(ref mut output) = result.output {
                    match post_proc.process(std::mem::take(output)).await {
                        Ok(processed_output) => {
                            *output = processed_output;
                        }
                        Err(e) => {
                            // Mark as error in result
                            result.output = Err(format!("Post-processing failed: {}", e));
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

/// Batch processing pipeline
#[derive(Debug)]
pub struct BatchPipeline<I, O> {
    batch_processor: BatchProcessor<I, O>,
    pre_batching: Option<PipelineStage<I, I>>,
}

impl<I, O> BatchPipeline<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Create a new batch pipeline
    pub fn new<F>(
        batch_size: usize,
        max_wait_time: Duration,
        processor: F,
    ) -> Self
    where
        F: Fn(Vec<I>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<O>> + Send>> + Send + Sync + 'static,
    {
        let config = crate::batching::BatchConfig::new(batch_size, max_wait_time, processor);
        let batch_processor = BatchProcessor::new(config);

        Self {
            batch_processor,
            pre_batching: None,
        }
    }

    /// Add pre-batching processing
    pub fn with_pre_processing<F>(mut self, name: impl Into<String>, processor: F) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<I, PipelineError>> + Send>> + Send + Sync + 'static,
    {
        self.pre_batching = Some(PipelineStage::new(name, processor));
        self
    }

    /// Submit item for batch processing
    pub async fn submit(&self, item: I) -> Result<(), PipelineError> {
        let processed_item = if let Some(pre_proc) = &self.pre_batching {
            pre_proc.process(item).await?
        } else {
            item
        };

        self.batch_processor.submit(processed_item).await
            .map_err(|_| PipelineError::BatchError("Submission failed".to_string()))
    }

    /// Collect batch processing results
    pub async fn collect_results(mut self) -> Vec<O> {
        self.batch_processor.collect().await
    }

    /// Get pipeline statistics
    pub fn stats(&self) -> crate::batching::BatchStats {
        self.batch_processor.stats()
    }
}

/// Pipeline builder for fluent API
#[derive(Debug)]
pub struct PipelineBuilder<I, O> {
    stages: Vec<PipelineStage<I, O>>,
}

impl<I, O> PipelineBuilder<I, O>
where
    I: Send + Clone + 'static,
    O: Send + 'static,
{
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
        }
    }

    /// Add a processing stage
    pub fn add_stage<F>(mut self, name: impl Into<String>, processor: F) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<O, PipelineError>> + Send>> + Send + Sync + 'static,
    {
        let stage = PipelineStage::new(name, processor);
        self.stages.push(stage);
        self
    }

    /// Build the processing pipeline
    pub fn build(self) -> ProcessingPipeline<I, O> {
        let mut pipeline = ProcessingPipeline::new();
        pipeline.stages = self.stages;
        pipeline
    }

    /// Build as parallel pipeline
    pub fn build_parallel<F>(self, num_workers: usize, buffer_size: usize, parallel_processor: F) -> ParallelPipeline<I, O>
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = O> + Send>> + Send + Sync + Clone + 'static,
    {
        ParallelPipeline::new(num_workers, buffer_size, parallel_processor)
    }
}

impl<I, O> Default for PipelineBuilder<I, O>
where
    I: Send + Clone + 'static,
    O: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Pipeline statistics
#[derive(Debug, Clone)]
pub struct PipelineStats {
    pub stages: HashMap<String, StageStats>,
    pub total_operations: u64,
    pub total_errors: u64,
    pub avg_total_time: Duration,
    pub is_shutdown: bool,
}

/// Stage statistics
#[derive(Debug, Clone)]
pub struct StageStats {
    pub operations: u64,
    pub errors: u64,
    pub avg_processing_time: Duration,
}

/// Pipeline error types
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("pipeline stage failed: {0}")]
    StageError(String),

    #[error("pipeline is shutting down")]
    Shutdown,

    #[error("batch processing error: {0}")]
    BatchError(String),

    #[error("parallel processing error: {0}")]
    ParallelError(String),

    #[error("result collection timeout")]
    CollectionTimeout,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    async fn validate_stage(input: i32) -> Result<i32, PipelineError> {
        if input < 0 {
            Err(PipelineError::StageError("negative input".to_string()))
        } else {
            Ok(input)
        }
    }

    async fn double_stage(input: i32) -> Result<i32, PipelineError> {
        Ok(input * 2)
    }

    async fn string_stage(input: i32) -> Result<String, PipelineError> {
        Ok(format!("result: {}", input))
    }

    #[tokio::test]
    async fn test_processing_pipeline() {
        let pipeline = PipelineBuilder::new()
            .add_stage("validate", validate_stage)
            .add_stage("double", double_stage)
            .add_stage("stringify", string_stage)
            .build();

        // Test successful processing
        let result = pipeline.process_item(5).await.unwrap();
        assert_eq!(result, "result: 10");

        // Test error handling
        let error_result = pipeline.process_item(-1).await;
        assert!(matches!(error_result, Err(PipelineError::StageError(_))));
    }

    #[tokio::test]
    async fn test_parallel_pipeline() {
        let pipeline = ParallelPipeline::new(
            2,
            10,
            |x: i32| Box::pin(async move { x * 2 }),
        ).with_pre_processing("filter", |x| Box::pin(async move {
            Ok(if x % 2 == 0 { x } else { x + 1 }) // Make even
        }));

        let inputs = vec![1, 2, 3, 4, 5];
        let results = pipeline.process_items(inputs).await.unwrap();

        assert_eq!(results.len(), 5);

        // All results should be even numbers doubled
        for result in results {
            assert_eq!(result.output % 4, 0); // Even * 2 = multiple of 4
        }
    }

    #[tokio::test]
    async fn test_batch_pipeline() {
        let mut pipeline = BatchPipeline::new(
            3,
            Duration::from_millis(100),
            |batch| Box::pin(async move {
                vec![batch.iter().sum()] // Single sum result
            }),
        );

        // Submit items
        for i in 0..7 {
            pipeline.submit(i).await.unwrap();
        }

        // Collect results
        let results = pipeline.collect_results().await;

        // Should have 3 batches: [0+1+2], [3+4+5], [6]
        assert_eq!(results, vec![3, 12, 6]);
    }

    #[tokio::test]
    async fn test_pipeline_stats() {
        let pipeline = PipelineBuilder::new()
            .add_stage("double", double_stage)
            .build();

        // Process some items
        for i in 0..3 {
            let _ = pipeline.process_item(i).await;
        }

        let stats = pipeline.stats();
        assert_eq!(stats.stages.len(), 1);
        assert!(stats.stages.contains_key("double"));

        let double_stats = &stats.stages["double"];
        assert_eq!(double_stats.operations, 3);
        assert_eq!(double_stats.errors, 0);
    }

    #[tokio::test]
    async fn test_pipeline_shutdown() {
        let pipeline = ProcessingPipeline::<i32, i32>::new();
        assert!(!pipeline.stats().is_shutdown);

        pipeline.shutdown().await;
        assert!(pipeline.stats().is_shutdown);
    }

    #[tokio::test]
    async fn test_pipeline_error_propagation() {
        let pipeline = PipelineBuilder::new()
            .add_stage("failing", |_: i32| Box::pin(async move {
                Err(PipelineError::StageError("intentional failure".to_string()))
            }))
            .add_stage("should_not_reach", double_stage)
            .build();

        let result = pipeline.process_item(5).await;
        assert!(matches!(result, Err(PipelineError::StageError(_))));

        // Second stage should not have processed anything
        let stats = pipeline.stats();
        let failing_stats = &stats.stages["failing"];
        let unreachable_stats = &stats.stages["should_not_reach"];

        assert_eq!(failing_stats.errors, 1);
        assert_eq!(unreachable_stats.operations, 0);
    }
}
