# Redis Setup and Operations Guide

## Quick Start

### macOS

```bash
# Install via Homebrew
brew install redis

# Start Redis
brew services start redis

# Or run manually
redis-server

# Verify
redis-cli ping
```

### Linux (Ubuntu/Debian)

```bash
# Install
sudo apt-get update
sudo apt-get install redis-server

# Start
sudo systemctl start redis-server
sudo systemctl enable redis-server

# Verify
redis-cli ping
```

### Docker

```bash
# Run Redis container
docker run -d --name redis \
  -p 6379:6379 \
  -v redis-data:/data \
  redis:7-alpine \
  redis-server --appendonly yes

# Verify
docker exec -it redis redis-cli ping
```

### Docker Compose

```yaml
version: '3.8'
services:
  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    volumes:
      - redis-data:/data
    command: redis-server --appendonly yes --maxmemory 512mb --maxmemory-policy allkeys-lru
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 3s
      retries: 5

volumes:
  redis-data:
```

## Production Configuration

### redis.conf Essentials

```conf
# Network
bind 127.0.0.1 ::1
protected-mode yes
port 6379

# Memory
maxmemory 2gb
maxmemory-policy allkeys-lru

# Persistence
save 900 1
save 300 10
save 60 10000
appendonly yes
appendfsync everysec

# Security
requirepass your-strong-password

# Performance
tcp-keepalive 300
timeout 0
```

### R Commerce Specific Configuration

```conf
# Key eviction for session/cache data
maxmemory-policy allkeys-lru

# Disable persistence for job queues (use replication)
# Note: Only if using replicated setup

# Enable keyspace notifications for real-time features
notify-keyspace-events Ex
```

## Environment-Specific Setups

### Development

Minimal setup, data persistence optional:

```bash
redis-server --port 6379 --maxmemory 256mb
```

### Staging

Match production structure with less resources:

```bash
redis-server --port 6379 \
  --maxmemory 512mb \
  --maxmemory-policy allkeys-lru \
  --appendonly yes \
  --requirepass staging-password
```

### Production

High availability with Redis Sentinel:

```yaml
# docker-compose.prod.yml
version: '3.8'
services:
  redis-master:
    image: redis:7-alpine
    volumes:
      - redis-master:/data
    command: redis-server --appendonly yes --maxmemory 2gb

  redis-replica:
    image: redis:7-alpine
    volumes:
      - redis-replica:/data
    command: redis-server --replicaof redis-master 6379

  redis-sentinel:
    image: redis:7-alpine
    command: redis-sentinel /etc/redis/sentinel.conf
    volumes:
      - ./sentinel.conf:/etc/redis/sentinel.conf

volumes:
  redis-master:
  redis-replica:
```

## Monitoring Commands

### Basic Health Check

```bash
# Connectivity
redis-cli ping

# Server info
redis-cli INFO server

# Memory usage
redis-cli INFO memory | grep used_memory_human

# Connected clients
redis-cli INFO clients | grep connected_clients
```

### Performance Monitoring

```bash
# Slow queries
redis-cli SLOWLOG GET 10

# Command statistics
redis-cli INFO commandstats

# Keyspace statistics
redis-cli INFO keyspace

# Replication status
redis-cli INFO replication
```

### R Commerce Specific Checks

```bash
# Count WebSocket sessions
redis-cli KEYS "ws:session:*" | wc -l

# Count rate limit entries
redis-cli KEYS "rate:limit:*" | wc -l

# Queue depth
redis-cli LLEN "jobs:queue:default/queue:high"
redis-cli LLEN "jobs:queue:default/queue:normal"
redis-cli LLEN "jobs:queue:default/queue:low"

# Scheduled jobs count
redis-cli ZCARD "jobs:queue:default/scheduled"
```

## Backup and Restore

### Backup

```bash
# RDB backup
redis-cli BGSAVE
ls -la /var/lib/redis/dump.rdb

# Copy backup
cp /var/lib/redis/dump.rdb /backups/redis-$(date +%Y%m%d).rdb

# AOF backup
cp /var/lib/redis/appendonly.aof /backups/redis-aof-$(date +%Y%m%d).aof
```

### Restore

```bash
# Stop Redis
sudo systemctl stop redis-server

# Restore RDB
cp /backups/redis-20260127.rdb /var/lib/redis/dump.rdb

# Start Redis
sudo systemctl start redis-server
```

## Maintenance Tasks

### Clear Test Data

```bash
# Clear only test queues
redis-cli --scan --pattern "jobs:queue:test_*" | xargs redis-cli DEL

# Clear all R Commerce data (DANGEROUS)
redis-cli --scan --pattern "*" | xargs redis-cli DEL
```

### Memory Optimization

```bash
# Find large keys
redis-cli --bigkeys

# Check memory usage of specific pattern
for key in $(redis-cli --scan --pattern "ws:session:*"); do
    size=$(redis-cli MEMORY USAGE "$key")
    echo "$size $key"
done | sort -nr | head -20

# Compact AOF
redis-cli BGREWRITEAOF
```

### Upgrade Redis

1. Backup data
2. Stop application
3. Upgrade Redis
4. Verify with `redis-cli INFO server`
5. Start application

## Troubleshooting

### Connection Refused

```bash
# Check if Redis is running
sudo systemctl status redis-server

# Check port binding
netstat -tlnp | grep 6379

# Check logs
sudo tail -f /var/log/redis/redis-server.log
```

### High Memory Usage

```bash
# Set maxmemory if not set
redis-cli CONFIG SET maxmemory 2gb
redis-cli CONFIG SET maxmemory-policy allkeys-lru

# Find memory hogs
redis-cli --bigkeys

# Check eviction stats
redis-cli INFO stats | grep evicted_keys
```

### Slow Performance

```bash
# Enable slow log
redis-cli CONFIG SET slowlog-log-slower-than 10000
redis-cli SLOWLOG GET 10

# Check for blocked clients
redis-cli INFO clients | grep blocked_clients

# Monitor commands in real-time
redis-cli MONITOR
```

## Integration with R Commerce

### Configuration File

```toml
# config/production.toml
[cache]
enabled = true
cache_type = "Redis"

[cache.redis]
host = "127.0.0.1"
port = 6379
database = 0
password = "${REDIS_PASSWORD}"
use_tls = false

[cache.redis.pool]
max_connections = 50
min_connections = 10
connection_timeout = 30
idle_timeout = 600

[cache.redis.ttl]
session = 3600
rate_limit = 60
api_cache = 300
token_blacklist = 86400
```

### Environment Variables

```bash
export REDIS_URL=redis://:password@localhost:6379/0
export REDIS_PASSWORD=your-secure-password
export REDIS_POOL_SIZE=50
```

### Health Check Endpoint

The application exposes a health check that includes Redis status:

```bash
curl http://localhost:8080/health

# Response
{
  "status": "healthy",
  "components": {
    "redis": {
      "status": "connected",
      "pool_size": 50,
      "active_connections": 5
    }
  }
}
```

## Security Checklist

- [ ] Enable `requirepass` or use Redis ACLs
- [ ] Bind to specific interfaces (not 0.0.0.0)
- [ ] Enable TLS for remote connections
- [ ] Configure firewall rules
- [ ] Regular security updates
- [ ] Audit logging enabled
- [ ] Backup encryption

---

*For issues or questions, check the main Redis documentation at https://redis.io/documentation*
