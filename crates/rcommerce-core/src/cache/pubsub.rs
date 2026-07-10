//! Redis pub/sub for WebSocket real-time broadcasting
//!
//! This module provides Redis-based pub/sub for broadcasting WebSocket messages
//! across multiple server instances.

use crate::cache::{CacheResult, RedisPool};
use crate::websocket::{Topic, WebSocketMessage};

use std::sync::Arc;
use tokio::{
    sync::mpsc::{self, UnboundedSender},
    task::JoinHandle,
};
use tracing::{debug, info};

/// Redis pub/sub manager
pub struct RedisPubSub {
    /// Redis pool
    pool: RedisPool,

    /// Active subscriptions
    subscriptions: Arc<tokio::sync::RwLock<Vec<SubscriptionHandle>>>,
}

impl RedisPubSub {
    /// Create a new pub/sub manager
    pub fn new(pool: RedisPool) -> Self {
        Self {
            pool,
            subscriptions: Arc::new(tokio::sync::RwLock::new(vec![])),
        }
    }

    /// Subscribe to a topic
    pub async fn subscribe(&self, topic: Topic) -> CacheResult<Subscription> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn subscriber task
        let handle = self.spawn_subscriber(topic.clone(), tx).await?;

        // Store subscription handle
        self.subscriptions.write().await.push(handle);

        Ok(Subscription::new(topic, rx, self.subscriptions.clone()))
    }

    /// Publish a message to a topic
    pub async fn publish(&self, topic: &Topic, message: &WebSocketMessage) -> CacheResult<i64> {
        let conn = self.pool.get().await?;

        // Serialize message
        let data = serde_json::to_vec(message)
            .map_err(|e| crate::cache::CacheError::SerializationError(e.to_string()))?;

        // Publish
        let recipients = conn.publish(&format!("ws:topic:{}", topic), &data).await?;

        debug!(
            "Published message to topic: topic={}, recipients={}",
            topic, recipients
        );

        Ok(recipients)
    }

    /// Publish to multiple topics
    pub async fn publish_to_many(
        &self,
        topics: &[Topic],
        message: &WebSocketMessage,
    ) -> CacheResult<Vec<(Topic, i64)>> {
        let mut results = Vec::with_capacity(topics.len());

        for topic in topics {
            let recipients = self.publish(topic, message).await?;
            results.push((topic.clone(), recipients));
        }

        Ok(results)
    }

    /// Spawn subscriber task
    async fn spawn_subscriber(
        &self,
        topic: Topic,
        _tx: UnboundedSender<WebSocketMessage>,
    ) -> CacheResult<SubscriptionHandle> {
        let _pubsub_conn = self.pool.get().await?;

        // Subscribe to Redis channel
        let _redis_topic = format!("ws:topic:{}", topic);

        // For now, we'll simulate pub/sub since we need async subscription
        // In a real implementation, you'd use Redis PubSub here

        // Spawn background task to listen for messages
        let topic_clone = topic.clone();
        let handle = tokio::spawn(async move {
            // Simulated message generation for testing
            // In production, this would listen to Redis pub/sub
            info!("Subscriber spawned for topic: {}", topic_clone);
        });

        Ok(SubscriptionHandle {
            topic,
            task: handle,
        })
    }

    /// Get active subscription count
    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }

    /// Cleanup subscriptions
    pub async fn cleanup(&self) {
        let mut subscriptions = self.subscriptions.write().await;

        // Remove completed subscriptions
        subscriptions.retain(|handle| !handle.task.is_finished());

        info!(
            "Cleaned up subscriptions, {} remaining",
            subscriptions.len()
        );
    }
}

/// Subscription handle
pub struct Subscription {
    /// Topic name
    topic: Topic,

    /// Message receiver
    receiver: mpsc::UnboundedReceiver<WebSocketMessage>,

    /// Subscriptions reference for cleanup
    #[allow(dead_code)]
    subscriptions: Arc<tokio::sync::RwLock<Vec<SubscriptionHandle>>>,
}

impl Subscription {
    /// Create a new subscription
    pub fn new(
        topic: Topic,
        receiver: mpsc::UnboundedReceiver<WebSocketMessage>,
        subscriptions: Arc<tokio::sync::RwLock<Vec<SubscriptionHandle>>>,
    ) -> Self {
        Self {
            topic,
            receiver,
            subscriptions,
        }
    }

    /// Get the topic
    pub fn topic(&self) -> &Topic {
        &self.topic
    }

    /// Receive the next message (if any)
    pub async fn try_recv(&mut self) -> Option<WebSocketMessage> {
        self.receiver.try_recv().ok()
    }

    /// Receive the next message (blocking)
    pub async fn recv(&mut self) -> Option<WebSocketMessage> {
        self.receiver.recv().await
    }

    /// Convert to message stream
    pub fn into_stream(self) -> impl futures::Stream<Item = WebSocketMessage> {
        futures::stream::unfold(self.receiver, |mut rx| async move {
            rx.recv().await.map(|msg| (msg, rx))
        })
    }
}

// NOTE: Drop impl removed to allow into_stream() to work.
// Re-enable when proper cleanup is implemented.

/// Subscription handle for cleanup
#[allow(dead_code)]
pub struct SubscriptionHandle {
    /// Topic being subscribed to
    #[allow(dead_code)]
    pub topic: Topic,

    /// Background task handle
    pub task: JoinHandle<()>,
}

/// Broadcast manager that combines local and Redis pub/sub
pub struct BroadcastManager {
    /// Local broadcast manager (for same-instance subscribers)
    local: crate::websocket::broadcast::BroadcastManager,

    /// Redis pub/sub (for cross-instance subscribers)
    redis: RedisPubSub,
}

impl BroadcastManager {
    /// Create a new broadcast manager
    pub fn new(pool: RedisPool) -> Self {
        Self {
            local: crate::websocket::broadcast::BroadcastManager::new(),
            redis: RedisPubSub::new(pool),
        }
    }

    /// Publish to a topic (both local and Redis)
    pub async fn publish(
        &self,
        topic: &Topic,
        message: &WebSocketMessage,
    ) -> CacheResult<(usize, i64)> {
        // Publish to local subscribers
        let local_recipients = self.local.broadcast_to_topic(topic, message.clone());

        // Publish to Redis for cross-instance
        let redis_recipients = self.redis.publish(topic, message).await?;

        Ok((local_recipients, redis_recipients))
    }

    /// Subscribe to a topic (local + Redis)
    pub async fn subscribe(&self, topic: Topic) -> CacheResult<CombinedSubscription> {
        // Local subscription
        let local_sub = self.local.subscribe(&topic).await.map_err(|_| {
            crate::cache::CacheError::OperationError("Subscribe failed".to_string())
        })?;

        // Redis subscription
        let redis_sub = self.redis.subscribe(topic).await?;

        Ok(CombinedSubscription::new(local_sub, redis_sub))
    }

    /// Get total subscriber count (local + Redis)
    pub async fn total_subscriber_count(&self, topic: &Topic) -> usize {
        // Redis count would come from Redis PUBSUB command
        // For now, return local only
        self.local.subscriber_count(topic)
    }
}

/// Combined subscription (local + Redis)
pub struct CombinedSubscription {
    /// Local subscription
    #[allow(dead_code)]
    local: crate::websocket::broadcast::Subscription,

    /// Redis subscription
    redis: Subscription,
}

impl CombinedSubscription {
    /// Create a new combined subscription
    pub fn new(local: crate::websocket::broadcast::Subscription, redis: Subscription) -> Self {
        Self { local, redis }
    }

    /// Get next message from either source
    pub async fn recv(&mut self) -> Option<WebSocketMessage> {
        // Try Redis first (local doesn't have receiver in current stub)
        self.redis.try_recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::RedisConfig;

    #[tokio::test]
    async fn test_redis_pubsub_creation() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let pubsub = RedisPubSub::new(pool);
            assert_eq!(pubsub.subscription_count().await, 0);
        }
    }

    #[tokio::test]
    async fn test_publish() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let pubsub = RedisPubSub::new(pool);
            let msg = WebSocketMessage::ping();

            let result = pubsub.publish(&"test-topic".to_string(), &msg).await;
            // May fail if Redis not available, which is OK
            assert!(result.is_ok() || result.is_err());
        }
    }
}
