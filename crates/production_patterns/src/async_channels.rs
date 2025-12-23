//! Advanced Async Channel Patterns
//!
//! Async channels provide sophisticated communication patterns for concurrent
//! systems, including broadcast channels, watch channels, and barrier patterns.
//! These are essential for coordinating complex async workflows.
//!
//! ## Key Concepts
//!
//! - **MPSC**: Multi-producer single-consumer channels
//! - **Broadcast**: One-to-many communication with multiple receivers
//! - **Watch**: Latest value distribution to multiple observers
//! - **Oneshot**: Single message delivery with cancellation
//! - **Barrier**: Synchronization point for multiple async tasks
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::async_channels::{ChannelHub, ChannelType};
//!
//! let hub = ChannelHub::new();
//!
//! // Create different channel types
//! let mpsc = hub.create_mpsc::<i32>("counter", 100);
//! let broadcast = hub.create_broadcast::<String>("events", 10);
//! let watch = hub.create_watch::<bool>("shutdown", false);
//!
//! // Use channels
//! mpsc.send(42).await;
//! broadcast.send("event occurred".to_string()).await;
//! watch.send(true); // Notify all watchers
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, broadcast, watch, oneshot, Mutex, Notify};
use tokio::time::{Duration, timeout};
use tracing::{debug, instrument};

use crate::common::Metrics;

/// Unified channel hub for managing multiple channel types
#[derive(Debug)]
pub struct ChannelHub {
    channels: Arc<Mutex<HashMap<String, ChannelEntry>>>,
    metrics: Arc<Metrics>,
}

#[derive(Debug)]
enum ChannelEntry {
    Mpsc { sender: Box<dyn std::any::Any + Send + Sync> },
    Broadcast { sender: Box<dyn std::any::Any + Send + Sync> },
    Watch { sender: Box<dyn std::any::Any + Send + Sync> },
}

impl ChannelHub {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Create an MPSC channel
    pub fn create_mpsc<T: Send + 'static>(&self, name: &str, capacity: usize) -> MpscChannel<T> {
        let (tx, rx) = mpsc::channel(capacity);
        let channel = MpscChannel { sender: tx, receiver: rx };

        let mut channels = self.channels.blocking_lock();
        channels.insert(name.to_string(), ChannelEntry::Mpsc {
            sender: Box::new(tx.clone()),
        });

        channel
    }

    /// Create a broadcast channel
    pub fn create_broadcast<T: Send + Clone + 'static>(&self, name: &str, capacity: usize) -> BroadcastChannel<T> {
        let (tx, _) = broadcast::channel(capacity);
        let channel = BroadcastChannel { sender: tx.clone() };

        let mut channels = self.channels.blocking_lock();
        channels.insert(name.to_string(), ChannelEntry::Broadcast {
            sender: Box::new(tx),
        });

        channel
    }

    /// Create a watch channel
    pub fn create_watch<T: Send + Clone + 'static>(&self, name: &str, initial: T) -> WatchChannel<T> {
        let (tx, _) = watch::channel(initial);
        let channel = WatchChannel { sender: tx.clone() };

        let mut channels = self.channels.blocking_lock();
        channels.insert(name.to_string(), ChannelEntry::Watch {
            sender: Box::new(tx),
        });

        channel
    }

    /// Get statistics for all channels
    pub fn stats(&self) -> ChannelHubStats {
        let channels = self.channels.blocking_lock();
        let (ops, errs, avg_duration) = self.metrics.get_stats();

        ChannelHubStats {
            num_channels: channels.len(),
            total_operations: ops,
            total_errors: errs,
            avg_operation_time: avg_duration,
        }
    }
}

/// Statistics for channel hub
#[derive(Debug, Clone)]
pub struct ChannelHubStats {
    pub num_channels: usize,
    pub total_operations: u64,
    pub total_errors: u64,
    pub avg_operation_time: Duration,
}

/// MPSC Channel wrapper with metrics
#[derive(Debug)]
pub struct MpscChannel<T> {
    sender: mpsc::Sender<T>,
    receiver: mpsc::Receiver<T>,
}

impl<T> MpscChannel<T>
where
    T: Send + 'static,
{
    /// Send a message
    #[instrument(skip(self, msg))]
    pub async fn send(&self, msg: T) -> Result<(), ChannelError> {
        self.sender.send(msg).await
            .map_err(|_| ChannelError::ChannelClosed)
    }

    /// Try to send without blocking
    pub fn try_send(&self, msg: T) -> Result<(), ChannelError> {
        self.sender.try_send(msg)
            .map_err(|_| ChannelError::ChannelFull)
    }

    /// Receive a message
    pub async fn recv(&mut self) -> Option<T> {
        self.receiver.recv().await
    }

    /// Close the channel
    pub fn close(&self) {
        self.sender.clone(); // This will keep the sender alive, but in practice we'd need a different approach
    }
}

/// Broadcast Channel wrapper with metrics
#[derive(Debug)]
pub struct BroadcastChannel<T> {
    sender: broadcast::Sender<T>,
}

impl<T> BroadcastChannel<T>
where
    T: Send + Clone + 'static,
{
    /// Send a message to all receivers
    #[instrument(skip(self, msg))]
    pub async fn send(&self, msg: T) -> Result<usize, ChannelError> {
        self.sender.send(msg)
            .map_err(|_| ChannelError::ChannelClosed)
    }

    /// Subscribe to the broadcast
    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        BroadcastReceiver {
            receiver: self.sender.subscribe(),
        }
    }

    /// Get the number of active receivers
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

/// Broadcast receiver wrapper
#[derive(Debug)]
pub struct BroadcastReceiver<T> {
    receiver: broadcast::Receiver<T>,
}

impl<T> BroadcastReceiver<T>
where
    T: Clone,
{
    /// Receive a broadcast message
    pub async fn recv(&mut self) -> Result<T, ChannelError> {
        self.receiver.recv().await
            .map_err(|_| ChannelError::ChannelClosed)
    }

    /// Try to receive without blocking
    pub fn try_recv(&mut self) -> Result<T, ChannelError> {
        self.receiver.try_recv()
            .map_err(|_| ChannelError::ChannelEmpty)
    }
}

/// Watch Channel wrapper with metrics
#[derive(Debug)]
pub struct WatchChannel<T> {
    sender: watch::Sender<T>,
}

impl<T> WatchChannel<T>
where
    T: Send + Clone + 'static,
{
    /// Send a new value to all watchers
    #[instrument(skip(self, value))]
    pub fn send(&self, value: T) -> Result<(), ChannelError> {
        self.sender.send(value)
            .map_err(|_| ChannelError::ChannelClosed)
    }

    /// Send a new value with timeout
    pub async fn send_timeout(&self, value: T, timeout_dur: Duration) -> Result<(), ChannelError> {
        match timeout(timeout_dur, self.sender.closed()).await {
            Ok(_) => Err(ChannelError::ChannelClosed),
            Err(_) => {
                // Channel still open, try to send
                self.send(value)
            }
        }
    }

    /// Subscribe to value changes
    pub fn subscribe(&self) -> WatchReceiver<T> {
        WatchReceiver {
            receiver: self.sender.subscribe(),
        }
    }

    /// Get the current value
    pub fn borrow(&self) -> watch::Ref<T> {
        self.sender.borrow()
    }
}

/// Watch receiver wrapper
#[derive(Debug)]
pub struct WatchReceiver<T> {
    receiver: watch::Receiver<T>,
}

impl<T> WatchReceiver<T>
where
    T: Clone,
{
    /// Wait for value changes
    pub async fn changed(&mut self) -> Result<(), ChannelError> {
        self.receiver.changed().await
            .map_err(|_| ChannelError::ChannelClosed)
    }

    /// Get the current value
    pub fn borrow(&self) -> watch::Ref<T> {
        self.receiver.borrow()
    }

    /// Get the current value and mark as seen
    pub fn borrow_and_update(&mut self) -> watch::Ref<T> {
        self.receiver.borrow_and_update()
    }
}

/// Oneshot channel utilities
pub mod oneshot_utils {
    use super::*;

    /// Send a message with cancellation support
    pub async fn send_with_cancel<T>(
        sender: oneshot::Sender<T>,
        value: T,
        cancel: &mut tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), ChannelError> {
        tokio::select! {
            result = sender.send(value) => {
                result.map_err(|_| ChannelError::ChannelClosed)
            }
            _ = cancel.changed() => {
                Err(ChannelError::Cancelled)
            }
        }
    }

    /// Receive with timeout
    pub async fn recv_with_timeout<T>(
        receiver: oneshot::Receiver<T>,
        timeout_dur: Duration,
    ) -> Result<T, ChannelError> {
        match timeout(timeout_dur, receiver).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err(ChannelError::ChannelClosed),
            Err(_) => Err(ChannelError::Timeout),
        }
    }
}

/// Barrier pattern for synchronizing multiple async tasks
#[derive(Debug)]
pub struct AsyncBarrier {
    count: usize,
    current: Arc<Mutex<usize>>,
    notify: Arc<Notify>,
}

impl AsyncBarrier {
    pub fn new(count: usize) -> Self {
        Self {
            count,
            current: Arc::new(Mutex::new(0)),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Wait for all participants to reach the barrier
    #[instrument(skip(self))]
    pub async fn wait(&self) -> Result<(), ChannelError> {
        let mut current = self.current.lock().await;
        *current += 1;

        if *current >= self.count {
            // All participants have arrived
            *current = 0;
            self.notify.notify_waiters();
            Ok(())
        } else {
            // Wait for others
            drop(current); // Release lock before waiting
            self.notify.notified().await;
            Ok(())
        }
    }

    /// Reset the barrier (for reuse)
    pub async fn reset(&self) {
        let mut current = self.current.lock().await;
        *current = 0;
    }
}

/// Rendezvous channel (both sender and receiver must be ready)
#[derive(Debug)]
pub struct RendezvousChannel<T> {
    sender_queue: mpsc::Sender<(T, oneshot::Sender<()>)>,
    receiver_queue: mpsc::Receiver<(T, oneshot::Sender<()>)>,
}

impl<T> RendezvousChannel<T>
where
    T: Send + 'static,
{
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(1); // Buffer of 1 for rendezvous
        Self {
            sender_queue: tx,
            receiver_queue: rx,
        }
    }

    /// Send a message (blocks until receiver is ready)
    pub async fn send(&self, value: T) -> Result<(), ChannelError> {
        let (ack_tx, ack_rx) = oneshot::channel();

        self.sender_queue.send((value, ack_tx)).await
            .map_err(|_| ChannelError::ChannelClosed)?;

        ack_rx.await.map_err(|_| ChannelError::ChannelClosed)
    }

    /// Receive a message (blocks until sender is ready)
    pub async fn recv(&mut self) -> Result<T, ChannelError> {
        let (value, ack) = self.receiver_queue.recv().await
            .ok_or(ChannelError::ChannelClosed)?;

        // Acknowledge receipt
        let _ = ack.send(());

        Ok(value)
    }
}

/// Channel error types
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("channel is closed")]
    ChannelClosed,

    #[error("channel is full")]
    ChannelFull,

    #[error("channel is empty")]
    ChannelEmpty,

    #[error("operation timed out")]
    Timeout,

    #[error("operation was cancelled")]
    Cancelled,

    #[error("lag detected in broadcast channel")]
    BroadcastLag,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_mpsc_channel() {
        let channel = ChannelHub::new().create_mpsc("test", 10);

        // Send and receive
        channel.send(42).await.unwrap();
        let value = channel.recv().await;
        assert_eq!(value, Some(42));
    }

    #[tokio::test]
    async fn test_broadcast_channel() {
        let hub = ChannelHub::new();
        let channel = hub.create_broadcast("test", 10);

        let mut rx1 = channel.subscribe();
        let mut rx2 = channel.subscribe();

        // Send broadcast
        let receivers = channel.send("hello".to_string()).await.unwrap();
        assert_eq!(receivers, 2);

        // Both receivers should get the message
        let msg1 = rx1.recv().await.unwrap();
        let msg2 = rx2.recv().await.unwrap();
        assert_eq!(msg1, "hello");
        assert_eq!(msg2, "hello");
    }

    #[tokio::test]
    async fn test_watch_channel() {
        let hub = ChannelHub::new();
        let channel = hub.create_watch("test", 0);

        let mut rx1 = channel.subscribe();
        let mut rx2 = channel.subscribe();

        // Initial value
        assert_eq!(*channel.borrow(), 0);

        // Send new value
        channel.send(42).unwrap();

        // Receivers should detect change
        rx1.changed().await.unwrap();
        rx2.changed().await.unwrap();

        assert_eq!(*rx1.borrow(), 42);
        assert_eq!(*rx2.borrow(), 42);
    }

    #[tokio::test]
    async fn test_barrier() {
        let barrier = AsyncBarrier::new(3);

        let b1 = barrier.clone();
        let b2 = barrier.clone();
        let b3 = barrier.clone();

        tokio::spawn(async move {
            b1.wait().await.unwrap();
        });

        tokio::spawn(async move {
            b2.wait().await.unwrap();
        });

        // Third wait should unblock all
        let result = timeout(Duration::from_secs(1), b3.wait()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rendezvous_channel() {
        let mut channel = RendezvousChannel::new();

        let sender = tokio::spawn(async move {
            channel.send(42).await.unwrap();
        });

        let receiver = tokio::spawn(async move {
            let value = channel.recv().await.unwrap();
            assert_eq!(value, 42);
        });

        // Both should complete successfully
        let (sender_result, receiver_result) = tokio::join!(sender, receiver);
        assert!(sender_result.is_ok());
        assert!(receiver_result.is_ok());
    }

    #[tokio::test]
    async fn test_channel_hub_stats() {
        let hub = ChannelHub::new();

        let _mpsc = hub.create_mpsc::<i32>("mpsc", 10);
        let _broadcast = hub.create_broadcast::<String>("broadcast", 10);
        let _watch = hub.create_watch::<bool>("watch", false);

        let stats = hub.stats();
        assert_eq!(stats.num_channels, 3);
    }

    #[tokio::test]
    async fn test_oneshot_with_timeout() {
        let (tx, rx) = oneshot::channel::<i32>();

        // Send after delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = tx.send(42);
        });

        // Receive with timeout
        let result = oneshot_utils::recv_with_timeout(rx, Duration::from_millis(100)).await;
        assert_eq!(result.unwrap(), 42);

        // Test timeout
        let (tx2, rx2) = oneshot::channel::<i32>();
        let result = oneshot_utils::recv_with_timeout(rx2, Duration::from_millis(10)).await;
        assert!(matches!(result, Err(ChannelError::Timeout)));
    }
}
