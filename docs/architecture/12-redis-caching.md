# Redis Caching and Data Layer

## Overview

R Commerce uses Redis as a high-performance caching and data layer for real-time operations, session management, rate limiting, and background job processing. This document covers the architecture, configuration, and usage patterns.

## Table of Contents

- [Architecture](#architecture)
- [Configuration](#configuration)
- [Connection Management](#connection-management)
- [Cache Modules](#cache-modules)
- [Job Queue](#job-queue)
- [Testing](#testing)
- [Monitoring](#monitoring)
- [Best Practices](#best-practices)

## Architecture

### Redis Usage Areas

```
┌─────────────────────────────────────────────────────────────┐
│                     R Commerce Platform                      │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   WebSocket │  │  Rate Limit │  │   Token Blacklist   │  │
│  │   Sessions  │  │   Storage   │  │                     │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                    │             │
│  ┌──────┴────────────────┴────────────────────┴──────────┐  │
│  │                    Redis Layer                        │  │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐  │  │
│  │  │ Pub/Sub │  │  Lists  │  │  Sets   │  │ Strings │  │  │
│  │  └─────────┘  └─────────┘  └─────────┘  └─────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
│                           │                                  │
│                    ┌──────┴──────┐                          │
│                    │   Redis     │                          │
│                    │   Server    │                          │
│                    │  (Single/   │                          │
│                    │   Cluster)  │                          │
│                    └─────────────┘                          │
└─────────────────────────────────────────────────────────────┘
```

### Key Features

| Feature | Purpose | Data Structure |
|---------|---------|----------------|
| WebSocket Sessions | Real-time connection state | Hash + TTL |
| Rate Limiting | API throttling counters | String (INCR) + TTL |
| Token Blacklist | Revoked JWT tokens | Set |
| Job Queue | Background task processing | List (LPUSH/RPOP) |
| Pub/Sub | Real-time broadcasts | Redis Pub/Sub |
| API Cache | Response caching | String |

## Configuration

### Basic Configuration

```toml
[cache]
enabled = true
cache_type = "Redis"  # or "Memory" for in-memory cache

[cache.redis]
host = "127.0.0.1"
port = 6379
database = 0
username = ""           # Optional
password = ""           # Optional
use_tls = false         # Enable for production

# Connection pool settings
[cache.redis.pool]
max_connections = 20
min_connections = 5
connection_timeout = 5  # seconds
idle_timeout = 300      # seconds

# Time-to-live settings
[cache.redis.ttl]
session = 3600          # 1 hour
rate_limit = 60         # 1 minute
api_cache = 300         # 5 minutes
token_blacklist = 86400 # 24 hours
```

### Environment-Specific Configurations

**Development:**
```toml
[cache.redis]
host = "127.0.0.1"
port = 6379
use_tls = false
```

**Production:**
```toml
[cache.redis]
host = "redis.internal"
port = 6380
use_tls = true
username = "rcommerce"
password = "${REDIS_PASSWORD}"

[cache.redis.pool]
max_connections = 50
min_connections = 10
```

### Configuration Code

```rust
use rcommerce_core::cache::{CacheConfig, RedisConfig};

// Load from config file
let config = CacheConfig::load("config.toml")?;

// Or create programmatically
let redis_config = RedisConfig {
    host: "127.0.0.1".to_string(),
    port: 6379,
    database: 0,
    use_tls: false,
    max_connections: 20,
    connection_timeout: Duration::from_secs(5),
    ..Default::default()
};
```

## Connection Management

### Connection Pool

The `RedisPool` provides efficient connection management:

```rust
use rcommerce_core::cache::RedisPool;

// Create pool
let pool = RedisPool::new(config).await?;

// Get connection from pool
let conn = pool.get().await?;

// Connection automatically returned to pool when dropped
```

### Pool Configuration Guidelines

| Environment | Max Connections | Min Connections | Timeout |
|-------------|-----------------|-----------------|---------|
| Development | 10 | 2 | 5s |
| Staging | 20 | 5 | 10s |
| Production | 50+ | 10 | 30s |

### Connection States

```rust
use rcommerce_core::cache::ConnectionState;

match pool.state() {
    ConnectionState::Connected => println!("Redis ready"),
    ConnectionState::Reconnecting => println!("Reconnecting..."),
    ConnectionState::Failed => println!("Connection failed!"),
    ConnectionState::Exhausted => println!("Pool exhausted!"),
}
```

## Cache Modules

### 1. WebSocket Session Store

Manages real-time connection state:

```rust
use rcommerce_core::cache::{SessionStore, WebSocketSession};

let store = SessionStore::new(pool);

// Store session
let session = WebSocketSession {
    id: session_id,
    user_id: Some(user_id),
    connected_at: Utc::now(),
    last_activity: Utc::now(),
    subscriptions: vec!["orders".to_string()],
};
store.save_session(&session).await?;

// Retrieve session
let session = store.get_session(session_id).await?;

// Update activity
store.touch_session(session_id).await?;

// Remove on disconnect
store.remove_session(session_id).await?;
```

**Key Pattern:** `ws:session:{session_id}`

### 2. Rate Limiter

Distributed rate limiting for API protection:

```rust
use rcommerce_core::cache::RedisRateLimiter;

let limiter = RedisRateLimiter::new(pool);

// Check rate limit
let key = format!("ip:{}", client_ip);
let result = limiter.check_rate_limit(&key, 100, Duration::from_secs(60)).await?;

if result.allowed {
    // Process request
} else {
    // Return 429 Too Many Requests
    println!("Retry after: {:?}", result.retry_after);
}

// Block abusive clients
limiter.block(&key, Duration::from_secs(3600)).await?;

// Unblock
limiter.unblock(&key).await?;
```

**Key Pattern:** `rate:limit:{identifier}`

### 3. Token Blacklist

JWT token revocation:

```rust
use rcommerce_core::cache::TokenBlacklist;

let blacklist = TokenBlacklist::new(pool);

// Blacklist token on logout
blacklist.blacklist(token_id, expires_at).await?;

// Check if token is valid
if blacklist.is_blacklisted(token_id).await? {
    return Err(Error::Unauthorized);
}

// Cleanup expired tokens
blacklist.cleanup().await?;
```

**Key Pattern:** `token:blacklist:{token_id}`

### 4. Pub/Sub

Real-time message broadcasting:

```rust
use rcommerce_core::cache::RedisPubSub;

let pubsub = RedisPubSub::new(pool);

// Subscribe to channel
let mut subscription = pubsub.subscribe("order_updates").await?;

// Receive messages
while let Some(msg) = subscription.recv().await {
    println!("Received: {:?}", msg);
}

// Publish message
pubsub.publish("order_updates", &order_update).await?;

// Unsubscribe
subscription.unsubscribe().await?;
```

### 5. Key Namespacing

Prevent key collisions with namespaces:

```rust
use rcommerce_core::cache::CacheNamespace;

// Generate prefixed keys
let session_key = CacheNamespace::WebSocketSession.key("conn:123");
// Result: "ws:session:conn:123"

let rate_key = CacheNamespace::RateLimit.key("ip:192.168.1.1");
// Result: "rate:limit:ip:192.168.1.1"
```

**Available Namespaces:**
- `WebSocketSession` → `ws:session`
- `RateLimit` → `rate:limit`
- `TokenBlacklist` → `token:blacklist`
- `MessageQueue` → `msg:queue`
- `ApiResponse` → `api:cache`
- `Session` → `session`

## Job Queue

Redis-backed job queue for background processing:

```rust
use rcommerce_core::jobs::{JobQueue, Job, JobPriority};

let queue = JobQueue::new(pool, "default");

// Create job
let job = Job::new("process_order", json!({"order_id": "123"}), "default")
    .with_priority(JobPriority::High);

// Enqueue
queue.enqueue(&job).await?;

// Dequeue (blocking with timeout)
while let Some(job) = queue.dequeue(30).await? {
    // Process job
    process_job(&job).await?;
    
    // Mark complete
    job.mark_completed();
    queue.update_job(&job).await?;
}
```

### Priority Queues

Jobs are processed by priority:

```rust
pub enum JobPriority {
    High,   // Processed first
    Normal, // Default
    Low,    // Processed last
}
```

**Implementation:** Separate Redis lists per priority:
- `jobs:queue:{name}/queue:high`
- `jobs:queue:{name}/queue:normal`
- `jobs:queue:{name}/queue:low`

### Scheduled Jobs

Delay job execution:

```rust
let job = Job::new("send_email", payload, "default")
    .schedule_for(Utc::now() + Duration::from_secs(3600));

queue.enqueue(&job).await?;
```

**Implementation:** Redis Sorted Set with timestamp as score

## Testing

### Running Redis Tests

```bash
# Ensure Redis is running
redis-cli ping

# Run all cache tests
cargo test -p rcommerce-core --lib -- cache

# Run specific module tests
cargo test -p rcommerce-core --lib cache::rate_limit
cargo test -p rcommerce-core --lib cache::session

# Run job queue tests
cargo test -p rcommerce-core --lib jobs::queue
```

### Test Configuration

Tests use a test-specific Redis database (default: db 0) with unique keys:

```rust
#[tokio::test]
async fn test_queue_operations() {
    let config = RedisConfig::default();
    let pool = RedisPool::new(config).await.unwrap();
    
    // Use unique queue name per test
    let queue_name = format!("test_queue_{}", Uuid::new_v4());
    let queue = JobQueue::new(pool, &queue_name);
    
    // Test operations...
    
    // Cleanup
    queue.clear().await.ok();
}
```

### CI/CD Testing

For CI environments without Redis:

```yaml
# .github/workflows/test.yml
services:
  redis:
    image: redis:7-alpine
    ports:
      - 6379:6379
```

## Monitoring

### Health Checks

```rust
// Check Redis connectivity
pub async fn health_check(pool: &RedisPool) -> Result<HealthStatus> {
    match pool.get().await {
        Ok(conn) => {
            conn.ping().await?;
            Ok(HealthStatus::Healthy)
        }
        Err(e) => Ok(HealthStatus::Unhealthy(e.to_string())),
    }
}
```

### Metrics to Track

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| Connection Pool Usage | Active connections | >80% |
| Command Latency | Redis response time | >10ms |
| Error Rate | Failed operations | >1% |
| Memory Usage | Redis memory consumption | >80% |
| Hit Rate | Cache effectiveness | <90% |

### Redis INFO Commands

```bash
# Connection info
redis-cli INFO clients

# Memory usage
redis-cli INFO memory

# Key statistics
redis-cli INFO keyspace

# Slow queries
redis-cli SLOWLOG GET 10
```

## Best Practices

### 1. Key Naming

- Use consistent prefixes: `{namespace}:{entity}:{id}`
- Keep keys under 100 characters
- Avoid special characters except `:` and `_`

### 2. TTL Management

Always set appropriate TTLs:

```rust
// Session data - 1 hour
conn.setex(&key, 3600, &data).await?;

// Rate limit counters - 1 minute
conn.setex(&key, 60, &count).await?;

// API cache - 5 minutes
conn.setex(&key, 300, &response).await?;
```

### 3. Error Handling

```rust
match operation.await {
    Ok(result) => result,
    Err(CacheError::ConnectionError(e)) => {
        // Log and potentially fail open
        tracing::error!("Redis connection error: {}", e);
        fallback_operation().await
    }
    Err(CacheError::Timeout) => {
        tracing::warn!("Redis timeout");
        // Retry with backoff
        retry_with_backoff(operation).await
    }
    Err(e) => return Err(e.into()),
}
```

### 4. Batch Operations

Use pipelines for bulk operations:

```rust
let mut pipeline = redis::Pipeline::new();

for key in keys {
    pipeline.cmd("GET").arg(key);
}

let results: Vec<Value> = conn.execute_pipeline(&pipeline).await?;
```

### 5. Security

- Enable TLS in production
- Use Redis AUTH password
- Restrict network access
- Rotate credentials regularly
- Encrypt sensitive data before storing

### 6. Memory Management

```bash
# Set maxmemory policy in redis.conf
maxmemory 1gb
maxmemory-policy allkeys-lru
```

Recommended policies:
- `allkeys-lru`: Evict least recently used (good for caching)
- `volatile-ttl`: Evict keys with shortest TTL (good for sessions)
- `noeviction`: Return errors when full (good for queues)

## Troubleshooting

### Connection Issues

```bash
# Check if Redis is running
redis-cli ping

# Test connection pool size
redis-cli INFO clients | grep connected_clients

# Check for blocked clients
redis-cli INFO clients | grep blocked_clients
```

### Memory Issues

```bash
# Find largest keys
redis-cli --bigkeys

# Check memory per key
redis-cli MEMORY USAGE key_name
```

### Performance Issues

```bash
# Enable slow log
redis-cli CONFIG SET slowlog-log-slower-than 10000

# Check command stats
redis-cli INFO commandstats
```

## Migration Guide

### From Memory Cache to Redis

1. Update configuration:
```toml
[cache]
cache_type = "Redis"  # Changed from "Memory"
```

2. No code changes needed - same interface

### Redis Version Requirements

- Minimum: Redis 6.0
- Recommended: Redis 7.0+

### Cluster Mode

For high availability, configure Redis Cluster:

```toml
[cache.redis.cluster]
enabled = true
nodes = [
    "redis://node1:6379",
    "redis://node2:6379",
    "redis://node3:6379",
]
```

---

*Last updated: January 2026*
