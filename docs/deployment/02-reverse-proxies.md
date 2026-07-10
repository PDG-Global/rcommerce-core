# Reverse Proxy Deployment Guide

This guide covers deploying R Commerce behind popular reverse proxies and load balancers.

## Table of Contents

- [Overview](#overview)
- [Nginx](#nginx)
- [Caddy](#caddy)
- [HAProxy](#haproxy)
- [Traefik](#traefik)
- [Comparison](#comparison)
- [SSL/TLS Configuration](#ssltls-configuration)
- [Performance Tuning](#performance-tuning)

## Overview

A reverse proxy sits between clients and your R Commerce server, providing:

- **SSL/TLS termination** - Handle HTTPS encryption
- **Load balancing** - Distribute traffic across multiple instances
- **Caching** - Reduce backend load
- **Security** - Hide backend details, rate limiting
- **Compression** - Gzip/Brotli compression

## Nginx

### Installation

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install nginx

# CentOS/RHEL
sudo yum install nginx

# macOS
brew install nginx

# FreeBSD
pkg install nginx
```

### Basic Configuration

```nginx
# /etc/nginx/sites-available/rcommerce
server {
    listen 80;
    server_name commerce.example.com;
    
    # Redirect HTTP to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name commerce.example.com;

    # SSL certificates
    ssl_certificate /etc/nginx/ssl/commerce.crt;
    ssl_certificate_key /etc/nginx/ssl/commerce.key;
    
    # Modern SSL configuration
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;
    ssl_prefer_server_ciphers off;
    
    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;

    # Gzip compression
    gzip on;
    gzip_types text/plain text/css application/json application/javascript;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Forwarded-Host $host;
        proxy_set_header X-Forwarded-Port $server_port;
        
        # WebSocket support
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        
        # Timeouts
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }
    
    # Static files (if serving directly)
    location /static {
        alias /var/www/rcommerce/static;
        expires 1y;
        add_header Cache-Control "public, immutable";
    }
    
    # Health check endpoint
    location /health {
        proxy_pass http://127.0.0.1:8080/health;
        access_log off;
    }
}
```

### Load Balancing

```nginx
upstream rcommerce_backend {
    least_conn;  # Load balancing method
    
    server 127.0.0.1:8080 weight=5;
    server 127.0.0.1:8081 weight=5;
    server 127.0.0.1:8082 backup;
    
    keepalive 32;
}

server {
    listen 443 ssl http2;
    
    location / {
        proxy_pass http://rcommerce_backend;
        # ... rest of configuration
    }
}
```

### Rate Limiting

```nginx
# Define limit zone
limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
limit_req_zone $binary_remote_addr zone=login:10m rate=1r/s;

server {
    location /api/ {
        limit_req zone=api burst=20 nodelay;
        proxy_pass http://127.0.0.1:8080;
    }
    
    location /api/v1/auth/login {
        limit_req zone=login burst=5 nodelay;
        proxy_pass http://127.0.0.1:8080;
    }
}
```

## Caddy

Caddy is a modern, easy-to-use web server with automatic HTTPS.

### Installation

```bash
# Ubuntu/Debian
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install caddy

# macOS
brew install caddy

# Docker
docker run -p 80:80 -p 443:443 caddy:2
```

### Basic Configuration

```caddyfile
# /etc/caddy/Caddyfile
commerce.example.com {
    # Automatic HTTPS (Let's Encrypt)
    tls admin@example.com
    
    # Reverse proxy to R Commerce
    reverse_proxy localhost:8080 {
        # WebSocket support
        header_up Upgrade {http.request.header.Upgrade}
        header_up Connection {http.request.header.Connection}
        
        # Forward original headers
        header_up X-Real-IP {http.request.remote}
        header_up X-Forwarded-For {http.request.header.X-Forwarded-For}
        header_up X-Forwarded-Proto {http.request.scheme}
        header_up X-Forwarded-Host {http.request.host}
        
        # Health checks
        health_uri /health
        health_interval 30s
        health_timeout 5s
    }
    
    # Compression
    encode gzip zstd
    
    # Security headers
    header {
        X-Frame-Options "SAMEORIGIN"
        X-Content-Type-Options "nosniff"
        X-XSS-Protection "1; mode=block"
        Referrer-Policy "strict-origin-when-cross-origin"
        remove Server
    }
    
    # Logging
    log {
        output file /var/log/caddy/access.log {
            roll_size 100mb
            roll_keep 10
            roll_keep_for 720h
        }
        format json
    }
    
    # Static file serving (optional)
    handle_path /static/* {
        root * /var/www/rcommerce/static
        file_server
        header Cache-Control "public, max-age=31536000, immutable"
    }
}
```

### Load Balancing

```caddyfile
commerce.example.com {
    tls admin@example.com
    
    reverse_proxy backend1:8080 backend2:8080 backend3:8080 {
        # Load balancing policy
        lb_policy least_conn
        
        # Health checks
        health_uri /health
        health_interval 30s
        health_timeout 5s
        
        # Retry failed requests
        fail_duration 30s
        max_fails 3
        unhealthy_latency 5s
    }
    
    encode gzip zstd
}
```

### Rate Limiting

```caddyfile
commerce.example.com {
    tls admin@example.com
    
    # Rate limiting module (requires caddy-rate-limit)
    rate_limit {
        zone static_example {
            key static
            events 100
            window 1m
        }
        zone dynamic_example {
            key {http.request.remote}
            events 10
            window 1s
        }
    }
    
    reverse_proxy localhost:8080
}
```

### API with Caddy

```caddyfile
# Caddy JSON API for dynamic configuration
curl -X POST "http://localhost:2019/load" \
  -H "Content-Type: application/json" \
  -d '{
    "apps": {
      "http": {
        "servers": {
          "srv0": {
            "listen": [":443"],
            "routes": [{
              "match": [{"host": ["commerce.example.com"]}],
              "handle": [{
                "handler": "reverse_proxy",
                "upstreams": [{"dial": "localhost:8080"}]
              }]
            }]
          }
        }
      }
    }
  }'
```

## HAProxy

HAProxy is a high-performance TCP/HTTP load balancer.

### Installation

```bash
# Ubuntu/Debian
sudo apt-get install haproxy

# CentOS/RHEL
sudo yum install haproxy

# macOS
brew install haproxy

# FreeBSD
pkg install haproxy
```

### Basic Configuration

```haproxy
# /etc/haproxy/haproxy.cfg
global
    log /dev/log local0
    log /dev/log local1 notice
    chroot /var/lib/haproxy
    stats socket /run/haproxy/admin.sock mode 660 level admin
    stats timeout 30s
    user haproxy
    group haproxy
    daemon
    
    # Performance tuning
    maxconn 4096
    nbproc 2
    
    # SSL/TLS
    ssl-default-bind-ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256
    ssl-default-bind-options ssl-min-ver TLSv1.2 no-tls-tickets

defaults
    mode http
    option httplog
    option dontlognull
    option forwardfor except 127.0.0.0/8
    option redispatch
    
    retries 3
    timeout connect 5000
    timeout client 50000
    timeout server 50000
    timeout http-request 10s
    timeout http-keep-alive 2s

frontend https_frontend
    bind *:80
    bind *:443 ssl crt /etc/haproxy/ssl/commerce.pem alpn h2,http/1.1
    
    # Redirect HTTP to HTTPS
    redirect scheme https code 301 if !{ ssl_fc }
    
    # Security headers
    http-response set-header X-Frame-Options SAMEORIGIN
    http-response set-header X-Content-Type-Options nosniff
    http-response set-header X-XSS-Protection "1; mode=block"
    http-response set-header Referrer-Policy "strict-origin-when-cross-origin"
    
    # Rate limiting (requires stick-table)
    stick-table type ip size 100k expire 30s store http_req_rate(10s)
    tcp-request connection track-sc0 src
    http-request deny deny_status 429 if { sc_http_req_rate(0) gt 100 }
    
    # WebSocket support
    acl is_websocket hdr(Upgrade) -i websocket
    acl is_connection_upgrade hdr_beg(Connection) -i upgrade
    
    use_backend websocket_backend if is_websocket is_connection_upgrade
    default_backend rcommerce_backend

backend rcommerce_backend
    balance roundrobin
    
    # Health checks
    option httpchk GET /health
    http-check expect status 200
    
    server rcommerce1 127.0.0.1:8080 check inter 5s rise 2 fall 3
    server rcommerce2 127.0.0.1:8081 check inter 5s rise 2 fall 3 backup
    
    # Compression
    compression algo gzip
    compression type text/html text/plain text/css application/json

backend websocket_backend
    balance source
    option httpchk GET /health
    server rcommerce1 127.0.0.1:8080 check inter 5s rise 2 fall 3
```

### Advanced Load Balancing

```haproxy
backend rcommerce_backend
    # Load balancing algorithms
    # balance roundrobin     # Standard round-robin
    # balance leastconn      # Best for long connections
    # balance source         # Session persistence
    balance uri             # Based on URL hash
    hash-type consistent    # Consistent hashing
    
    # Cookie-based session persistence
    cookie SERVERID insert indirect nocache
    
    server rcommerce1 127.0.0.1:8080 cookie s1 check inter 5s rise 2 fall 3 weight 100
    server rcommerce2 127.0.0.1:8081 cookie s2 check inter 5s rise 2 fall 3 weight 100
    server rcommerce3 127.0.0.1:8082 cookie s3 check inter 5s rise 2 fall 3 weight 50 backup
```

### Statistics Page

```haproxy
listen stats
    bind *:8404 ssl crt /etc/haproxy/ssl/stats.pem
    stats enable
    stats uri /stats
    stats refresh 10s
    stats admin if TRUE
    stats auth admin:secure_password
```

## Traefik

Traefik is a cloud-native edge router with automatic service discovery.

### Installation

```bash
# Docker
docker run -d \
  -p 80:80 \
  -p 443:443 \
  -p 8080:8080 \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v /path/to/traefik.yml:/etc/traefik/traefik.yml \
  traefik:v2.10

# Kubernetes (Helm)
helm repo add traefik https://traefik.github.io/charts
helm install traefik traefik/traefik

# Binary
curl -s https://raw.githubusercontent.com/traefik/traefik/master/script/gcg/traefik-beta-release.sh | bash
```

### Static Configuration

```yaml
# traefik.yml
global:
  checkNewVersion: false
  sendAnonymousUsage: false

api:
  dashboard: true
  insecure: true  # Set to false in production

entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
          permanent: true
  
  websecure:
    address: ":443"
    http:
      tls:
        certResolver: letsencrypt

providers:
  docker:
    exposedByDefault: false
    network: rcommerce_network
  
  file:
    directory: /etc/traefik/dynamic
    watch: true

certificatesResolvers:
  letsencrypt:
    acme:
      email: admin@example.com
      storage: /letsencrypt/acme.json
      tlsChallenge: {}

log:
  level: INFO
  format: json

accessLog:
  format: json

metrics:
  prometheus:
    addEntryPointsLabels: true
    addServicesLabels: true
```

### Dynamic Configuration

```yaml
# /etc/traefik/dynamic/rcommerce.yml
http:
  routers:
    rcommerce:
      rule: "Host(`commerce.example.com`)"
      entryPoints:
        - websecure
      service: rcommerce-service
      tls:
        certResolver: letsencrypt
      middlewares:
        - security-headers
        - rate-limit
        - compress
    
    rcommerce-api:
      rule: "Host(`commerce.example.com`) && PathPrefix(`/api`)"
      entryPoints:
        - websecure
      service: rcommerce-service
      tls:
        certResolver: letsencrypt
      middlewares:
        - security-headers
        - api-rate-limit
        - compress

  services:
    rcommerce-service:
      loadBalancer:
        servers:
          - url: "http://rcommerce1:8080"
          - url: "http://rcommerce2:8080"
          - url: "http://rcommerce3:8080"
        healthCheck:
          path: /health
          interval: 10s
          timeout: 5s
          followRedirects: true
        passHostHeader: true

  middlewares:
    security-headers:
      headers:
        customRequestHeaders:
          X-Forwarded-Proto: "https"
        customResponseHeaders:
          X-Frame-Options: "SAMEORIGIN"
          X-Content-Type-Options: "nosniff"
          X-XSS-Protection: "1; mode=block"
          Referrer-Policy: "strict-origin-when-cross-origin"
        contentSecurityPolicy: "default-src 'self'"
        stsSeconds: 31536000
        stsIncludeSubdomains: true
        stsPreload: true

    rate-limit:
      rateLimit:
        average: 100
        burst: 50
        period: 1m

    api-rate-limit:
      rateLimit:
        average: 60
        burst: 30
        period: 1m

    compress:
      compress:
        excludedContentTypes:
          - text/event-stream

    # Retry failed requests
    retry:
      retry:
        attempts: 3
        initialInterval: 100ms

tcp:
  routers:
    # For raw TCP if needed
  
  services:

udp:
  routers:
  
  services:
```

### Docker Integration

```yaml
# docker-compose.yml
version: '3.8'

services:
  traefik:
    image: traefik:v2.10
    command:
      - "--api.insecure=true"
      - "--providers.docker=true"
      - "--providers.docker.exposedbydefault=false"
      - "--entrypoints.web.address=:80"
      - "--entrypoints.websecure.address=:443"
      - "--certificatesresolvers.letsencrypt.acme.tlschallenge=true"
      - "--certificatesresolvers.letsencrypt.acme.email=admin@example.com"
      - "--certificatesresolvers.letsencrypt.acme.storage=/letsencrypt/acme.json"
    ports:
      - "80:80"
      - "443:443"
      - "8080:8080"  # Dashboard
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - ./letsencrypt:/letsencrypt
    networks:
      - rcommerce_network

  rcommerce:
    image: rcommerce:latest
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.rcommerce.rule=Host(`commerce.example.com`)"
      - "traefik.http.routers.rcommerce.entrypoints=websecure"
      - "traefik.http.routers.rcommerce.tls.certresolver=letsencrypt"
      - "traefik.http.services.rcommerce.loadbalancer.server.port=8080"
      - "traefik.http.middlewares.rcommerce-ratelimit.ratelimit.average=100"
      - "traefik.http.middlewares.rcommerce-ratelimit.ratelimit.burst=50"
    networks:
      - rcommerce_network
    deploy:
      replicas: 3

networks:
  rcommerce_network:
    external: true
```

### Kubernetes Ingress

```yaml
# rcommerce-ingress.yml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: rcommerce-ingress
  annotations:
    traefik.ingress.kubernetes.io/router.entrypoints: websecure
    traefik.ingress.kubernetes.io/router.tls: "true"
    traefik.ingress.kubernetes.io/router.tls.certresolver: letsencrypt
    traefik.ingress.kubernetes.io/router.middlewares: default-rate-limit@kubernetescrd
spec:
  rules:
  - host: commerce.example.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: rcommerce-service
            port:
              number: 8080

---
apiVersion: traefik.containo.us/v1alpha1
kind: Middleware
metadata:
  name: rate-limit
spec:
  rateLimit:
    average: 100
    burst: 50

---
apiVersion: traefik.containo.us/v1alpha1
kind: Middleware
metadata:
  name: security-headers
spec:
  headers:
    customResponseHeaders:
      X-Frame-Options: "SAMEORIGIN"
      X-Content-Type-Options: "nosniff"
```

## Comparison

| Feature | Nginx | Caddy | HAProxy | Traefik |
|---------|-------|-------|---------|---------|
| **Ease of Use** | Moderate | Easy | Moderate | Easy |
| **Performance** | Excellent | Good | Excellent | Good |
| **Auto HTTPS** | No | Yes | No | Yes |
| **Dynamic Config** | No | Yes | Limited | Yes |
| **Docker Native** | No | Yes | No | Yes |
| **K8s Ingress** | Via Ingress | Via CRD | Via Ingress | Native |
| **WebSocket** | Yes | Yes | Yes | Yes |
| **gRPC** | Yes | Yes | Yes | Yes |
| **Rate Limiting** | Yes | Plugin | Yes | Yes |
| **Metrics** | Via module | Basic | Built-in | Prometheus |

## SSL/TLS Configuration

### Let's Encrypt with Certbot (Nginx)

```bash
# Install Certbot
sudo apt-get install certbot python3-certbot-nginx

# Obtain certificate
sudo certbot --nginx -d commerce.example.com

# Auto-renewal
sudo certbot renew --dry-run
```

### Let's Encrypt with Caddy

Automatic - no configuration needed!

### Let's Encrypt with Traefik

```yaml
certificatesResolvers:
  letsencrypt:
    acme:
      email: admin@example.com
      storage: /letsencrypt/acme.json
      httpChallenge:
        entryPoint: web
```

## Performance Tuning

### Nginx

```nginx
worker_processes auto;
worker_rlimit_nofile 65535;

events {
    worker_connections 4096;
    use epoll;
    multi_accept on;
}

http {
    sendfile on;
    tcp_nopush on;
    tcp_nodelay on;
    
    # Connection keepalive
    keepalive_timeout 30;
    keepalive_requests 1000;
    
    # Buffers
    client_body_buffer_size 128k;
    client_max_body_size 50m;
    
    # Compression
    gzip on;
    gzip_vary on;
    gzip_min_length 1000;
    gzip_comp_level 5;
    gzip_types
        text/plain
        text/css
        application/json
        application/javascript;
}
```

### Caddy

```caddyfile
{
    # Global options
    auto_https off
    admin off
    
    # Performance
    servers {
        max_header_size 10MB
        read_header_timeout 30s
        write_timeout 30s
    }
}
```

### HAProxy

```haproxy
global
    # Performance
    maxconn 10000
    
    # Threading
    nbthread 4
    
    # Memory
    maxcompcpuusage 50
    
defaults
    # Timeouts
    timeout connect 5s
    timeout client 30s
    timeout server 30s
    timeout tunnel 1h
    
    # HTTP reuse
    http-reuse aggressive
```

### Traefik

```yaml
entryPoints:
  websecure:
    address: ":443"
    transport:
      respondingTimeouts:
        readTimeout: 30s
        writeTimeout: 30s
        idleTimeout: 30s
      lifeCycle:
        requestAcceptGraceTimeout: 10s
        graceTimeOut: 10s
```

---

For more information, consult the official documentation:
- [Nginx Documentation](https://nginx.org/en/docs/)
- [Caddy Documentation](https://caddyserver.com/docs/)
- [HAProxy Documentation](https://www.haproxy.org/documentation.html)
- [Traefik Documentation](https://doc.traefik.io/traefik/)
