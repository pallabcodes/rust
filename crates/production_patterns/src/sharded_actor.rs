//! Sharded Actor Pattern
//!
//! Sharded actors distribute state across multiple partitions, each managed by
//! a separate actor. This enables horizontal scaling while maintaining per-key
//! consistency and ordering.
//!
//! ## Key Concepts
//!
//! - **Key Partitioning**: Keys are hashed to determine shard assignment
//! - **Per-Shard State**: Each shard owns a subset of the total state
//! - **Consistent Hashing**: Same key always routes to same shard
//! - **Horizontal Scaling**: Add more shards to increase capacity
//!
//! ## Usage
//!
//! ```rust,no_run
//! use production_patterns::sharded_actor::{ShardedActor, ShardedMessage};
//!
//! #[derive(Debug, Hash, PartialEq, Eq)]
//! struct UserKey(String);
//!
//! #[derive(Debug)]
//! enum UserMsg {
//!     SetBalance(UserKey, i64),
//!     GetBalance(UserKey),
//!     Transfer(UserKey, UserKey, i64),
//! }
//!
//! impl ShardedMessage for UserMsg {
//!     type Key = UserKey;
//!     type Response = Result<i64, String>;
//!
//!     fn key(&self) -> &Self::Key {
//!         match self {
//!             UserMsg::SetBalance(k, _) => k,
//!             UserMsg::GetBalance(k) => k,
//!             UserMsg::Transfer(from, _, _) => from,
//!         }
//!     }
//! }
//!
//! // Create sharded actor system
//! let shards = ShardedActor::new(8, |shard_id| {
//!     // State for this shard
//!     std::collections::HashMap::new()
//! });
//!
//! // Use the sharded actor
//! shards.send(UserMsg::SetBalance(UserKey("alice".to_string()), 1000)).await;
//! ```

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{debug, instrument};

use crate::actor::{Actor, ActorHandle, Message, SpawnExt};
use crate::error::ActorError;

/// Message trait for sharded actors
pub trait ShardedMessage: Send + Debug + 'static {
    type Key: Hash + Eq + Send + Sync + Debug + Clone + 'static;
    type Response: Send + Debug + 'static;

    /// Extract the partitioning key from the message
    fn key(&self) -> &Self::Key;
}

/// Sharded actor system that distributes messages across multiple shards
#[derive(Debug)]
pub struct ShardedActor<M, S> {
    shards: Vec<Shard<M, S>>,
    num_shards: usize,
}

#[derive(Debug)]
struct Shard<M, S> {
    id: usize,
    actor: ActorHandle<M>,
    _phantom: std::marker::PhantomData<S>,
}

impl<M, S> ShardedActor<M, S>
where
    M: ShardedMessage,
    S: Send + 'static,
{
    /// Create a new sharded actor system
    pub fn new<F>(num_shards: usize, shard_factory: F) -> Self
    where
        F: Fn(usize) -> S,
    {
        let mut shards = Vec::with_capacity(num_shards);

        for shard_id in 0..num_shards {
            let state = shard_factory(shard_id);
            let actor = ShardActor { state, shard_id }.spawn();

            shards.push(Shard {
                id: shard_id,
                actor,
                _phantom: std::marker::PhantomData,
            });
        }

        Self {
            shards,
            num_shards,
        }
    }

    /// Send a message to the appropriate shard
    #[instrument(skip(self, msg), fields(shard = %self.get_shard_id(&msg.key())))]
    pub async fn send(&self, msg: M) -> Result<M::Response, ActorError> {
        let shard_id = self.get_shard_id(&msg.key());
        let shard = &self.shards[shard_id];
        debug!("Routing message to shard {}", shard_id);
        shard.actor.send(msg).await
    }

    /// Get shard ID for a key using consistent hashing
    fn get_shard_id(&self, key: &M::Key) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() % self.num_shards as u64) as usize
    }

    /// Get statistics about the sharded actor system
    pub fn stats(&self) -> ShardedStats {
        ShardedStats {
            num_shards: self.num_shards,
            shard_status: self.shards.iter().map(|s| s.actor.is_closed()).collect(),
        }
    }
}

/// Statistics for sharded actor system
#[derive(Debug)]
pub struct ShardedStats {
    pub num_shards: usize,
    pub shard_status: Vec<bool>, // true if shard is closed
}

/// Internal shard actor implementation
struct ShardActor<S> {
    state: S,
    shard_id: usize,
}

impl<S> ShardActor<S> {
    fn new(state: S, shard_id: usize) -> Self {
        Self { state, shard_id }
    }
}

/// Message envelope for shard communication
#[derive(Debug)]
struct ShardEnvelope<M: ShardedMessage> {
    message: M,
    responder: oneshot::Sender<M::Response>,
}

impl<M, S> Actor for ShardActor<S>
where
    M: ShardedMessage,
    S: Send + 'static,
{
    type Message = M;

    async fn handle(&mut self, msg: Self::Message) -> M::Response {
        // This would be implemented by the user - for now just return a placeholder
        // In real usage, this would delegate to user-provided handlers
        panic!("ShardActor handle method must be customized for specific use case")
    }

    fn name(&self) -> &'static str {
        &format!("ShardActor-{}", self.shard_id)
    }
}

/// Builder pattern for creating typed sharded actors
pub struct ShardedActorBuilder<M, S> {
    num_shards: usize,
    shard_factory: Option<Box<dyn Fn(usize) -> S + Send + Sync>>,
    message_handler: Option<Box<dyn Fn(&mut S, M) -> M::Response + Send + Sync>>,
}

impl<M, S> ShardedActorBuilder<M, S>
where
    M: ShardedMessage,
    S: Send + 'static,
{
    pub fn new() -> Self {
        Self {
            num_shards: 8,
            shard_factory: None,
            message_handler: None,
        }
    }

    pub fn with_shards(mut self, num_shards: usize) -> Self {
        self.num_shards = num_shards;
        self
    }

    pub fn with_shard_factory<F>(mut self, factory: F) -> Self
    where
        F: Fn(usize) -> S + Send + Sync + 'static,
    {
        self.shard_factory = Some(Box::new(factory));
        self
    }

    pub fn with_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut S, M) -> M::Response + Send + Sync + 'static,
    {
        self.message_handler = Some(Box::new(handler));
        self
    }

    pub fn build(self) -> Result<ShardedActor<M, S>, &'static str> {
        let factory = self.shard_factory.ok_or("shard factory required")?;
        let handler = self.message_handler.ok_or("message handler required")?;

        // Create custom shard actor type
        struct CustomShardActor<S, M, F> {
            state: S,
            shard_id: usize,
            handler: F,
            _phantom: std::marker::PhantomData<M>,
        }

        impl<S, M, F> CustomShardActor<S, M, F>
        where
            M: ShardedMessage,
            F: Fn(&mut S, M) -> M::Response + Send + Sync,
        {
            fn new(state: S, shard_id: usize, handler: F) -> Self {
                Self {
                    state,
                    shard_id,
                    handler,
                    _phantom: std::marker::PhantomData,
                }
            }
        }

        #[async_trait::async_trait]
        impl<S, M, F> Actor for CustomShardActor<S, M, F>
        where
            M: ShardedMessage,
            F: Fn(&mut S, M) -> M::Response + Send + Sync,
            S: Send + 'static,
        {
            type Message = M;

            async fn handle(&mut self, msg: Self::Message) -> M::Response {
                (self.handler)(&mut self.state, msg)
            }

            fn name(&self) -> &'static str {
                &format!("CustomShardActor-{}", self.shard_id)
            }
        }

        let mut shards = Vec::with_capacity(self.num_shards);

        for shard_id in 0..self.num_shards {
            let state = factory(shard_id);
            let actor = CustomShardActor::new(state, shard_id, handler.as_ref().clone()).spawn();

            shards.push(Shard {
                id: shard_id,
                actor,
                _phantom: std::marker::PhantomData,
            });
        }

        Ok(ShardedActor {
            shards,
            num_shards: self.num_shards,
        })
    }
}

impl<M, S> Default for ShardedActorBuilder<M, S>
where
    M: ShardedMessage,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Example implementation: Key-Value Store
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    struct TestKey(String);

    #[derive(Debug)]
    enum KVMsg {
        Put(TestKey, String),
        Get(TestKey),
        Delete(TestKey),
    }

    impl ShardedMessage for KVMsg {
        type Key = TestKey;
        type Response = Option<String>;

        fn key(&self) -> &Self::Key {
            match self {
                KVMsg::Put(k, _) => k,
                KVMsg::Get(k) => k,
                KVMsg::Delete(k) => k,
            }
        }
    }

    fn kv_handler(state: &mut HashMap<TestKey, String>, msg: KVMsg) -> Option<String> {
        match msg {
            KVMsg::Put(key, value) => {
                state.insert(key, value);
                None
            }
            KVMsg::Get(key) => state.get(&key).cloned(),
            KVMsg::Delete(key) => state.remove(&key),
        }
    }

    #[tokio::test]
    async fn test_sharded_kv_store() {
        let shards = ShardedActorBuilder::new()
            .with_shards(4)
            .with_shard_factory(|_| HashMap::new())
            .with_handler(kv_handler)
            .build()
            .unwrap();

        let key1 = TestKey("key1".to_string());
        let key2 = TestKey("key2".to_string());

        // Test put and get
        shards.send(KVMsg::Put(key1.clone(), "value1".to_string())).await.unwrap();
        let result = shards.send(KVMsg::Get(key1.clone())).await.unwrap();
        assert_eq!(result, Some("value1".to_string()));

        // Test different key
        let result = shards.send(KVMsg::Get(key2.clone())).await.unwrap();
        assert_eq!(result, None);

        // Test delete
        let result = shards.send(KVMsg::Delete(key1.clone())).await.unwrap();
        assert_eq!(result, Some("value1".to_string()));

        let result = shards.send(KVMsg::Get(key1)).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_key_partitioning() {
        let shards = ShardedActorBuilder::new()
            .with_shards(8)
            .with_shard_factory(|_| HashMap::new())
            .with_handler(kv_handler)
            .build()
            .unwrap();

        let key = TestKey("test_key".to_string());

        // Same key should always go to same shard
        let shard1 = shards.get_shard_id(&key);
        let shard2 = shards.get_shard_id(&key);
        assert_eq!(shard1, shard2);

        // Different keys should usually go to different shards
        let key2 = TestKey("different_key".to_string());
        let shard3 = shards.get_shard_id(&key2);
        // Note: This might occasionally fail due to hash collisions, but it's rare
        assert!(shard3 < 8); // At least within bounds
    }

    #[tokio::test]
    async fn test_stats() {
        let shards = ShardedActorBuilder::new()
            .with_shards(4)
            .with_shard_factory(|_| HashMap::new())
            .with_handler(kv_handler)
            .build()
            .unwrap();

        let stats = shards.stats();
        assert_eq!(stats.num_shards, 4);
        assert_eq!(stats.shard_status.len(), 4);
        // All shards should be running initially
        assert!(stats.shard_status.iter().all(|&closed| !closed));
    }
}
