//! Async Streams and Reactive Programming
//!
//! Async streams provide a way to handle sequences of asynchronous data,
//! enabling reactive programming patterns for processing continuous data flows.
//! This is essential for real-time systems, data pipelines, and event processing.
//!
//! ## Key Concepts
//!
//! - **AsyncIterator**: Asynchronous iteration over data sequences
//! - **Stream Composition**: Chaining and combining multiple streams
//! - **Backpressure**: Flow control for preventing resource exhaustion
//! - **Reactive Operators**: Map, filter, fold operations on streams
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::async_streams::{StreamExt, stream};
//! use tokio_stream::{StreamExt as TokioStreamExt, wrappers::ReceiverStream};
//!
//! // Create a stream from a channel
//! let (tx, rx) = tokio::sync::mpsc::channel(100);
//! let stream = ReceiverStream::new(rx);
//!
//! // Process the stream reactively
//! let processed = stream
//!     .map(|x| x * 2)
//!     .filter(|x| x > 10)
//!     .take(5);
//!
//! // Collect results
//! let results: Vec<_> = processed.collect().await;
//! ```

use std::pin::Pin;
use std::task::{Context, Poll};
use futures::{Stream, StreamExt};
use tokio_stream::{wrappers::ReceiverStream, StreamMap};
use tracing::{debug, instrument};

/// Extended stream utilities for reactive programming
pub trait StreamExt: Stream {
    /// Throttle stream to emit at most one item per interval
    fn throttle(self, interval: std::time::Duration) -> Throttle<Self>
    where
        Self: Sized,
    {
        Throttle::new(self, interval)
    }

    /// Batch items into vectors of specified size
    fn batch(self, size: usize) -> Batch<Self>
    where
        Self: Sized,
    {
        Batch::new(self, size)
    }

    /// Buffer items with configurable capacity
    fn buffer(self, capacity: usize) -> Buffer<Self>
    where
        Self: Sized,
    {
        Buffer::new(self, capacity)
    }

    /// Apply a sliding window of specified size
    fn window(self, size: usize) -> Window<Self>
    where
        Self: Sized,
    {
        Window::new(self, size)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}

/// Throttle stream to limit emission rate
#[derive(Debug)]
pub struct Throttle<S> {
    stream: S,
    interval: std::time::Duration,
    last_emit: Option<std::time::Instant>,
}

impl<S> Throttle<S> {
    pub fn new(stream: S, interval: std::time::Duration) -> Self {
        Self {
            stream,
            interval,
            last_emit: None,
        }
    }
}

impl<S> Stream for Throttle<S>
where
    S: Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let now = std::time::Instant::now();

        // Check if we need to wait
        if let Some(last) = self.last_emit {
            if now.duration_since(last) < self.interval {
                // Schedule wakeup after remaining interval
                let remaining = self.interval - now.duration_since(last);
                let waker = cx.waker().clone();
                tokio::spawn(async move {
                    tokio::time::sleep(remaining).await;
                    waker.wake();
                });
                return Poll::Pending;
            }
        }

        // Poll the underlying stream
        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                self.last_emit = Some(now);
                Poll::Ready(Some(item))
            }
            other => other,
        }
    }
}

/// Batch stream items into vectors
#[derive(Debug)]
pub struct Batch<S> {
    stream: S,
    batch_size: usize,
    current_batch: Vec<S::Item>,
}

impl<S> Batch<S> {
    pub fn new(stream: S, batch_size: usize) -> Self {
        Self {
            stream,
            batch_size,
            current_batch: Vec::with_capacity(batch_size),
        }
    }
}

impl<S> Stream for Batch<S>
where
    S: Stream + Unpin,
    S::Item: Clone,
{
    type Item = Vec<S::Item>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.stream).poll_next(cx) {
                Poll::Ready(Some(item)) => {
                    self.current_batch.push(item);
                    if self.current_batch.len() >= self.batch_size {
                        let batch = std::mem::replace(&mut self.current_batch, Vec::with_capacity(self.batch_size));
                        return Poll::Ready(Some(batch));
                    }
                    // Continue polling for more items
                }
                Poll::Ready(None) => {
                    // Stream ended, emit remaining batch if not empty
                    if !self.current_batch.is_empty() {
                        let batch = std::mem::take(&mut self.current_batch);
                        return Poll::Ready(Some(batch));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Buffer stream with bounded capacity
#[derive(Debug)]
pub struct Buffer<S> {
    stream: S,
    buffer: std::collections::VecDeque<S::Item>,
    capacity: usize,
}

impl<S> Buffer<S> {
    pub fn new(stream: S, capacity: usize) -> Self {
        Self {
            stream,
            buffer: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
    }
}

impl<S> Stream for Buffer<S>
where
    S: Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // First try to drain from buffer
        if let Some(item) = self.buffer.pop_front() {
            return Poll::Ready(Some(item));
        }

        // Buffer is empty, poll upstream
        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                // Fill buffer if possible
                while self.buffer.len() < self.capacity {
                    match Pin::new(&mut self.stream).poll_next(cx) {
                        Poll::Ready(Some(additional)) => {
                            self.buffer.push_back(additional);
                        }
                        Poll::Ready(None) => break,
                        Poll::Pending => break,
                    }
                }
                Poll::Ready(Some(item))
            }
            other => other,
        }
    }
}

/// Sliding window over stream items
#[derive(Debug)]
pub struct Window<S> {
    stream: S,
    window: std::collections::VecDeque<S::Item>,
    size: usize,
}

impl<S> Window<S> {
    pub fn new(stream: S, size: usize) -> Self {
        Self {
            stream,
            window: std::collections::VecDeque::with_capacity(size),
            size,
        }
    }
}

impl<S> Stream for Window<S>
where
    S: Stream + Unpin,
    S::Item: Clone,
{
    type Item = Vec<S::Item>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.stream).poll_next(cx) {
                Poll::Ready(Some(item)) => {
                    self.window.push_back(item);
                    if self.window.len() > self.size {
                        self.window.pop_front();
                    }
                    if self.window.len() == self.size {
                        let window: Vec<_> = self.window.iter().cloned().collect();
                        return Poll::Ready(Some(window));
                    }
                }
                Poll::Ready(None) => {
                    // Stream ended
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Merge multiple streams into one
pub fn merge_streams<S, I>(streams: I) -> tokio_stream::StreamMap<String, S>
where
    S: Stream + Send + 'static,
    S::Item: Send + 'static,
    I: IntoIterator<Item = (String, S)>,
{
    let mut stream_map = tokio_stream::StreamMap::new();
    for (key, stream) in streams {
        stream_map.insert(key, stream);
    }
    stream_map
}

/// Stream processing pipeline builder
#[derive(Debug)]
pub struct Pipeline<S> {
    stream: S,
}

impl<S> Pipeline<S>
where
    S: Stream + Unpin,
{
    pub fn new(stream: S) -> Self {
        Self { stream }
    }

    pub fn throttle(self, interval: std::time::Duration) -> Pipeline<Throttle<S>> {
        Pipeline::new(self.stream.throttle(interval))
    }

    pub fn batch(self, size: usize) -> Pipeline<Batch<S>>
    where
        S::Item: Clone,
    {
        Pipeline::new(self.stream.batch(size))
    }

    pub fn buffer(self, capacity: usize) -> Pipeline<Buffer<S>> {
        Pipeline::new(self.stream.buffer(capacity))
    }

    pub fn window(self, size: usize) -> Pipeline<Window<S>>
    where
        S::Item: Clone,
    {
        Pipeline::new(self.stream.window(size))
    }

    pub fn map<U, F>(self, f: F) -> Pipeline<tokio_stream::Iter<std::vec::IntoIter<U>>>
    where
        F: FnMut(S::Item) -> U,
        U: Send + 'static,
    {
        let mapped: Vec<U> = tokio::task::block_in_place(|| {
            futures::executor::block_on(async {
                self.stream.map(f).collect().await
            })
        });
        Pipeline::new(tokio_stream::iter(mapped))
    }

    pub fn filter<F>(self, f: F) -> Pipeline<tokio_stream::Iter<std::vec::IntoIter<S::Item>>>
    where
        F: FnMut(&S::Item) -> bool,
    {
        let filtered: Vec<S::Item> = tokio::task::block_in_place(|| {
            futures::executor::block_on(async {
                self.stream.filter(f).collect().await
            })
        });
        Pipeline::new(tokio_stream::iter(filtered))
    }

    pub async fn collect<B>(self) -> B
    where
        B: Default + Extend<S::Item>,
    {
        self.stream.collect().await
    }

    pub fn into_stream(self) -> S {
        self.stream
    }
}

/// Stream utilities for common patterns
pub mod utils {
    use super::*;

    /// Create an interval stream that emits at regular intervals
    pub fn interval_stream(interval: std::time::Duration) -> impl Stream<Item = std::time::Instant> {
        tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(interval))
            .map(|_| std::time::Instant::now())
    }

    /// Create a stream that emits items from an iterator
    pub fn iter_stream<I>(iter: I) -> impl Stream<Item = I::Item>
    where
        I: IntoIterator,
        I::Item: Send + 'static,
    {
        tokio_stream::iter(iter)
    }

    /// Create a stream that never emits (for testing)
    pub fn empty_stream<T>() -> impl Stream<Item = T> {
        tokio_stream::empty()
    }

    /// Create a stream that emits a single item
    pub fn once_stream<T>(item: T) -> impl Stream<Item = T>
    where
        T: Send + 'static,
    {
        tokio_stream::once(Ok(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, timeout};
    use tokio_stream::{StreamExt as TokioStreamExt, wrappers::ReceiverStream};

    #[tokio::test]
    async fn test_throttle_stream() {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let stream = ReceiverStream::new(rx);

        // Send items quickly
        for i in 0..3 {
            tx.send(i).await.unwrap();
        }
        drop(tx);

        let throttled = stream.throttle(Duration::from_millis(50));
        let results: Vec<_> = timeout(Duration::from_secs(1), throttled.collect()).await.unwrap();

        assert_eq!(results.len(), 3);
        // Items should be emitted with at least 50ms intervals
    }

    #[tokio::test]
    async fn test_batch_stream() {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let stream = ReceiverStream::new(rx);

        // Send 7 items
        for i in 0..7 {
            tx.send(i).await.unwrap();
        }
        drop(tx);

        let batched = stream.batch(3);
        let batches: Vec<Vec<i32>> = batched.collect().await;

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0], vec![0, 1, 2]);
        assert_eq!(batches[1], vec![3, 4, 5]);
        assert_eq!(batches[2], vec![6]); // Partial batch
    }

    #[tokio::test]
    async fn test_buffer_stream() {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let stream = ReceiverStream::new(rx);

        // Send items
        for i in 0..5 {
            tx.send(i).await.unwrap();
        }
        drop(tx);

        let buffered = stream.buffer(3);
        let results: Vec<i32> = buffered.collect().await;

        assert_eq!(results, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_window_stream() {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let stream = ReceiverStream::new(rx);

        // Send items
        for i in 0..5 {
            tx.send(i).await.unwrap();
        }
        drop(tx);

        let windowed = stream.window(3);
        let windows: Vec<Vec<i32>> = windowed.collect().await;

        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0], vec![0, 1, 2]);
        assert_eq!(windows[1], vec![1, 2, 3]);
        assert_eq!(windows[2], vec![2, 3, 4]);
    }

    #[tokio::test]
    async fn test_pipeline_composition() {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let stream = ReceiverStream::new(rx);

        // Send items
        for i in 0..6 {
            tx.send(i).await.unwrap();
        }
        drop(tx);

        let pipeline = Pipeline::new(stream)
            .batch(2)
            .map(|batch: Vec<i32>| batch.into_iter().sum::<i32>());

        let results: Vec<i32> = pipeline.collect().await;

        assert_eq!(results, vec![1, 5, 9]); // [0+1, 2+3, 4+5]
    }

    #[tokio::test]
    async fn test_interval_stream() {
        let stream = utils::interval_stream(Duration::from_millis(10));
        let items: Vec<_> = stream.take(3).collect().await;

        assert_eq!(items.len(), 3);
        // Verify timing (items should be ~10ms apart)
    }
}
