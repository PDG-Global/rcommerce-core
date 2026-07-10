//! Advanced caching strategies for performance optimization

use lru::LruCache as LruCacheImpl;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Cache strategy trait
pub trait CacheStrategy<K, V> {
    /// Get value from cache
    fn get(&mut self, key: &K) -> Option<&V>;

    /// Put value into cache
    fn put(&mut self, key: K, value: V);

    /// Remove value from cache
    fn remove(&mut self, key: &K) -> Option<V>;

    /// Clear cache
    fn clear(&mut self);

    /// Get cache size
    fn len(&self) -> usize;

    /// Check if cache is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// LRU Cache implementation
pub struct LruCache<K, V> {
    inner: LruCacheImpl<K, V>,
    hits: u64,
    misses: u64,
}

impl<K, V> LruCache<K, V>
where
    K: Eq + std::hash::Hash,
{
    /// Create new LRU cache with capacity
    pub fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity).unwrap_or_else(|| NonZeroUsize::new(1).unwrap());
        Self {
            inner: lru::LruCache::new(capacity),
            hits: 0,
            misses: 0,
        }
    }

    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl<K, V> CacheStrategy<K, V> for LruCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    fn get(&mut self, key: &K) -> Option<&V> {
        match self.inner.get(key) {
            Some(value) => {
                self.hits += 1;
                Some(value)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    fn put(&mut self, key: K, value: V) {
        self.inner.put(key, value);
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        self.inner.pop(key)
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.hits = 0;
        self.misses = 0;
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

/// TTL-based cache implementation
pub struct TtlCache<K, V> {
    inner: HashMap<K, (V, Instant)>,
    ttl: Duration,
    hits: u64,
    misses: u64,
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    /// Create new TTL cache
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: HashMap::new(),
            ttl,
            hits: 0,
            misses: 0,
        }
    }

    /// Clean expired entries
    pub fn clean_expired(&mut self) {
        let now = Instant::now();
        self.inner.retain(|_, (_, expiry)| now < *expiry);
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            size: self.inner.len(),
            hits: self.hits,
            misses: self.misses,
            hit_rate: self.hit_rate(),
        }
    }

    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl<K, V> CacheStrategy<K, V> for TtlCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    fn get(&mut self, key: &K) -> Option<&V> {
        let now = Instant::now();

        // Check if entry exists and is expired
        let is_expired = self
            .inner
            .get(key)
            .map(|(_, expiry)| now >= *expiry)
            .unwrap_or(false);

        if is_expired {
            self.inner.remove(key);
            self.misses += 1;
            return None;
        }

        match self.inner.get(key) {
            Some((value, _)) => {
                self.hits += 1;
                Some(value)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    fn put(&mut self, key: K, value: V) {
        let expiry = Instant::now() + self.ttl;
        self.inner.insert(key, (value, expiry));
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        self.inner.remove(key).map(|(v, _)| v)
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.hits = 0;
        self.misses = 0;
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Thread-safe async cache wrapper
pub struct AsyncCache<K, V, C: CacheStrategy<K, V>> {
    inner: Arc<RwLock<C>>,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V, C: CacheStrategy<K, V> + Send + Sync> AsyncCache<K, V, C>
where
    K: Send + Sync,
    V: Send + Sync,
{
    /// Create new async cache
    pub fn new(cache: C) -> Self {
        Self {
            inner: Arc::new(RwLock::new(cache)),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get value from cache
    pub async fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        let mut cache = self.inner.write().await;
        cache.get(key).cloned()
    }

    /// Put value into cache
    pub async fn put(&self, key: K, value: V) {
        let mut cache = self.inner.write().await;
        cache.put(key, value);
    }

    /// Remove value from cache
    pub async fn remove(&self, key: &K) -> Option<V> {
        let mut cache = self.inner.write().await;
        cache.remove(key)
    }

    /// Clear cache
    pub async fn clear(&self) {
        let mut cache = self.inner.write().await;
        cache.clear();
    }

    /// Get cache size
    pub async fn len(&self) -> usize {
        let cache = self.inner.read().await;
        cache.len()
    }

    /// Check if cache is empty
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Current size
    pub size: usize,

    /// Number of cache hits
    pub hits: u64,

    /// Number of cache misses
    pub misses: u64,

    /// Hit rate (0.0 - 1.0)
    pub hit_rate: f64,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Cache type (LRU or TTL)
    pub cache_type: CacheType,

    /// Max size (for LRU)
    pub max_size: usize,

    /// TTL (for TTL cache)
    pub ttl: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheType {
    /// Least Recently Used
    Lru,

    /// Time To Live
    Ttl,
}
