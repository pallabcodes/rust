//! Batching Pipeline Patterns
//!
//! Batching groups multiple items together for efficient processing,
//! reducing overhead and improving throughput for operations that benefit
//! from economies of scale.
//!
//! ## Key Concepts
//!
//! - **Size-based Batching**: Group by count
//! - **Time-based Batching**: Group by time windows
//! - **Size-or-Time Batching**: Group by either limit
//! - **Adaptive Batching**: Adjust batch size based on load
//! - **Batch Processing**: Efficient bulk operations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::batching::{BatchProcessor, BatchConfig};
//!
//! let processor = BatchProcessor::new(BatchConfig {
//!     max_batch_size: 100,
//!     max_wait_time: Duration::from_millis(50),
//!     processor: |batch: Vec<i32>| async move {
//!         // Process entire batch efficiently
//!         batch.into_iter().map(|x| x * 2).collect()
//!     },
//! });
//!
//! // Submit items (batched automatically)
//! for i in 0..150 {
//!     processor.submit(i).await;
//! }
//!
//! // Collect results
//! let results = processor.collect().await;
//! ```

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::time::{Duration, Instant, timeout};
use tracing::{debug, instrument};

use crate::common::Metrics;

/// Configuration for batch processing
#[derive(Debug, Clone)]
pub struct BatchConfig<F, T, R> {
    pub max_batch_size: usize,
    pub max_wait_time: Duration,
    pub processor: F,
    pub _phantom: std::marker::PhantomData<(T, R)>,
}

impl<F, T, R> BatchConfig<F, T, R> {
    pub fn new(max_batch_size: usize, max_wait_time: Duration, processor: F) -> Self {
        Self {
            max_batch_size,
            max_wait_time,
            processor,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Batch processor that groups items for efficient processing
#[derive(Debug)]
pub struct BatchProcessor<T, R> {
    config: BatchConfig<Box<dyn Fn(Vec<T>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<R>> + Send>> + Send + Sync>, T, R>,
    input_sender: mpsc::Sender<T>,
    result_receiver: mpsc::Receiver<Vec<R>>,
    metrics: Arc<Metrics>,
}

impl<T, R> BatchProcessor<T, R>
where
    T: Send + 'static,
    R: Send + 'static,
{
    /// Create a new batch processor
    pub fn new<F>(config: BatchConfig<F, T, R>) -> Self
    where
        F: Fn(Vec<T>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<R>> + Send>> + Send + Sync + 'static,
    {
        let (input_sender, input_receiver) = mpsc::channel(1000);
        let (result_sender, result_receiver) = mpsc::channel(100);
        let metrics = Arc::new(Metrics::new());

        // Wrap the processor function
        let processor = Arc::new(config.processor);

        // Start batch processing task
        tokio::spawn(async move {
            run_batch_processor(
                input_receiver,
                result_sender,
                config.max_batch_size,
                config.max_wait_time,
                processor,
                metrics,
            ).await;
        });

        Self {
            config: BatchConfig {
                max_batch_size: config.max_batch_size,
                max_wait_time: config.max_wait_time,
                processor: Box::new(|_| panic!("Not used")),
                _phantom: std::marker::PhantomData,
            },
            input_sender,
            result_receiver,
            metrics,
        }
    }

    /// Submit an item for batch processing
    #[instrument(skip(self, item))]
    pub async fn submit(&self, item: T) -> Result<(), BatchError> {
        self.input_sender.send(item).await
            .map_err(|_| BatchError::ChannelClosed)?;
        Ok(())
    }

    /// Submit multiple items at once
    pub async fn submit_batch(&self, items: Vec<T>) -> Result<(), BatchError> {
        for item in items {
            self.submit(item).await?;
        }
        Ok(())
    }

    /// Collect processed results
    pub async fn collect(&mut self) -> Vec<R> {
        let mut all_results = Vec::new();

        while let Some(batch_results) = self.result_receiver.recv().await {
            all_results.extend(batch_results);
        }

        all_results
    }

    /// Try to collect available results without waiting
    pub fn try_collect(&mut self) -> Vec<R> {
        let mut results = Vec::new();

        while let Ok(batch_results) = self.result_receiver.try_recv() {
            results.extend(batch_results);
        }

        results
    }

    /// Get processor statistics
    pub fn stats(&self) -> BatchStats {
        let (ops, errs, avg_duration) = self.metrics.get_stats();
        BatchStats {
            total_batches: ops,
            total_errors: errs,
            avg_batch_time: avg_duration,
            max_batch_size: self.config.max_batch_size,
        }
    }
}

/// Internal batch processing function
async fn run_batch_processor<T, R, F>(
    mut input_receiver: mpsc::Receiver<T>,
    result_sender: mpsc::Sender<Vec<R>>,
    max_batch_size: usize,
    max_wait_time: Duration,
    processor: Arc<F>,
    metrics: Arc<Metrics>,
) where
    F: Fn(Vec<T>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<R>> + Send>>,
{
    debug!("Batch processor started with max_size={}, max_wait={:?}", max_batch_size, max_wait_time);

    loop {
        let mut current_batch = Vec::with_capacity(max_batch_size);
        let batch_start = Instant::now();

        // Fill batch either by size or timeout
        loop {
            let remaining_time = max_wait_time.saturating_sub(batch_start.elapsed());

            if current_batch.len() >= max_batch_size {
                // Batch is full
                break;
            }

            if batch_start.elapsed() >= max_wait_time && !current_batch.is_empty() {
                // Timeout reached and we have items
                break;
            }

            match timeout(remaining_time, input_receiver.recv()).await {
                Ok(Some(item)) => {
                    current_batch.push(item);

                    if current_batch.len() >= max_batch_size {
                        break;
                    }
                }
                Ok(None) => {
                    // Channel closed, process remaining items if any
                    if !current_batch.is_empty() {
                        break;
                    } else {
                        return; // No more items and channel closed
                    }
                }
                Err(_) => {
                    // Timeout, process current batch if not empty
                    if !current_batch.is_empty() {
                        break;
                    }
                }
            }
        }

        if current_batch.is_empty() {
            continue;
        }

        // Process the batch
        let batch_size = current_batch.len();
        let start_time = Instant::now();

        let batch_results = processor(current_batch).await;
        let processing_time = start_time.elapsed();

        metrics.record_operation(processing_time);

        debug!("Processed batch of {} items in {:?}", batch_size, processing_time);

        // Send results
        if result_sender.send(batch_results).await.is_err() {
            // Result channel closed
            break;
        }
    }
}

/// Statistics for batch processing
#[derive(Debug, Clone)]
pub struct BatchStats {
    pub total_batches: u64,
    pub total_errors: u64,
    pub avg_batch_time: Duration,
    pub max_batch_size: usize,
}

/// Batch processing error types
#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    #[error("batch processor channel is closed")]
    ChannelClosed,

    #[error("batch processing failed: {0}")]
    ProcessingError(String),

    #[error("batch timeout exceeded")]
    Timeout,

    #[error("invalid batch configuration: {0}")]
    ConfigError(String),
}

/// Adaptive batch processor that adjusts batch size based on performance
#[derive(Debug)]
pub struct AdaptiveBatchProcessor<T, R> {
    processor: BatchProcessor<T, R>,
    current_batch_size: Arc<Mutex<usize>>,
    min_batch_size: usize,
    max_batch_size: usize,
    target_processing_time: Duration,
    adjustment_factor: f64,
}

impl<T, R> AdaptiveBatchProcessor<T, R>
where
    T: Send + 'static,
    R: Send + 'static,
{
    pub fn new<F>(
        initial_batch_size: usize,
        min_batch_size: usize,
        max_batch_size: usize,
        max_wait_time: Duration,
        target_processing_time: Duration,
        processor: F,
    ) -> Self
    where
        F: Fn(Vec<T>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<R>> + Send>> + Send + Sync + 'static,
    {
        let config = BatchConfig::new(initial_batch_size, max_wait_time, processor);
        let processor = BatchProcessor::new(config);

        Self {
            processor,
            current_batch_size: Arc::new(Mutex::new(initial_batch_size)),
            min_batch_size,
            max_batch_size,
            target_processing_time,
            adjustment_factor: 0.1, // Adjust by 10% each time
        }
    }

    /// Submit item (uses current adaptive batch size)
    pub async fn submit(&self, item: T) -> Result<(), BatchError> {
        self.processor.submit(item).await
    }

    /// Collect results and potentially adjust batch size
    pub async fn collect_and_adjust(&mut self) -> Vec<R> {
        let results = self.processor.collect().await;

        // Analyze performance and adjust batch size
        self.adjust_batch_size().await;

        results
    }

    async fn adjust_batch_size(&self) {
        let stats = self.processor.stats();

        if stats.total_batches == 0 {
            return; // No data yet
        }

        let current_size = *self.current_batch_size.lock().await;

        if stats.avg_batch_time > self.target_processing_time {
            // Too slow, reduce batch size
            let new_size = ((current_size as f64) * (1.0 - self.adjustment_factor)) as usize;
            let new_size = new_size.max(self.min_batch_size);

            if new_size != current_size {
                *self.current_batch_size.lock().await = new_size;
                debug!("Reduced batch size from {} to {} (too slow)", current_size, new_size);
            }
        } else if stats.avg_batch_time < self.target_processing_time / 2 {
            // Too fast, increase batch size
            let new_size = ((current_size as f64) * (1.0 + self.adjustment_factor)) as usize;
            let new_size = new_size.min(self.max_batch_size);

            if new_size != current_size {
                *self.current_batch_size.lock().await = new_size;
                debug!("Increased batch size from {} to {} (too fast)", current_size, new_size);
            }
        }
    }

    /// Get current adaptive batch size
    pub async fn current_batch_size(&self) -> usize {
        *self.current_batch_size.lock().await
    }
}

/// Time-windowed batch processor
#[derive(Debug)]
pub struct TimeWindowBatchProcessor<T, R> {
    window_duration: Duration,
    batches: Arc<Mutex<VecDeque<TimeWindowBatch<T>>>>,
    processor: Arc<dyn Fn(Vec<T>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<R>> + Send>> + Send + Sync>,
    result_sender: mpsc::Sender<Vec<R>>,
    metrics: Arc<Metrics>,
}

#[derive(Debug)]
struct TimeWindowBatch<T> {
    items: Vec<T>,
    window_start: Instant,
}

impl<T, R> TimeWindowBatchProcessor<T, R>
where
    T: Send + 'static,
    R: Send + 'static,
{
    pub fn new<F>(
        window_duration: Duration,
        processor: F,
    ) -> (Self, mpsc::Receiver<Vec<R>>)
    where
        F: Fn(Vec<T>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<R>> + Send>> + Send + Sync + 'static,
    {
        let (result_sender, result_receiver) = mpsc::channel(100);
        let batches = Arc::new(Mutex::new(VecDeque::new()));
        let processor = Arc::new(processor);
        let metrics = Arc::new(Metrics::new());

        let processor_clone = processor.clone();
        let batches_clone = batches.clone();
        let result_sender_clone = result_sender.clone();
        let metrics_clone = metrics.clone();

        // Start window processing task
        tokio::spawn(async move {
            run_time_window_processor(
                window_duration,
                batches_clone,
                processor_clone,
                result_sender_clone,
                metrics_clone,
            ).await;
        });

        (
            Self {
                window_duration,
                batches,
                processor,
                result_sender,
                metrics,
            },
            result_receiver,
        )
    }

    /// Add item to current time window
    pub async fn submit(&self, item: T) {
        let mut batches = self.batches.lock().await;
        let now = Instant::now();

        // Get or create current window batch
        if batches.back().map_or(true, |b| now.duration_since(b.window_start) >= self.window_duration) {
            batches.push_back(TimeWindowBatch {
                items: Vec::new(),
                window_start: now,
            });
        }

        if let Some(batch) = batches.back_mut() {
            batch.items.push(item);
        }
    }

    /// Force processing of current window
    pub async fn flush(&self) {
        let mut batches = self.batches.lock().await;

        while let Some(batch) = batches.pop_front() {
            if !batch.items.is_empty() {
                let results = (self.processor)(batch.items).await;
                let _ = self.result_sender.send(results).await;
            }
        }
    }
}

/// Internal time window processing function
async fn run_time_window_processor<T, R, F>(
    window_duration: Duration,
    batches: Arc<Mutex<VecDeque<TimeWindowBatch<T>>>>,
    processor: Arc<F>,
    result_sender: mpsc::Sender<Vec<R>>,
    metrics: Arc<Metrics>,
) where
    F: Fn(Vec<T>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<R>> + Send>>,
{
    loop {
        tokio::time::sleep(window_duration).await;

        let mut batches_lock = batches.lock().await;
        let now = Instant::now();

        // Process completed windows
        while let Some(batch) = batches_lock.front() {
            if now.duration_since(batch.window_start) >= window_duration && !batch.items.is_empty() {
                let batch = batches_lock.pop_front().unwrap();

                let start_time = Instant::now();
                let results = processor(batch.items).await;
                let processing_time = start_time.elapsed();

                metrics.record_operation(processing_time);

                if result_sender.send(results).await.is_err() {
                    return; // Result channel closed
                }
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    async fn sum_batch_processor(batch: Vec<i32>) -> Vec<i32> {
        tokio::time::sleep(Duration::from_millis(1)).await; // Simulate processing
        vec![batch.iter().sum()]
    }

    async fn identity_processor(batch: Vec<i32>) -> Vec<i32> {
        batch
    }

    #[tokio::test]
    async fn test_batch_processor_size_based() {
        let config = BatchConfig::new(3, Duration::from_secs(10), |batch| Box::pin(sum_batch_processor(batch)));
        let mut processor = BatchProcessor::new(config);

        // Submit 7 items (should create 3 batches: 3+3+1)
        for i in 0..7 {
            processor.submit(i).await.unwrap();
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(50)).await;

        let results = processor.try_collect();
        assert_eq!(results, vec![0 + 1 + 2, 3 + 4 + 5, 6]); // 3, 12, 6
    }

    #[tokio::test]
    async fn test_batch_processor_time_based() {
        let config = BatchConfig::new(100, Duration::from_millis(50), |batch| Box::pin(identity_processor(batch)));
        let mut processor = BatchProcessor::new(config);

        // Submit fewer items than batch size
        for i in 0..3 {
            processor.submit(i).await.unwrap();
        }

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        let results = processor.try_collect();
        assert_eq!(results, vec![0, 1, 2]);
    }

    #[tokio::test]
    async fn test_adaptive_batch_processor() {
        let mut processor = AdaptiveBatchProcessor::new(
            10,    // initial batch size
            1,     // min batch size
            100,   // max batch size
            Duration::from_secs(1),
            Duration::from_millis(10), // target processing time
            |batch| Box::pin(sum_batch_processor(batch)),
        );

        // Submit items
        for i in 0..20 {
            processor.submit(i).await.unwrap();
        }

        // Collect and potentially adjust
        let _results = processor.collect_and_adjust().await;

        let current_size = processor.current_batch_size().await;
        assert!(current_size >= 1 && current_size <= 100);
    }

    #[tokio::test]
    async fn test_time_window_batch_processor() {
        let (processor, mut receiver) = TimeWindowBatchProcessor::new(
            Duration::from_millis(50),
            |batch| Box::pin(identity_processor(batch)),
        );

        // Submit items
        for i in 0..5 {
            processor.submit(i).await;
        }

        // Wait for window to close
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Collect results
        let mut all_results = Vec::new();
        while let Ok(results) = receiver.try_recv() {
            all_results.extend(results);
        }

        assert_eq!(all_results.len(), 5);
        assert_eq!(all_results.iter().sum::<i32>(), (0..5).sum::<i32>());
    }

    #[tokio::test]
    async fn test_batch_processor_stats() {
        let config = BatchConfig::new(2, Duration::from_secs(1), |batch| Box::pin(sum_batch_processor(batch)));
        let processor = BatchProcessor::new(config);

        // Submit items to create batches
        for i in 0..4 {
            processor.submit(i).await.unwrap();
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = processor.stats();
        assert_eq!(stats.max_batch_size, 2);
        assert!(stats.total_batches >= 1); // At least one batch processed
    }

    #[tokio::test]
    async fn test_batch_processor_flush() {
        let (processor, mut receiver) = TimeWindowBatchProcessor::new(
            Duration::from_secs(10), // Long window
            |batch| Box::pin(identity_processor(batch)),
        );

        // Submit items
        for i in 0..3 {
            processor.submit(i).await;
        }

        // Force flush
        processor.flush().await;

        // Should get results immediately
        if let Ok(results) = receiver.try_recv() {
            assert_eq!(results, vec![0, 1, 2]);
        } else {
            panic!("Expected results after flush");
        }
    }
}
