//! Query result caching for database optimizations

use crate::cache::RedisPool;
use crate::performance::{PerformanceError, PerformanceResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Cached query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedQueryResult<T> {
    /// Query result data
    pub data: T,

    /// Cache timestamp
    pub cached_at: i64,

    /// TTL in seconds
    pub ttl_secs: u64,

    /// Query fingerprint (hash)
    pub fingerprint: String,
}

impl<T> CachedQueryResult<T> {
    /// Check if result is still valid
    pub fn is_valid(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        (now - self.cached_at) < self.ttl_secs as i64
    }

    /// Time until expiration
    pub fn time_until_expiration(&self) -> i64 {
        let now = chrono::Utc::now().timestamp();
        let expiry = self.cached_at + self.ttl_secs as i64;
        (expiry - now).max(0)
    }
}

/// Query cache manager
pub struct QueryCache {
    /// Redis pool
    pool: RedisPool,

    /// Default TTL for query results
    default_ttl: Duration,

    /// Key prefix for query cache
    key_prefix: String,
}

impl QueryCache {
    /// Create new query cache
    pub fn new(pool: RedisPool, default_ttl: Duration) -> Self {
        Self {
            pool,
            default_ttl,
            key_prefix: "query_cache".to_string(),
        }
    }

    /// Execute query with caching
    pub async fn execute_with_cache<F, Fut, T, E>(
        &self,
        query: &str,
        execute_fn: F,
        ttl: Option<Duration>,
    ) -> PerformanceResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        T: serde::Serialize + for<'de> serde::Deserialize<'de> + Clone,
        E: std::error::Error + Into<PerformanceError>,
    {
        // Generate query fingerprint
        let fingerprint = self.generate_fingerprint(query);
        let cache_key = self.cache_key(&fingerprint);

        // Try to get from cache
        if let Some(cached) = self.get_cached_result::<T>(&cache_key).await? {
            if cached.is_valid() {
                return Ok(cached.data);
            }
        }

        // Execute query
        let result = execute_fn().await.map_err(|e| e.into())?;

        // Cache result
        let ttl = ttl.unwrap_or(self.default_ttl);
        self.cache_result(&cache_key, &result, ttl).await?;

        Ok(result)
    }

    /// Get cached result
    async fn get_cached_result<T>(
        &self,
        key: &str,
    ) -> PerformanceResult<Option<CachedQueryResult<T>>>
    where
        T: serde::de::DeserializeOwned,
    {
        let conn = self.pool.get().await?;

        match conn.get(key).await? {
            Some(data) => {
                let result: CachedQueryResult<T> = serde_json::from_slice(&data)
                    .map_err(|e| PerformanceError::CacheError(e.to_string()))?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    /// Cache query result
    async fn cache_result<T>(&self, key: &str, result: &T, ttl: Duration) -> PerformanceResult<()>
    where
        T: serde::Serialize + Clone,
    {
        let conn = self.pool.get().await?;

        let cached_result = CachedQueryResult {
            data: result.clone(),
            cached_at: chrono::Utc::now().timestamp(),
            ttl_secs: ttl.as_secs(),
            fingerprint: key.to_string(),
        };

        let data = serde_json::to_vec(&cached_result)
            .map_err(|e| PerformanceError::CacheError(e.to_string()))?;

        conn.setex(key, ttl.as_secs(), &data).await?;

        Ok(())
    }

    /// Invalidate cached result
    pub async fn invalidate(&self, query: &str) -> PerformanceResult<bool> {
        let fingerprint = self.generate_fingerprint(query);
        let cache_key = self.cache_key(&fingerprint);

        let conn = self.pool.get().await?;
        conn.del(&cache_key).await?;

        Ok(true)
    }

    /// Invalidate all cached results matching pattern
    pub async fn invalidate_pattern(&self, pattern: &str) -> PerformanceResult<u64> {
        let conn = self.pool.get().await?;

        let cache_pattern = format!("{}:*", self.key_prefix);
        let keys: Vec<String> = conn.keys(&cache_pattern).await?;

        let mut deleted = 0;
        for key in &keys {
            if key.contains(pattern) {
                conn.del(key).await?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    /// Generate query fingerprint
    fn generate_fingerprint(&self, query: &str) -> String {
        use hex::encode;
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(query.trim().to_lowercase().as_bytes());
        encode(hasher.finalize())[..16].to_string()
    }

    /// Generate cache key
    fn cache_key(&self, fingerprint: &str) -> String {
        format!("{}:{}", self.key_prefix, fingerprint)
    }

    /// Get cache statistics
    pub async fn stats(&self) -> PerformanceResult<QueryCacheStats> {
        let conn = self.pool.get().await?;

        let pattern = format!("{}:*", self.key_prefix);
        let keys: Vec<String> = conn.keys(&pattern).await?;

        Ok(QueryCacheStats {
            cached_queries: keys.len(),
            total_size_bytes: keys.iter().map(|k: &String| k.len()).sum(),
        })
    }
}

/// Query cache statistics
#[derive(Debug, Clone)]
pub struct QueryCacheStats {
    /// Number of cached queries
    pub cached_queries: usize,

    /// Total size in bytes
    pub total_size_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::RedisConfig;

    #[tokio::test]
    async fn test_query_cache_creation() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let cache = QueryCache::new(pool, Duration::from_secs(300));
            assert_eq!(cache.key_prefix, "query_cache");
        }
    }
}
