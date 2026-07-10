//! Cache backends for API responses

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cached response data
#[derive(Clone, Serialize, Deserialize)]
pub struct CachedResponse {
    pub body: Vec<u8>,
    pub content_type: String,
    pub cached_at: DateTime<Utc>,
    #[serde(skip)]
    pub backend: String,
}

/// Cache backend trait
#[async_trait]
pub trait CacheBackend: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<CachedResponse>>;
    async fn set(&self, key: &str, value: &CachedResponse, ttl_secs: u64) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn health_check(&self) -> bool;
}

/// Cache wrapper
pub struct Cache {
    backend: Arc<dyn CacheBackend>,
}

impl Cache {
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self { backend }
    }

    pub async fn get(&self, key: &str) -> Result<Option<CachedResponse>> {
        let mut result = self.backend.get(key).await?;
        if let Some(ref mut cached) = result {
            cached.backend = "cache".to_string();
        }
        Ok(result)
    }

    pub async fn set(&self, key: &str, value: &CachedResponse, ttl_secs: u64) -> Result<()> {
        self.backend.set(key, value, ttl_secs).await
    }

    pub async fn health_check(&self) -> bool {
        self.backend.health_check().await
    }
}

/// In-memory LRU cache
pub struct MemoryCache {
    cache: RwLock<lru::LruCache<String, CachedResponse>>,
    ttl_secs: u64,
}

impl MemoryCache {
    pub fn new(capacity: usize, ttl_secs: u64) -> Self {
        Self {
            cache: RwLock::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(capacity).unwrap(),
            )),
            ttl_secs,
        }
    }
}

#[async_trait]
impl CacheBackend for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<CachedResponse>> {
        let cache = self.cache.read().await;
        if let Some(cached) = cache.peek(key) {
            let age = Utc::now()
                .signed_duration_since(cached.cached_at)
                .num_seconds() as u64;
            if age < self.ttl_secs {
                return Ok(Some(cached.clone()));
            }
        }
        Ok(None)
    }

    async fn set(&self, key: &str, value: &CachedResponse, _ttl_secs: u64) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.put(key.to_string(), value.clone());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.pop(key);
        Ok(())
    }

    async fn health_check(&self) -> bool {
        true
    }
}

/// Redis cache backend
pub struct RedisCache {
    client: redis::aio::ConnectionManager,
    default_ttl_secs: u64,
}

impl RedisCache {
    pub async fn new(redis_url: &str, default_ttl_secs: u64) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let conn = client.get_connection_manager().await?;

        Ok(Self {
            client: conn,
            default_ttl_secs,
        })
    }
}

#[async_trait]
impl CacheBackend for RedisCache {
    async fn get(&self, key: &str) -> Result<Option<CachedResponse>> {
        let mut conn = self.client.clone();
        let data: Option<Vec<u8>> = redis::cmd("GET").arg(key).query_async(&mut conn).await?;

        match data {
            Some(bytes) => {
                let cached: CachedResponse = bincode::deserialize(&bytes)?;
                Ok(Some(cached))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &CachedResponse, ttl_secs: u64) -> Result<()> {
        let mut conn = self.client.clone();
        let data = bincode::serialize(value)?;
        let ttl = if ttl_secs > 0 {
            ttl_secs
        } else {
            self.default_ttl_secs
        };

        let _: () = redis::cmd("SETEX")
            .arg(key)
            .arg(ttl as i64)
            .arg(data)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.client.clone();
        let _: () = redis::cmd("DEL").arg(key).query_async(&mut conn).await?;
        Ok(())
    }

    async fn health_check(&self) -> bool {
        let mut conn = self.client.clone();
        redis::cmd("PING")
            .query_async::<_, String>(&mut conn)
            .await
            .is_ok()
    }
}
