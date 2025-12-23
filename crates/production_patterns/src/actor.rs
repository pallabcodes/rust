//! Actor Pattern Implementation
//!
//! The actor pattern provides state ownership and message passing, preventing
//! shared mutable state and enabling fault isolation. This is a critical pattern
//! for SDE-3 level systems work.
//!
//! ## Key Concepts
//!
//! - **State Ownership**: Each actor owns its state exclusively
//! - **Message Passing**: Communication via typed messages
//! - **Mailbox**: Bounded channel for incoming messages
//! - **Fault Isolation**: Actor failures don't crash the system
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::actor::{Actor, ActorHandle, Message};
//!
//! #[derive(Debug)]
//! struct CounterActor {
//!     count: i64,
//! }
//!
//! #[derive(Debug)]
//! enum CounterMsg {
//!     Increment(i64),
//!     GetCount,
//! }
//!
//! impl Message for CounterMsg {
//!     type Response = i64;
//! }
//!
//! #[async_trait::async_trait]
//! impl Actor for CounterActor {
//!     type Message = CounterMsg;
//!
//!     async fn handle(&mut self, msg: Self::Message) -> Self::Response {
//!         match msg {
//!             CounterMsg::Increment(delta) => {
//!                 self.count += delta;
//!                 self.count
//!             }
//!             CounterMsg::GetCount => self.count,
//!         }
//!     }
//! }
//!
//! // Usage
//! let handle = CounterActor { count: 0 }.spawn();
//! handle.send(CounterMsg::Increment(5)).await;
//! let count = handle.send(CounterMsg::GetCount).await;
//! ```

use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument};

use crate::error::ActorError;
use crate::common::{Metrics, ShutdownCoordinator};

/// Message trait for actor communication
pub trait Message: Send + Debug + 'static {
    type Response: Send + Debug + 'static;
}

/// Core actor trait that must be implemented by all actors
#[async_trait::async_trait]
pub trait Actor: Send + 'static {
    type Message: Message;

    /// Handle a single message and return a response
    async fn handle(&mut self, msg: Self::Message) -> <Self::Message as Message>::Response;

    /// Optional: Called when actor starts
    async fn on_start(&mut self) -> Result<(), ActorError> {
        Ok(())
    }

    /// Optional: Called when actor stops
    async fn on_stop(&mut self) -> Result<(), ActorError> {
        Ok(())
    }

    /// Optional: Process name for logging
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Handle for communicating with a running actor
#[derive(Debug)]
pub struct ActorHandle<M: Message> {
    sender: mpsc::Sender<Envelope<M>>,
    _shutdown: ShutdownCoordinator,
}

impl<M: Message> ActorHandle<M> {
    /// Send a message to the actor and wait for response
    #[instrument(skip(self, msg), fields(actor = %std::any::type_name::<M>()))]
    pub async fn send(&self, msg: M) -> Result<M::Response, ActorError> {
        let (tx, rx) = oneshot::channel();

        self.sender.send(Envelope {
            message: msg,
            responder: tx,
        }).await.map_err(|_| ActorError::MailboxFull)?;

        rx.await.map_err(|_| ActorError::ShuttingDown)
    }

    /// Try to send a message without waiting (non-blocking)
    pub fn try_send(&self, msg: M) -> Result<(), ActorError> {
        let (tx, rx) = oneshot::channel();

        self.sender.try_send(Envelope {
            message: msg,
            responder: tx,
        }).map_err(|_| ActorError::MailboxFull)
    }

    /// Check if actor is still running
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

impl<M: Message> Clone for ActorHandle<M> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            _shutdown: self._shutdown.clone(),
        }
    }
}

/// Internal message envelope with response channel
#[derive(Debug)]
struct Envelope<M: Message> {
    message: M,
    responder: oneshot::Sender<M::Response>,
}

/// Actor system for managing multiple actors
#[derive(Debug)]
pub struct ActorSystem {
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
}

impl ActorSystem {
    pub fn new() -> Self {
        Self {
            shutdown: ShutdownCoordinator::new(),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Spawn an actor in this system
    pub fn spawn<A>(&self, actor: A) -> ActorHandle<A::Message>
    where
        A: Actor,
        A::Message: Message,
    {
        let (tx, rx) = mpsc::channel(32); // Configurable mailbox size
        let shutdown = self.shutdown.clone();
        let metrics = self.metrics.clone();

        let handle = ActorHandle {
            sender: tx,
            _shutdown: shutdown.clone(),
        };

        tokio::spawn(async move {
            if let Err(e) = run_actor(actor, rx, shutdown, metrics).await {
                error!("Actor failed: {}", e);
            }
        });

        handle
    }

    /// Gracefully shutdown all actors in the system
    pub async fn shutdown(&self) {
        self.shutdown.shutdown();
        // Give actors time to cleanup
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

impl Default for ActorSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait for spawning actors directly
pub trait SpawnExt: Actor + Sized {
    fn spawn(self) -> ActorHandle<Self::Message> {
        let system = ActorSystem::new();
        system.spawn(self)
    }
}

impl<A: Actor> SpawnExt for A {}

/// Internal actor runner
async fn run_actor<A>(
    mut actor: A,
    mut mailbox: mpsc::Receiver<Envelope<A::Message>>,
    shutdown: ShutdownCoordinator,
    metrics: Arc<Metrics>,
) -> Result<(), ActorError>
where
    A: Actor,
{
    info!("Starting actor: {}", actor.name());

    // Call startup hook
    if let Err(e) = actor.on_start().await {
        error!("Actor startup failed: {}", e);
        return Err(e);
    }

    loop {
        tokio::select! {
            // Handle shutdown signal
            _ = shutdown.wait_shutdown() => {
                info!("Shutting down actor: {}", actor.name());
                break;
            }

            // Handle incoming message
            envelope = mailbox.recv() => {
                match envelope {
                    Some(envelope) => {
                        let timer = crate::common::Timer::new();

                        // Process message
                        let result = actor.handle(envelope.message).await;

                        let duration = timer.elapsed();
                        metrics.record_operation(duration);

                        // Send response (ignore send errors - client may have gone away)
                        let _ = envelope.responder.send(result);
                    }
                    None => {
                        // Mailbox closed
                        break;
                    }
                }
            }
        }
    }

    // Call shutdown hook
    if let Err(e) = actor.on_stop().await {
        error!("Actor shutdown failed: {}", e);
        return Err(e);
    }

    info!("Actor stopped: {}", actor.name());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[derive(Debug, PartialEq)]
    enum TestMsg {
        Ping,
        Add(i32),
        Get,
    }

    impl Message for TestMsg {
        type Response = i32;
    }

    struct TestActor {
        value: i32,
    }

    #[async_trait::async_trait]
    impl Actor for TestActor {
        type Message = TestMsg;

        async fn handle(&mut self, msg: Self::Message) -> Self::Response {
            match msg {
                TestMsg::Ping => 42,
                TestMsg::Add(n) => {
                    self.value += n;
                    self.value
                }
                TestMsg::Get => self.value,
            }
        }

        fn name(&self) -> &'static str {
            "TestActor"
        }
    }

    #[tokio::test]
    async fn test_basic_actor() {
        let actor = TestActor { value: 0 };
        let handle = actor.spawn();

        // Test ping
        let response = handle.send(TestMsg::Ping).await.unwrap();
        assert_eq!(response, 42);

        // Test add
        let response = handle.send(TestMsg::Add(10)).await.unwrap();
        assert_eq!(response, 10);

        // Test get
        let response = handle.send(TestMsg::Get).await.unwrap();
        assert_eq!(response, 10);
    }

    #[tokio::test]
    async fn test_actor_shutdown() {
        let actor = TestActor { value: 0 };
        let system = ActorSystem::new();
        let handle = system.spawn(actor);

        // Send a message
        let response = handle.send(TestMsg::Ping).await.unwrap();
        assert_eq!(response, 42);

        // Shutdown system
        system.shutdown().await;

        // Actor should be closed
        assert!(handle.is_closed());
    }

    #[tokio::test]
    async fn test_mailbox_full() {
        let actor = TestActor { value: 0 };
        let handle = actor.spawn();

        // Fill mailbox (capacity is 32)
        for _ in 0..35 {
            let _ = handle.try_send(TestMsg::Ping);
        }

        // Next send should fail
        let result = timeout(Duration::from_millis(100), handle.send(TestMsg::Ping)).await;
        assert!(result.is_err() || matches!(result, Ok(Err(ActorError::MailboxFull))));
    }
}
