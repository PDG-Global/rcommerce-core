//! Redis connection management with pooling

use crate::cache::{CacheError, CacheResult, RedisConfig};
use redis::aio::MultiplexedConnection as AsyncConnection;
use redis::{Client, Cmd, Pipeline, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Redis connection pool
#[derive(Clone)]
pub struct RedisPool {
    client: Arc<Client>,
    #[allow(dead_code)]
    config: Arc<RedisConfig>,
}

impl RedisPool {
    /// Create a new Redis connection pool
    pub async fn new(config: RedisConfig) -> CacheResult<Self> {
        let client = Client::open(&config.url[..])
            .map_err(|e| CacheError::ConnectionError(e.to_string()))?;

        tracing::info!("Redis pool created: url={}", config.url);

        Ok(Self {
            client: Arc::new(client),
            config: Arc::new(config),
        })
    }

    /// Get a multiplexed async connection from the pool
    pub async fn get(&self) -> CacheResult<RedisConnection> {
        let conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| CacheError::ConnectionError(e.to_string()))?;

        Ok(RedisConnection::new(conn))
    }
}

/// Single Redis connection
pub struct RedisConnection {
    inner: Mutex<AsyncConnection>,
}

impl RedisConnection {
    /// Create a new Redis connection
    pub fn new(inner: AsyncConnection) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    /// Execute a command and return Redis Value
    pub async fn execute(&self, cmd: Cmd) -> CacheResult<Value> {
        let mut conn = self.inner.lock().await;
        let value: Value = cmd
            .query_async(&mut *conn)
            .await
            .map_err(|e| CacheError::OperationError(e.to_string()))?;
        Ok(value)
    }

    /// Get a value by key
    pub async fn get(&self, key: &str) -> CacheResult<Option<Vec<u8>>> {
        let mut cmd = Cmd::new();
        cmd.arg("GET").arg(key);

        let value = self.execute(cmd).await?;
        match value {
            Value::BulkString(data) => Ok(Some(data)),
            Value::SimpleString(s) => Ok(Some(s.into_bytes())),
            Value::Nil => Ok(None),
            _ => Ok(None),
        }
    }

    /// Check if key exists (single key version)
    pub async fn exists(&self, key: &str) -> CacheResult<bool> {
        let mut cmd = Cmd::new();
        cmd.arg("EXISTS").arg(key);

        let value = self.execute(cmd).await?;
        match value {
            Value::Int(n) => Ok(n > 0),
            _ => Ok(false),
        }
    }

    /// Set a key to expire after TTL seconds
    pub async fn expire(&self, key: &str, ttl_secs: u64) -> CacheResult<bool> {
        let mut cmd = Cmd::new();
        cmd.arg("EXPIRE").arg(key).arg(ttl_secs);

        let value = self.execute(cmd).await?;
        match value {
            Value::Int(1) => Ok(true),
            _ => Ok(false),
        }
    }

    /// Increment a key's value
    pub async fn incr(&self, key: &str) -> CacheResult<i64> {
        let mut cmd = Cmd::new();
        cmd.arg("INCR").arg(key);

        let value = self.execute(cmd).await?;
        match value {
            Value::Int(n) => Ok(n),
            _ => Ok(0),
        }
    }

    /// Set a value without TTL
    pub async fn set(&self, key: &str, value: &[u8]) -> CacheResult<()> {
        let mut cmd = Cmd::new();
        cmd.arg("SET").arg(key).arg(value);

        self.execute(cmd).await?;
        Ok(())
    }

    /// Set a value with TTL
    pub async fn setex(&self, key: &str, ttl_secs: u64, value: &[u8]) -> CacheResult<()> {
        let mut cmd = Cmd::new();
        cmd.arg("SETEX").arg(key).arg(ttl_secs).arg(value);

        self.execute(cmd).await?;
        Ok(())
    }

    /// Delete a key, returns true if key existed and was deleted
    pub async fn del(&self, key: &str) -> CacheResult<bool> {
        let mut cmd = Cmd::new();
        cmd.arg("DEL").arg(key);

        let value = self.execute(cmd).await?;
        match value {
            Value::Int(n) => Ok(n > 0),
            _ => Ok(false),
        }
    }

    /// Add member to set, returns number of elements added
    pub async fn sadd(&self, key: &str, member: impl AsRef<[u8]>) -> CacheResult<i64> {
        let mut cmd = Cmd::new();
        cmd.arg("SADD").arg(key).arg(member.as_ref());

        let value = self.execute(cmd).await?;
        match value {
            Value::Int(n) => Ok(n),
            _ => Ok(0),
        }
    }

    /// Remove member from set, returns number of elements removed
    pub async fn srem(&self, key: &str, member: impl AsRef<[u8]>) -> CacheResult<i64> {
        let mut cmd = Cmd::new();
        cmd.arg("SREM").arg(key).arg(member.as_ref());

        let value = self.execute(cmd).await?;
        match value {
            Value::Int(n) => Ok(n),
            _ => Ok(0),
        }
    }

    /// Push to list (left), returns list length
    pub async fn lpush(&self, key: &str, value: impl AsRef<[u8]>) -> CacheResult<i64> {
        let mut cmd = Cmd::new();
        cmd.arg("LPUSH").arg(key).arg(value.as_ref());

        let res = self.execute(cmd).await?;
        match res {
            Value::Int(n) => Ok(n),
            _ => Ok(0),
        }
    }

    /// Execute a pipeline and return results
    pub async fn execute_pipeline(&self, pipeline: &Pipeline) -> CacheResult<Vec<Value>> {
        let mut conn = self.inner.lock().await;
        let results: Vec<Value> = pipeline
            .clone()
            .query_async(&mut *conn)
            .await
            .map_err(|e| CacheError::OperationError(e.to_string()))?;
        Ok(results)
    }

    /// Get sorted set range with scores
    pub async fn zrange_withscores(
        &self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> CacheResult<Vec<(String, f64)>> {
        let mut cmd = Cmd::new();
        cmd.arg("ZRANGE")
            .arg(key)
            .arg(start)
            .arg(stop)
            .arg("WITHSCORES");

        let value = self.execute(cmd).await?;
        match value {
            Value::Array(items) => {
                let mut results = Vec::new();
                let mut iter = items.chunks_exact(2);
                for chunk in &mut iter {
                    if let (Value::BulkString(member), Value::BulkString(score_str)) =
                        (&chunk[0], &chunk[1])
                    {
                        let member = String::from_utf8_lossy(member).to_string();
                        let score = String::from_utf8_lossy(score_str).parse().unwrap_or(0.0);
                        results.push((member, score));
                    }
                }
                Ok(results)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Get all members of a set
    pub async fn smembers(&self, key: &str) -> CacheResult<Vec<String>> {
        let mut cmd = Cmd::new();
        cmd.arg("SMEMBERS").arg(key);

        let value = self.execute(cmd).await?;
        match value {
            Value::Array(items) => {
                let mut results = Vec::new();
                for item in items {
                    if let Value::BulkString(data) = item {
                        results.push(String::from_utf8_lossy(&data).to_string());
                    }
                }
                Ok(results)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Get keys matching a pattern
    pub async fn keys(&self, pattern: &str) -> CacheResult<Vec<String>> {
        let mut cmd = Cmd::new();
        cmd.arg("KEYS").arg(pattern);

        let value = self.execute(cmd).await?;
        match value {
            Value::Array(items) => {
                let mut results = Vec::new();
                for item in items {
                    if let Value::BulkString(data) = item {
                        results.push(String::from_utf8_lossy(&data).to_string());
                    }
                }
                Ok(results)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Publish a message to a channel
    pub async fn publish(&self, channel: &str, message: impl AsRef<[u8]>) -> CacheResult<i64> {
        let mut cmd = Cmd::new();
        cmd.arg("PUBLISH").arg(channel).arg(message.as_ref());

        let value = self.execute(cmd).await?;
        match value {
            Value::Int(n) => Ok(n),
            _ => Ok(0),
        }
    }
}
