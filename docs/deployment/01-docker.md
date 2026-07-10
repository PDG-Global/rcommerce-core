# Docker Deployment Guide

This guide covers deploying R commerce using Docker and Docker Compose for production environments.

## Quick Start

```bash
# Clone the repository
git clone https://github.com/creativebastard/rcommerce.git
cd gocart

# Create environment file
cp .env.example .env
# Edit .env with your configuration

# Start with Docker Compose
docker-compose up -d

# Check logs
docker-compose logs -f rcommerce

# Verify health
curl http://localhost:8080/health
```

## Prerequisites

- Docker 20.10+
- Docker Compose 2.0+
- 2GB available RAM
- 1GB available disk space

## Docker Image

### Multi-Stage Dockerfile

```dockerfile
# Build stage
FROM rust:1.75-slim as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifest and build dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release --bin rcommerce || true

# Copy source code
COPY . .

# Build application
RUN cargo build --release --bin rcommerce

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \

    curl \
    && rm -rf /var/lib/apt/lists/*

# Create service user
RUN groupadd -r rcommerce && useradd -r -g rcommerce rcommerce

# Copy binary from builder
COPY --from=builder /app/target/release/rcommerce /usr/local/bin/
COPY --from=builder /app/config/default.toml /etc/rcommerce/config.toml

# Create directories
RUN mkdir -p /var/lib/rcommerce /var/log/rcommerce && \
    chown -R rcommerce:rcommerce /var/lib/rcommerce /var/log/rcommerce

# Switch to service user
USER rcommerce

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1

# Start application
CMD ["rcommerce"]
```

**Build the image:**

```bash
# Build image
docker build -t rcommerce:latest .

# Build with specific Rust version
docker build --build-arg RUST_VERSION=1.75 -t rcommerce:latest .

# Build for specific platform
docker build --platform linux/amd64 -t rcommerce:amd64 .
docker build --platform linux/arm64 -t rcommerce:arm64 .
```

## Docker Compose Configuration

### Production Configuration

```yaml
# docker-compose.yml
version: '3.8'

services:
  # Main application
  rcommerce:
    image: rcommerce:latest
    container_name: rcommerce
    restart: unless-stopped
    
    ports:
      - "8080:8080"
    
    environment:
      - RCOMMERCE_CONFIG=/etc/rcommerce/config.toml
      - RUST_LOG=info
      - RUST_BACKTRACE=1
      - DATABASE_URL=postgres://rcommerce:${DB_PASSWORD}@postgres:5432/rcommerce
      - REDIS_URL=redis://redis:6379
    
    volumes:
      - ./config/production.toml:/etc/rcommerce/config.toml:ro
      - rcommerce_data:/var/lib/rcommerce
      - rcommerce_logs:/var/log/rcommerce
      - ./uploads:/app/uploads:rw
    
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy
    
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
    
    ulimits:
      nofile:
        soft: 65536
        hard: 65536
      nproc:
        soft: 4096
        hard: 4096
    
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 1G
        reservations:
          cpus: '0.5'
          memory: 512M
    
    logging:
      driver: "json-file"
      options:
        max-size: "100m"
        max-file: "3"
        tag: "{{.Name}}"
    
    networks:
      - rcommerce_network
    
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.rcommerce.rule=Host(`api.yourstore.com`)"
      - "traefik.http.routers.rcommerce.entrypoints=websecure"
      - "traefik.http.routers.rcommerce.tls.certresolver=letsencrypt"
      - "traefik.http.services.rcommerce.loadbalancer.server.port=8080"

  # PostgreSQL database
  postgres:
    image: postgres:15-alpine
    container_name: rcommerce_db
    restart: unless-stopped
    
    environment:
      - POSTGRES_DB=rcommerce
      - POSTGRES_USER=rcommerce
      - POSTGRES_PASSWORD=${DB_PASSWORD}
      - POSTGRES_INITDB_ARGS=--auth-host=md5
      - PGDATA=/var/lib/postgresql/data/pgdata
    
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./init.sql:/docker-entrypoint-initdb.d/init.sql:ro
      - ./config/postgresql.conf:/etc/postgresql/postgresql.conf:ro
    
    command: postgres -c config_file=/etc/postgresql/postgresql.conf
    
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U rcommerce"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 30s
    
    ports:
      - "5432:5432"
    
    ulimits:
      nofile:
        soft: 65536
        hard: 65536
    
    sysctls:
      - net.core.somaxconn=65535
    
    logging:
      driver: "json-file"
      options:
        max-size: "50m"
        max-file: "2"
    
    networks:
      - rcommerce_network

  # Redis cache
  redis:
    image: redis:7-alpine
    container_name: rcommerce_cache
    restart: unless-stopped
    
    command: >
      redis-server
      --appendonly yes
      --maxmemory 256mb
      --maxmemory-policy allkeys-lru
      --maxmemory-samples 5
      --tcp-keepalive 300
      --timeout 0
      --save 900 1
      --save 300 10
      --save 60 10000
    
    volumes:
      - redis_data:/data
      - ./config/redis.conf:/usr/local/etc/redis/redis.conf:ro
    
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 3s
      retries: 5
      start_period: 10s
    
    ports:
      - "6379:6379"
    
    sysctls:
      - net.core.somaxconn=65535
    
    logging:
      driver: "json-file"
      options:
        max-size: "20m"
        max-file: "2"
    
    networks:
      - rcommerce_network

  # Nginx reverse proxy (optional)
  nginx:
    image: nginx:alpine
    container_name: rcommerce_proxy
    restart: unless-stopped
    
    ports:
      - "80:80"
      - "443:443"
    
    volumes:
      - ./config/nginx.conf:/etc/nginx/nginx.conf:ro
      - ./config/nginx/rcommerce.conf:/etc/nginx/conf.d/rcommerce.conf:ro
      - ./ssl:/etc/nginx/ssl:ro
      - nginx_cache:/var/cache/nginx
    
    depends_on:
      - rcommerce
    
    healthcheck:
      test: ["CMD", "wget", "--quiet", "--tries=1", "--spider", "http://localhost/health"]
      interval: 30s
      timeout: 10s
      retries: 3
    
    logging:
      driver: "json-file"
      options:
        max-size: "100m"
        max-file: "3"
    
    networks:
      - rcommerce_network
    
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.nginx.rule=Host(`api.yourstore.com`)"

  # Traefik reverse proxy (alternative to nginx)
  traefik:
    image: traefik:v3.0
    container_name: rcommerce_traefik
    restart: unless-stopped
    
    command:
      - "--api.insecure=false"
      - "--api.dashboard=true"
      - "--providers.docker=true"
      - "--providers.docker.exposedbydefault=false"
      - "--entrypoints.web.address=:80"
      - "--entrypoints.websecure.address=:443"
      - "--certificatesresolvers.letsencrypt.acme.tlschallenge=true"
      - "--certificatesresolvers.letsencrypt.acme.email=admin@yourstore.com"
      - "--certificatesresolvers.letsencrypt.acme.storage=/letsencrypt/acme.json"
      - "--log.level=INFO"
      - "--accesslog=true"
      - "--accesslog.filters.statuscodes=400-599"
    
    ports:
      - "80:80"
      - "443:443"
    
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - traefik_certs:/letsencrypt
      - ./config/traefik.yml:/etc/traefik/traefik.yml:ro
    
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.traefik.rule=Host(`monitor.yourstore.com`)"
      - "traefik.http.routers.traefik.entrypoints=websecure"
      - "traefik.http.routers.traefik.tls.certresolver=letsencrypt"
      - "traefik.http.routers.traefik.service=api@internal"
      - "traefik.http.middlewares.traefik-auth.basicauth.users=admin:$$apr1$$H6uskkkW$$IgXLP6ewTrSuBkTrqE8wj"
      - "traefik.http.routers.traefik.middlewares=traefik-auth"
    
    networks:
      - rcommerce_network

  # Log aggregation (optional)
  fluentd:
    image: fluent/fluentd:v1.16-debian-1
    container_name: rcommerce_fluentd
    restart: unless-stopped
    
    volumes:
      - ./config/fluentd:/fluentd/etc:ro
      - fluentd_logs:/var/log/fluentd
    
    ports:
      - "24224:24224"
    
    networks:
      - rcommerce_network

  # Monitoring (optional)
  prometheus:
    image: prom/prometheus:latest
    container_name: rcommerce_prometheus
    restart: unless-stopped
    
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--web.console.libraries=/etc/prometheus/console_libraries'
      - '--web.console.templates=/etc/prometheus/consoles'
      - '--storage.tsdb.retention.time=200h'
      - '--web.enable-lifecycle'
    
    volumes:
      - ./config/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus_data:/prometheus
    
    ports:
      - "9090:9090"
    
    networks:
      - rcommerce_network

  grafana:
    image: grafana/grafana:latest
    container_name: rcommerce_grafana
    restart: unless-stopped
    
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_PASSWORD}
    
    volumes:
      - grafana_data:/var/lib/grafana
      - ./config/grafana/dashboards:/etc/grafana/provisioning/dashboards:ro
      - ./config/grafana/datasources:/etc/grafana/provisioning/datasources:ro
    
    ports:
      - "3000:3000"
    
    networks:
      - rcommerce_network

volumes:
  postgres_data:
    driver: local
  redis_data:
    driver: local
  rcommerce_data:
    driver: local
  rcommerce_logs:
    driver: local
  nginx_cache:
    driver: local
  traefik_certs:
    driver: local
  prometheus_data:
    driver: local
  grafana_data:
    driver: local
  fluentd_logs:
    driver: local

networks:
  rcommerce_network:
    driver: bridge
    ipam:
      config:
        - subnet: 172.20.0.0/16
```

### Environment Variables

Create `.env` file:

```bash
# Database
DB_PASSWORD=super_secure_password_here_change_me

# Redis (optional)
REDIS_PASSWORD=another_secure_password

# Application secrets
JWT_SECRET=your_jwt_secret_key_here_min_32_chars
API_KEY_SECRET=your_api_key_secret_here

# External services
STRIPE_SECRET_KEY=sk_live_xxx
STRIPE_WEBHOOK_SECRET=whsec_xxx
SHIPSTATION_API_KEY=your_shipstation_key
SHIPSTATION_API_SECRET=your_shipstation_secret
SENDGRID_API_KEY=SG.xxx

# Monitoring
GRAFANA_PASSWORD=secure_grafana_password

# Application
RUST_LOG=info
RUST_BACKTRACE=1
```

## Database Configuration

### PostgreSQL Optimization

```conf
# config/postgresql.conf
listen_addresses = '*'
port = 5432
max_connections = 200
shared_buffers = 256MB
effective_cache_size = 768MB
maintenance_work_mem = 64MB
checkpoint_completion_target = 0.9
wal_buffers = 7864kB
default_statistics_target = 100
random_page_cost = 1.1
effective_io_concurrency = 200
work_mem = 1310kB
min_wal_size = 1GB
max_wal_size = 4GB
```

### Redis Configuration

```conf
# config/redis.conf
bind 0.0.0.0
port 6379
tcp-keepalive 300
timeout 0
databases 1
save 900 1
save 300 10
save 60 10000
stop-writes-on-bgsave-error yes
rdbcompression yes
rdbchecksum yes
dbfilename dump.rdb
dir /data
maxmemory 256mb
maxmemory-policy allkeys-lru
maxmemory-samples 5
appendonly yes
appendfilename "appendonly.aof"
appendfsync everysec
no-appendfsync-on-rewrite no
auto-aof-rewrite-percentage 100
auto-aof-rewrite-min-size 64mb
lua-time-limit 5000
slowlog-log-slower-than 10000
slowlog-max-len 128
latency-monitor-threshold 0
notify-keyspace-events ""
hash-max-ziplist-entries 512
hash-max-ziplist-value 64
list-max-ziplist-size -2
list-compress-depth 0
set-max-intset-entries 512
zset-max-ziplist-entries 128
zset-max-ziplist-value 64
hll-sparse-max-bytes 3000
activerehashing yes
client-output-buffer-limit normal 0 0 0
client-output-buffer-limit replica 256mb 64mb 60
client-output-buffer-limit pubsub 32mb 8mb 60
hz 10
dynamic-hz yes
aof-rewrite-incremental-fsync yes
rdb-save-incremental-fsync yes
```

## Production Best Practices

### 1. Security

```yaml
# docker-compose.security.yml
version: '3.8'

services:
  rcommerce:
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    cap_add:
      - CHOWN
      - SETGID
      - SETUID
    read_only: true
    tmpfs:
      - /tmp
      - /var/tmp
```

### 2. Resource Limits

```yaml
# docker-compose.resources.yml
version: '3.8'

services:
  rcommerce:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 1G
        reservations:
          cpus: '0.5'
          memory: 512M
    environment:
      - RUST_LOG=warn  # Reduce logging in production
```

### 3. Monitoring

```yaml
# docker-compose.monitoring.yml
version: '3.8'

services:
  rcommerce:
    logging:
      driver: "fluentd"
      options:
        fluentd-address: localhost:24224
        tag: "rcommerce.{{.Name}}"
```

### 4. Scaling

```yaml
# docker-compose.scale.yml
version: '3.8'

services:
  rcommerce:
    deploy:
      replicas: 3
      update_config:
        parallelism: 1
        delay: 10s
        failure_action: rollback
        order: stop-first
      rollback_config:
        parallelism: 1
        delay: 5s
        order: stop-first
      restart_policy:
        condition: on-failure
        delay: 5s
        max_attempts: 3
        window: 120s
        order: stop-first
```

## Docker Commands

```bash
# Build and start
docker-compose up -d

# View logs
docker-compose logs -f rcommerce
docker-compose logs -f --tail=100 rcommerce

# Scale application
docker-compose up -d --scale rcommerce=3

# Restart service
docker-compose restart rcommerce

# Update service
docker-compose up -d --no-deps --build rcommerce

# Run database migrations
docker-compose exec rcommerce rcommerce migrate run

# Access database
docker-compose exec postgres psql -U rcommerce -d rcommerce

# Access Redis
docker-compose exec redis redis-cli

# Backup database
docker-compose exec postgres pg_dump -U rcommerce rcommerce > backup.sql

# Restore database
cat backup.sql | docker-compose exec -T postgres psql -U rcommerce -d rcommerce

# View resource usage
docker stats

# Clean up
docker-compose down --volumes --remove-orphans
```

## Health Monitoring

### Application Health

```bash
# Health check endpoint
curl http://localhost:8080/health

# Expected response:
{
  "status": "healthy",
  "version": "0.1.0",
  "database": "connected",
  "cache": "connected",
  "timestamp": "2024-01-23T14:13:35Z"
}
```

### Container Health

```bash
# Check container status
docker ps

# Check logs for errors
docker-compose logs --tail=50 | grep ERROR

# Monitor resource usage
docker stats rcommerce

# Check container events
docker events --filter container=rcommerce
```

## Backup and Recovery

### Database Backup

```bash
#!/bin/bash
# backup.sh

echo "Starting backup..."

# Create backup directory
mkdir -p /backup/rcommerce

# Backup PostgreSQL
docker-compose exec -T postgres pg_dump -U rcommerce rcommerce | \
  gzip > /backup/rcommerce/database_$(date +%Y%m%d_%H%M%S).sql.gz

# Backup uploaded files
tar -czf /backup/rcommerce/uploads_$(date +%Y%m%d_%H%M%S).tar.gz \
  -C /var/lib/rcommerce/uploads .

# Backup configuration
cp /opt/rcommerce/config/production.toml \
  /backup/rcommerce/config_$(date +%Y%m%d_%H%M%S).toml

# Keep only last 7 days of backups
find /backup/rcommerce -type f -mtime +7 -delete

echo "Backup completed"
```

Add to crontab:
```bash
# Run backup daily at 2 AM
0 2 * * * /opt/rcommerce/backup.sh
```

### Automated Backup Service

```yaml
# docker-compose.backup.yml
version: '3.8'

services:
  backup:
    image: postgres:15-alpine
    container_name: rcommerce_backup
    restart: "no"
    
    environment:
      - PGPASSWORD=${DB_PASSWORD}
    
    volumes:
      - /backup/rcommerce:/backup
      - ./backup.sh:/backup.sh:ro
    
    command: >
      sh -c '
        echo "Starting backup at $$(date)" &&
        pg_dump -h postgres -U rcommerce rcommerce | \
          gzip > /backup/database_$$(date +%Y%m%d_%H%M%S).sql.gz &&
        echo "Backup completed" ||
        echo "Backup failed"
      '
    
    networks:
      - rcommerce_network
```

## Troubleshooting

### Container won't start

```bash
# Check logs
docker-compose logs rcommerce

# Check configuration
docker-compose exec rcommerce cat /etc/rcommerce/config.toml

# Check file permissions
docker-compose exec rcommerce ls -la /var/lib/rcommerce

# Test database connection
docker-compose exec rcommerce nc -zv postgres 5432
```

### Database connection issues

```bash
# Check PostgreSQL logs
docker-compose logs postgres

# Verify PostgreSQL is running
docker-compose exec postgres pg_isready -U rcommerce

# Check network connectivity
docker-compose exec rcommerce ping postgres

# Reset database (WARNING: deletes all data)
docker-compose down --volumes
docker-compose up -d postgres
docker-compose exec rcommerce rcommerce migrate run
```

### Performance issues

```bash
# Check resource usage
docker stats

# Check for slow queries
docker-compose exec postgres psql -U rcommerce -c "SELECT * FROM pg_stat_activity WHERE state = 'active';"

# Check Redis memory usage
docker-compose exec redis redis-cli info memory

# Check application logs for slow requests
docker-compose logs rcommerce | grep -E "(slow|timeout)"

# Restart with more resources
docker-compose up -d --force-recreate \
  --compatibility \
  --scale rcommerce=2
```

### Memory issues

```bash
# Check OOM kills
dmesg | grep -i oom

# Increase memory limits
docker-compose down
docker-compose -f docker-compose.yml -f docker-compose.memfix.yml up -d

# docker-compose.memfix.yml
version: '3.8'
services:
  rcommerce:
    mem_limit: 2G
    mem_reservation: 1G
```

### Network issues

```bash
# Check container networks
docker network ls
docker network inspect rcommerce_rcommerce_network

# Test connectivity
docker-compose exec rcommerce nc -zv postgres 5432
docker-compose exec rcommerce nc -zv redis 6379

# Restart network
docker-compose down
docker network prune -f
docker-compose up -d
```

## Security Hardening

### Use secrets for sensitive data

```yaml
# docker-compose.secrets.yml
version: '3.8'

secrets:
  db_password:
    file: ./secrets/db_password.txt
  stripe_key:
    file: ./secrets/stripe_key.txt
  jwt_secret:
    file: ./secrets/jwt_secret.txt

services:
  rcommerce:
    secrets:
      - db_password
      - stripe_key
      - jwt_secret
    environment:
      - DB_PASSWORD_FILE=/run/secrets/db_password
      - STRIPE_KEY_FILE=/run/secrets/stripe_key
      - JWT_SECRET_FILE=/run/secrets/jwt_secret
```

### Use non-root user

Already implemented in Dockerfile with:
```dockerfile
RUN groupadd -r rcommerce && useradd -r -g rcommerce rcommerce
USER rcommerce
```

### Enable AppArmor

```bash
# Create AppArmor profile
cp docker/apparmor/rcommerce /etc/apparmor.d/

# Load profile
apparmor_parser -r -W /etc/apparmor.d/rcommerce

# Apply in docker-compose.yml
security_opt:
  - apparmor:rcommerce
```

### Enable SELinux

```bash
# Build with SELinux contexts
docker build -t rcommerce:selinux .

# Run with SELinux
docker-compose -f docker-compose.yml -f docker-compose.selinux.yml up -d

# docker-compose.selinux.yml
version: '3.8'
services:
  rcommerce:
    security_opt:
      - label:user:rcommerce_t
      - label:role:rcommerce_r
      - label:type:rcommerce_t
      - label:level:s0
```

## Production Deployment Checklist

- [ ] Use specific image versions (not latest)
- [ ] Set resource limits (CPU, memory)
- [ ] Configure health checks
- [ ] Set up logging aggregation
- [ ] Configure monitoring and alerting
- [ ] Enable SSL/TLS certificates
- [ ] Set up backup strategy
- [ ] Configure firewall rules
- [ ] Use secrets for sensitive data
- [ ] Set up CI/CD pipeline
- [ ] Configure auto-scaling
- [ ] Set up reverse proxy
- [ ] Configure rate limiting
- [ ] Enable request tracing
- [ ] Set up error tracking
- [ ] Document deployment process
- [ ] Create runbooks
- [ ] Set up staging environment
- [ ] Perform load testing
- [ ] Create rollback plan

---

*Kubernetes deployment documentation coming soon.*
