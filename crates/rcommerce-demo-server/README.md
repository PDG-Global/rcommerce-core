# R Commerce Frontend Server

A **production-ready** Rust server for hosting customer frontends with R Commerce backend integration.

## Why This Instead of NodeJS?

| Feature | NodeJS + Express | R Commerce Frontend Server |
|---------|------------------|---------------------------|
| Binary Size | ~100MB+ with node_modules | ~6.5MB single binary |
| Memory Usage | 100-300MB | 10-30MB |
| Startup Time | 2-5 seconds | <100ms |
| Dependencies | 1000+ npm packages | 0 runtime dependencies |
| Security | Supply chain risks | Single Rust binary |
| Deployment | Complex | Single file + static assets |

## Features

### Security 🔒
- **API Key Protection**: Keys never exposed to browser (server-side injection)
- **Rate Limiting**: Per-IP rate limiting (configurable requests/minute)
- **Security Headers**: X-Content-Type-Options, X-Frame-Options, X-XSS-Protection
- **No CORS Required**: Serve frontend and API from same origin

### Performance 🚀
- **In-Memory Caching**: LRU cache for API responses (Redis-ready architecture)
- **Compression**: Brotli and Gzip compression for static files
- **HTTP/2 Ready**: Works behind Caddy/Nginx with HTTP/2
- **Connection Pooling**: Efficient backend connection reuse

### Production Ready ✅
- **Graceful Shutdown**: Handles SIGTERM/SIGINT properly
- **Health Checks**: `/health` endpoint for monitoring
- **Structured Logging**: JSON logging support
- **Request Timeouts**: Configurable timeouts to prevent hanging
- **Compression**: Automatic Brotli/Gzip for text assets

## Quick Start

### 1. Create API Key

```bash
# On your R Commerce server
rcommerce api-key create --name "Production Frontend" --scopes "products:read,orders:write,carts:write,customers:read"
# Save the key securely
```

### 2. Download Binary

```bash
# Download pre-built binary (or build from source)
wget https://github.com/creativebastard/rcommerce/releases/download/v0.1.0/rcommerce-demo-macos-arm64
chmod +x rcommerce-demo
```

### 3. Configure

Create `frontend-server.toml`:

```toml
# Server configuration
bind = "0.0.0.0:3000"
api_url = "http://your-rcommerce-server:8080"
api_key = "ak_yourprefix.yoursecret"
static_dir = "frontend"

# Production settings
cache_ttl_secs = 300          # Cache API responses for 5 minutes
rate_limit_per_minute = 60    # 60 requests per minute per IP
enable_compression = true     # Brotli/Gzip compression
enable_etag = true           # ETag support for static files
log_level = "info"
```

### 4. Run

```bash
./rcommerce-demo --config frontend-server.toml
```

## Customer Frontend Integration

Your customers' frontend should make API requests to relative URLs:

```javascript
// ✅ Good - goes through proxy, no API key needed
const products = await fetch('/api/v1/products').then(r => r.json());

// ❌ Bad - exposes API credentials
const products = await fetch('http://api.rcommerce.com/v1/products', {
  headers: { 'Authorization': 'Bearer ak_secret_key_here' }  // DON'T DO THIS!
});
```

Provide customers with a template frontend or let them build their own.

## Deployment

### With Caddy (Recommended)

```caddyfile
# Caddyfile
store.customerdomain.com {
    reverse_proxy localhost:3000
    
    # Automatic HTTPS
    tls your@email.com
    
    # Security headers
    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-Content-Type-Options "nosniff"
        X-Frame-Options "DENY"
        Referrer-Policy "strict-origin-when-cross-origin"
    }
}
```

### With Nginx

```nginx
server {
    listen 443 ssl http2;
    server_name store.customerdomain.com;
    
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    
    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # Timeouts
        proxy_connect_timeout 30s;
        proxy_send_timeout 30s;
        proxy_read_timeout 30s;
    }
}
```

### Systemd Service

```ini
# /etc/systemd/system/rcommerce-frontend.service
[Unit]
Description=R Commerce Frontend Server
After=network.target

[Service]
Type=simple
User=www-data
WorkingDirectory=/var/www/store
ExecStart=/usr/local/bin/rcommerce-demo --config /etc/rcommerce/frontend.toml
Restart=always
RestartSec=5

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/www/store

[Install]
WantedBy=multi-user.target
```

Enable:
```bash
sudo systemctl enable rcommerce-frontend
sudo systemctl start rcommerce-frontend
```

### Docker

```dockerfile
FROM scratch

# Copy static binary
COPY rcommerce-demo /rcommerce-demo

# Copy frontend files
COPY frontend /frontend

# Config via environment
ENV FRONTEND_BIND="0.0.0.0:3000"
ENV FRONTEND_STATIC_DIR="/frontend"

EXPOSE 3000

USER 1000:1000

ENTRYPOINT ["/rcommerce-demo"]
```

```bash
docker build -t rcommerce-frontend .
docker run -d \
  -p 3000:3000 \
  -e FRONTEND_API_URL="http://host.docker.internal:8080" \
  -e FRONTEND_API_KEY="your-api-key" \
  rcommerce-frontend
```

## Configuration

### Methods (in order of precedence)

1. **CLI flags** (highest priority)
2. **Environment variables**
3. **Config file**
4. **Defaults**

### All Options

| Option | CLI | Environment | Default | Description |
|--------|-----|-------------|---------|-------------|
| bind | `--bind` | `FRONTEND_BIND` | `0.0.0.0:3000` | Server bind address |
| api_url | `--api-url` | `FRONTEND_API_URL` | (required) | R Commerce API URL |
| api_key | `--api-key` | `FRONTEND_API_KEY` | (required) | Service API key |
| static_dir | `--static-dir` | `FRONTEND_STATIC_DIR` | `frontend` | Static files directory |
| cache_ttl_secs | - | - | `300` | API cache TTL (seconds) |
| rate_limit_per_minute | - | - | `60` | Requests per minute per IP |
| enable_compression | - | - | `true` | Enable Brotli/Gzip |
| cors | - | `FRONTEND_CORS` | `false` | Enable CORS (dev only!) |

### Example: Environment Variables

```bash
export FRONTEND_BIND="0.0.0.0:3000"
export FRONTEND_API_URL="http://rcommerce.internal:8080"
export FRONTEND_API_KEY="ak_production.yoursecret"
export FRONTEND_STATIC_DIR="/var/www/frontend"
export FRONTEND_CACHE_TTL_SECS="600"
export FRONTEND_RATE_LIMIT_PER_MINUTE="120"

./rcommerce-demo
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Customer Browser                      │
│  ┌─────────────────┐  ┌──────────────────────────────┐  │
│  │  Customer's     │  │  API Calls                   │  │
│  │  Frontend       │──▶│  fetch('/api/v1/products')   │  │
│  │  (HTML/JS/CSS)  │  │  (no API key in browser!)    │  │
│  └─────────────────┘  └──────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│              R Commerce Frontend Server                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Static Files │  │ API Proxy    │  │ Rate Limiter │  │
│  │ ServeDir     │  │ + Cache      │  │ per-IP       │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                           │                             │
│                    API Key Injection                    │
│                    (server-side only)                   │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   R Commerce API                         │
│              (your backend server)                       │
└─────────────────────────────────────────────────────────┘
```

## Monitoring

### Health Check

```bash
curl http://localhost:3000/health
# {"status":"ok","version":"0.1.1","service":"rcommerce-frontend"}
```

### Prometheus Metrics (future)

Planned metrics:
- `http_requests_total` - Total HTTP requests
- `http_request_duration_seconds` - Request latency
- `cache_hits_total` - Cache hit counter
- `rate_limit_hits_total` - Rate limit counter

### Logging

```bash
# JSON logging (for log aggregation)
FRONTEND_LOG_LEVEL=info ./rcommerce-demo 2>&1 | jq

# Output:
# {
#   "timestamp": "2024-02-25T10:30:00Z",
#   "level": "INFO",
#   "message": "Cache HIT: GET:/api/v1/products",
#   "ip": "192.168.1.100"
# }
```

## Security Best Practices

1. **Always use HTTPS in production**
   - Use Caddy or Nginx as reverse proxy with TLS
   - Never expose the frontend server directly to the internet without TLS

2. **Keep API keys secret**
   - Store in environment variables or secure config
   - Never commit API keys to version control
   - Rotate keys regularly

3. **Rate limiting**
   - Default is 60 requests/minute per IP
   - Adjust based on your use case
   - Monitor for abuse

4. **Static file serving**
   - Don't put sensitive files in the static directory
   - Use proper file permissions (read-only for server user)

5. **CORS**
   - Only enable CORS for development
   - In production, serve from same domain or use proper CORS origin whitelist

## Building from Source

```bash
# Clone repository
git clone https://github.com/creativebastard/rcommerce.git
cd rcommerce

# Build release binary
cargo build --release -p rcommerce-demo-server

# Binary location:
# target/release/rcommerce-demo
```

### Cross-Compilation

```bash
# For Linux from macOS
cargo zigbuild --release --target x86_64-unknown-linux-musl -p rcommerce-demo-server
```

## Troubleshooting

### "API key is required"
```bash
# Set API key via environment
export FRONTEND_API_KEY="ak_yourprefix.yoursecret"
```

### "Static directory does not exist"
```bash
# Create directory and add frontend files
mkdir frontend
cp -r your-frontend/* frontend/
```

### High memory usage
- Reduce `cache_ttl_secs` to clear cache faster
- Restart the server to clear in-memory cache
- Future: Redis support for distributed caching

### Rate limiting too aggressive
```toml
# Increase limit in config
rate_limit_per_minute = 120  # or higher
```

## Roadmap

- [ ] Redis backend for distributed caching
- [ ] WebSocket support for real-time updates
- [ ] Prometheus metrics endpoint
- [ ] Admin dashboard for cache management
- [ ] Edge caching with CDN integration

## License

AGPL-3.0
