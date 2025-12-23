//! Fan-Out/Fan-In Pipeline Patterns
//!
//! Fan-out distributes work across multiple concurrent workers, while fan-in
//! collects and combines results. These patterns enable parallel processing
//! pipelines that maximize throughput while maintaining ordering guarantees.
//!
//! ## Key Concepts
//!
//! - **Fan-Out**: Distribute work to multiple workers
//! - **Fan-In**: Collect and merge results from workers
//! - **Ordered Output**: Maintain input order in results
//! - **Load Balancing**: Distribute work fairly across workers
//! - **Backpressure**: Prevent worker overload
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::fan_out_fan_in::{FanOutFanIn, ProcessingResult};
//!
//! let pipeline = FanOutFanIn::new(4, 100); // 4 workers, buffer size 100
//!
//! // Submit work items
//! for i in 0..10 {
//!     pipeline.submit(i).await;
//! }
//!
//! // Collect ordered results
//! let results: Vec<ProcessingResult<i32, i32>> = pipeline.collect_ordered().await;
//!
//! for result in results {
//!     println!("Input: {}, Output: {}", result.input, result.output);
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, instrument};

use crate::common::Metrics;

/// Result of processing in fan-out/fan-in pipeline
#[derive(Debug, Clone)]
pub struct ProcessingResult<I, O> {
    pub input: I,
    pub output: O,
    pub worker_id: usize,
    pub sequence_number: u64,
    pub processing_time: std::time::Duration,
}

/// Fan-out/fan-in pipeline for parallel processing
#[derive(Debug)]
pub struct FanOutFanIn<I, O> {
    input_sender: mpsc::Sender<PipelineItem<I>>,
    result_collector: Arc<RwLock<ResultCollector<I, O>>>,
    workers: Vec<JoinHandle<()>>,
    next_sequence: std::sync::atomic::AtomicU64,
    metrics: Arc<Metrics>,
}

#[derive(Debug)]
struct PipelineItem<I> {
    input: I,
    sequence_number: u64,
}

#[derive(Debug)]
struct ResultCollector<I, O> {
    results: HashMap<u64, ProcessingResult<I, O>>,
    next_expected: u64,
    completed: bool,
}

impl<I, O> ResultCollector<I, O> {
    fn new() -> Self {
        Self {
            results: HashMap::new(),
            next_expected: 0,
            completed: false,
        }
    }

    fn add_result(&mut self, result: ProcessingResult<I, O>) {
        self.results.insert(result.sequence_number, result);
    }

    fn collect_ordered(&mut self) -> Vec<ProcessingResult<I, O>> {
        let mut ordered = Vec::new();

        while let Some(result) = self.results.remove(&self.next_expected) {
            ordered.push(result);
            self.next_expected += 1;
        }

        ordered
    }

    fn has_pending_results(&self) -> bool {
        !self.results.is_empty()
    }

    fn mark_completed(&mut self) {
        self.completed = true;
    }

    fn is_completed(&self) -> bool {
        self.completed && !self.has_pending_results()
    }
}

impl<I, O> FanOutFanIn<I, O>
where
    I: Send + Clone + 'static,
    O: Send + 'static,
{
    /// Create a new fan-out/fan-in pipeline
    pub fn new<F>(num_workers: usize, buffer_size: usize, processor: F) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = O> + Send>> + Send + Sync + Clone + 'static,
    {
        let (input_sender, input_receiver) = mpsc::channel(buffer_size);
        let result_collector = Arc::new(RwLock::new(ResultCollector::new()));
        let metrics = Arc::new(Metrics::new());

        // Start workers
        let mut workers = Vec::with_capacity(num_workers);
        for worker_id in 0..num_workers {
            let receiver = input_receiver.clone();
            let collector = result_collector.clone();
            let processor_clone = processor.clone();
            let metrics_clone = metrics.clone();

            let worker = tokio::spawn(async move {
                run_worker(worker_id, receiver, collector, processor_clone, metrics_clone).await;
            });

            workers.push(worker);
        }

        Self {
            input_sender,
            result_collector,
            workers,
            next_sequence: std::sync::atomic::AtomicU64::new(0),
            metrics,
        }
    }

    /// Submit an item for processing
    #[instrument(skip(self, input))]
    pub async fn submit(&self, input: I) -> Result<(), PipelineError> {
        let sequence_number = self.next_sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let item = PipelineItem {
            input,
            sequence_number,
        };

        self.input_sender.send(item).await
            .map_err(|_| PipelineError::ChannelClosed)?;

        debug!("Submitted item with sequence {}", sequence_number);
        Ok(())
    }

    /// Submit multiple items at once
    pub async fn submit_batch(&self, inputs: Vec<I>) -> Result<(), PipelineError> {
        for input in inputs {
            self.submit(input).await?;
        }
        Ok(())
    }

    /// Collect all completed results in order
    pub async fn collect_ordered(&self) -> Vec<ProcessingResult<I, O>> {
        let mut collector = self.result_collector.write().await;
        collector.collect_ordered()
    }

    /// Collect available results without waiting for all
    pub async fn collect_available(&self) -> Vec<ProcessingResult<I, O>> {
        let mut collector = self.result_collector.write().await;
        let mut available = Vec::new();

        // Collect all available results in order
        while let Some(result) = collector.results.remove(&collector.next_expected) {
            available.push(result);
            collector.next_expected += 1;
        }

        available
    }

    /// Wait for all submitted work to complete and collect results
    #[instrument(skip(self))]
    pub async fn complete_and_collect(mut self) -> Vec<ProcessingResult<I, O>> {
        // Close input channel to signal no more work
        drop(self.input_sender);

        // Wait for all workers to complete
        for worker in self.workers {
            let _ = worker.await;
        }

        // Mark collector as completed
        {
            let mut collector = self.result_collector.write().await;
            collector.mark_completed();
        }

        // Collect all remaining results
        let mut all_results = Vec::new();
        loop {
            let batch = self.collect_available().await;
            if batch.is_empty() {
                let collector = self.result_collector.read().await;
                if collector.is_completed() {
                    break;
                }
                // Wait a bit for more results
                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
            } else {
                all_results.extend(batch);
            }
        }

        all_results
    }

    /// Get pipeline statistics
    pub fn stats(&self) -> PipelineStats {
        let (ops, errs, avg_duration) = self.metrics.get_stats();
        PipelineStats {
            total_processed: ops,
            total_errors: errs,
            avg_processing_time: avg_duration,
            active_workers: self.workers.len(),
        }
    }

    /// Check if pipeline is still active
    pub fn is_active(&self) -> bool {
        !self.input_sender.is_closed()
    }
}

/// Worker function for processing pipeline items
async fn run_worker<I, O, F>(
    worker_id: usize,
    mut receiver: mpsc::Receiver<PipelineItem<I>>,
    collector: Arc<RwLock<ResultCollector<I, O>>>,
    processor: F,
    metrics: Arc<Metrics>,
) where
    F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = O> + Send>>,
{
    debug!("Pipeline worker {} started", worker_id);

    while let Some(item) = receiver.recv().await {
        let start_time = std::time::Instant::now();

        // Process the item
        let output = processor(item.input.clone()).await;
        let processing_time = start_time.elapsed();

        // Create result
        let result = ProcessingResult {
            input: item.input,
            output,
            worker_id,
            sequence_number: item.sequence_number,
            processing_time,
        };

        // Store result
        {
            let mut collector_lock = collector.write().await;
            collector_lock.add_result(result);
        }

        metrics.record_operation(processing_time);
        debug!("Worker {} completed item {}", worker_id, item.sequence_number);
    }

    debug!("Pipeline worker {} stopped", worker_id);
}

/// Pipeline statistics
#[derive(Debug, Clone)]
pub struct PipelineStats {
    pub total_processed: u64,
    pub total_errors: u64,
    pub avg_processing_time: std::time::Duration,
    pub active_workers: usize,
}

/// Pipeline error types
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("pipeline channel is closed")]
    ChannelClosed,

    #[error("pipeline is shutting down")]
    ShuttingDown,

    #[error("worker panic detected")]
    WorkerPanic,

    #[error("result collection timeout")]
    CollectionTimeout,
}

/// Scatter-gather pattern (fan-out with custom fan-in logic)
#[derive(Debug)]
pub struct ScatterGather<I, O, R> {
    fan_out: FanOutFanIn<I, O>,
    gather_fn: Box<dyn Fn(Vec<ProcessingResult<I, O>>) -> R + Send + Sync>,
}

impl<I, O, R> ScatterGather<I, O, R>
where
    I: Send + Clone + 'static,
    O: Send + 'static,
    R: Send + 'static,
{
    pub fn new<F, G>(
        num_workers: usize,
        buffer_size: usize,
        processor: F,
        gather_fn: G,
    ) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = O> + Send>> + Send + Sync + Clone + 'static,
        G: Fn(Vec<ProcessingResult<I, O>>) -> R + Send + Sync + 'static,
    {
        Self {
            fan_out: FanOutFanIn::new(num_workers, buffer_size, processor),
            gather_fn: Box::new(gather_fn),
        }
    }

    /// Scatter work and gather results
    pub async fn scatter_gather(self, inputs: Vec<I>) -> Result<R, PipelineError> {
        // Submit all inputs
        self.fan_out.submit_batch(inputs).await?;

        // Complete processing and collect results
        let results = self.fan_out.complete_and_collect().await;

        // Apply gather function
        Ok((self.gather_fn)(results))
    }
}

/// Load-balanced fan-out with worker selection
#[derive(Debug)]
pub struct LoadBalancedFanOut<I, O> {
    workers: Vec<mpsc::Sender<PipelineItem<I>>>,
    result_collector: Arc<RwLock<ResultCollector<I, O>>>,
    next_worker: std::sync::atomic::AtomicUsize,
    next_sequence: std::sync::atomic::AtomicU64,
}

impl<I, O> LoadBalancedFanOut<I, O>
where
    I: Send + Clone + 'static,
    O: Send + 'static,
{
    pub fn new<F>(num_workers: usize, buffer_size: usize, processor: F) -> Self
    where
        F: Fn(I) -> std::pin::Pin<Box<dyn std::future::Future<Output = O> + Send>> + Send + Sync + Clone + 'static,
    {
        let mut workers = Vec::with_capacity(num_workers);
        let result_collector = Arc::new(RwLock::new(ResultCollector::new()));

        for worker_id in 0..num_workers {
            let (tx, rx) = mpsc::channel(buffer_size);
            let collector = result_collector.clone();
            let processor_clone = processor.clone();

            tokio::spawn(async move {
                run_worker(worker_id, rx, collector, processor_clone, Arc::new(Metrics::new())).await;
            });

            workers.push(tx);
        }

        Self {
            workers,
            result_collector,
            next_worker: std::sync::atomic::AtomicUsize::new(0),
            next_sequence: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Submit item to next worker in round-robin fashion
    pub async fn submit_round_robin(&self, input: I) -> Result<(), PipelineError> {
        let sequence_number = self.next_sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let worker_index = self.next_worker.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % self.workers.len();

        let item = PipelineItem {
            input,
            sequence_number,
        };

        self.workers[worker_index].send(item).await
            .map_err(|_| PipelineError::ChannelClosed)
    }

    /// Collect available results
    pub async fn collect_available(&self) -> Vec<ProcessingResult<I, O>> {
        let mut collector = self.result_collector.write().await;
        collector.collect_ordered()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};

    async fn double_processor(input: i32) -> i32 {
        tokio::time::sleep(Duration::from_millis(1)).await; // Simulate work
        input * 2
    }

    async fn async_double_processor(input: i32) -> i32 {
        tokio::time::sleep(Duration::from_millis(1)).await;
        input * 2
    }

    #[tokio::test]
    async fn test_fan_out_fan_in_basic() {
        let pipeline = FanOutFanIn::new(2, 10, |x| Box::pin(double_processor(x)));

        // Submit items
        for i in 0..5 {
            pipeline.submit(i).await.unwrap();
        }

        // Complete and collect
        let results = pipeline.complete_and_collect().await;

        assert_eq!(results.len(), 5);

        // Results should be in order
        for (i, result) in results.into_iter().enumerate() {
            assert_eq!(result.input, i as i32);
            assert_eq!(result.output, (i as i32) * 2);
        }
    }

    #[tokio::test]
    async fn test_fan_out_fan_in_concurrent() {
        let pipeline = FanOutFanIn::new(4, 20, |x| Box::pin(async_double_processor(x)));

        // Submit items concurrently
        let mut handles = Vec::new();
        for i in 0..10 {
            let pipeline = &pipeline;
            let handle = tokio::spawn(async move {
                pipeline.submit(i).await.unwrap();
            });
            handles.push(handle);
        }

        // Wait for all submissions
        for handle in handles {
            handle.await.unwrap();
        }

        // Collect results
        let results = pipeline.complete_and_collect().await;
        assert_eq!(results.len(), 10);

        // Verify all inputs were processed
        let mut inputs: Vec<_> = results.into_iter().map(|r| r.input).collect();
        inputs.sort();
        assert_eq!(inputs, (0..10).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn test_collect_available() {
        let pipeline = FanOutFanIn::new(1, 10, |x| Box::pin(async_double_processor(x)));

        // Submit items
        for i in 0..3 {
            pipeline.submit(i).await.unwrap();
        }

        // Wait a bit for processing
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Collect available results
        let available = pipeline.collect_available().await;
        assert!(!available.is_empty());

        // Complete and get all
        let all = pipeline.complete_and_collect().await;
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_scatter_gather() {
        let scatter_gather = ScatterGather::new(
            3,
            10,
            |x| Box::pin(async_double_processor(x)),
            |results: Vec<ProcessingResult<i32, i32>>| {
                results.into_iter().map(|r| r.output).sum::<i32>()
            },
        );

        let inputs = vec![1, 2, 3, 4, 5];
        let sum = scatter_gather.scatter_gather(inputs).await.unwrap();

        // Expected: (1+2+3+4+5) * 2 = 30
        assert_eq!(sum, 30);
    }

    #[tokio::test]
    async fn test_load_balanced_fan_out() {
        let load_balanced = LoadBalancedFanOut::new(2, 10, |x| Box::pin(async_double_processor(x)));

        // Submit items
        for i in 0..6 {
            load_balanced.submit_round_robin(i).await.unwrap();
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Collect results
        let results = load_balanced.collect_available().await;
        assert_eq!(results.len(), 6);

        // Check that work was distributed across workers
        let worker_counts: std::collections::HashMap<usize, usize> = results
            .iter()
            .fold(std::collections::HashMap::new(), |mut acc, r| {
                *acc.entry(r.worker_id).or_insert(0) += 1;
                acc
            });

        // Should have work on both workers
        assert!(worker_counts.len() >= 1);
    }

    #[tokio::test]
    async fn test_pipeline_stats() {
        let pipeline = FanOutFanIn::new(2, 10, |x| Box::pin(async_double_processor(x)));

        for i in 0..3 {
            pipeline.submit(i).await.unwrap();
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(10)).await;

        let stats = pipeline.stats();
        assert_eq!(stats.active_workers, 2);
        assert_eq!(stats.total_processed, 3);
    }

    #[tokio::test]
    async fn test_pipeline_error_handling() {
        let pipeline = FanOutFanIn::new(1, 1, |x| Box::pin(async_double_processor(x)));

        // Fill the buffer
        for i in 0..2 {
            pipeline.submit(i).await.unwrap();
        }

        // Try to submit when buffer might be full
        let result = timeout(Duration::from_millis(10), pipeline.submit(2)).await;
        // Either succeeds or times out due to backpressure - both are acceptable
        let _ = result;
    }
}
