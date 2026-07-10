# Cross-Platform Deployment Guide

## Overview

R commerce is designed to run on multiple operating systems including **FreeBSD**, **Linux**, and **macOS**. This guide provides deployment instructions for each platform, ensuring consistent behavior and performance across all supported environments.

**Why Cross-Platform Support:**
- **FreeBSD**: Excellent for high-performance, secure deployments
- **Linux**: Most common production environment (Ubuntu, CentOS, Debian, etc.)
- **macOS**: Ideal for development and Apple ecosystem integration

## Platform-Specific Features

### Architecture Support Matrix

| Feature | Linux (x86_64) | Linux (ARM64) | FreeBSD (x86_64) | FreeBSD (ARM64) | macOS (x86_64) | macOS (ARM64) |
|---------|----------------|---------------|------------------|-----------------|----------------|---------------|
| **Tier 1** |  |  |  |  |  |  |
| Binary Releases |  |  |  |  |  |  |
| Systemd Service |  |  | ❌ | ❌ | ❌ | ❌ |
| rc.d Script | ❌ | ❌ |  |  | ❌ | ❌ |
| LaunchDaemon | ❌ | ❌ | ❌ | ❌ |  |  |
| kqueue Support | ❌ | ❌ |  |  |  |  |
| epoll Support |  |  | ❌ | ❌ | ❌ | ❌ |
| Jemalloc | ⚠️ Opt | ⚠️ Opt |  Default |  Default | ⚠️ Opt | ⚠️ Opt |

**Platform Tiers:**
- **Tier 1**: Fully supported with automated CI/CD testing
- **Tier 2**: Supported but manual testing required
- **Tier 3**: Community supported

## Building from Source

### Prerequisites

#### All Platforms

```bash
# Rust 1.70+ (Install from https://rustup.rs)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# PostgreSQL 13+

# Optional but recommended
# - pkg-config
# - OpenSSL development headers
```

#### Platform-Specific Prerequisites

**FreeBSD:**
```bash
# Install system packages
pkg install -y \
  postgresql15-client \
  pkgconf \
  openssl \
  ca_root_nss

# For additional features
pkg install -y \
  redis \
  nginx \
  haproxy
```

**Linux (Debian/Ubuntu):**
```bash
# Install build dependencies
apt-get update
apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  postgresql-client \
  libpq-dev \

```

**Linux (CentOS/RHEL/Fedora):**
```bash
# Install build dependencies
yum groupinstall -y "Development Tools"
yum install -y \
  openssl-devel \
  postgresql-devel \
  pkgconfig
```

**macOS:**
```bash
# Install Xcode command line tools
xcode-select --install

# Install Homebrew if not already installed
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install dependencies
brew install \
  postgresql@15 \
  pkg-config
```

### Build Steps (All Platforms)

```bash
# Clone repository
git clone https://github.com/creativebastard/rcommerce.git
cd gocart

# Build release binary
cargo build --release

# Binary location
target/release/rcommerce

# Run tests
cargo test --release

# Check platform compatibility
cargo check --target x86_64-unknown-linux-gnu     # Linux x86_64
cargo check --target aarch64-unknown-linux-gnu    # Linux ARM64
cargo check --target x86_64-unknown-freebsd      # FreeBSD x86_64
cargo check --target aarch64-unknown-freebsd     # FreeBSD ARM64
cargo check --target x86_64-apple-darwin         # macOS x86_64
cargo check --target aarch64-apple-darwin        # macOS ARM64 (Apple Silicon)
```

## Platform-Specific Optimizations

### FreeBSD Optimization

FreeBSD provides several advantages for high-performance deployments:

#### Kernel Tuning

```bash
# /etc/sysctl.conf additions for FreeBSD

# Network optimizations
net.inet.tcp.sendspace=262144
net.inet.tcp.recvspace=262144
net.inet.tcp.sendbuf_max=16777216
net.inet.tcp.recvbuf_max=16777216

# Increase file descriptors
kern.maxfiles=200000
kern.maxfilesperproc=100000

# TCP keepalive for long-running connections
net.inet.tcp.keepintvl=75000
net.inet.tcp.keepidle=75000
net.inet.tcp.always_keepalive=1

# Memory optimizations
kern.ipc.shmseg=512
kern.ipc.shmmni=128
kern.ipc.shmseg=512

# Apply settings
sysctl -f /etc/sysctl.conf
```

#### Jails Deployment (FreeBSD)

FreeBSD Jails provide lightweight containerization:

```bash
# Create jail for rcommerce
sudo jail -c \
  name=rcommerce \
  path=/usr/jails/rcommerce \
  host.hostname=rcommerce.example.com \
  ip4.addr=10.0.0.10 \
  exec.start="/usr/local/bin/rcommerce" \
  exec.stop="kill -TERM 1" \
  persist

# Or use iocage for easier management
pkg install -y iocage
iocage create -n rcommerce -r 13.2-RELEASE
iocage set ip4_addr="10.0.0.10" rcommerce
iocage set boot=on rcommerce
iocage set exec_start="/usr/local/bin/rcommerce" rcommerce
iocage start rcommerce
```

#### Service Script (rc.d)

Create `/usr/local/etc/rc.d/rcommerce`:

```bash
#!/bin/sh
#
# PROVIDE: rcommerce
# REQUIRE: LOGIN
# KEYWORD: shutdown

. /etc/rc.subr

name="rcommerce"
desc="R commerce Headless Ecommerce"
rcommand="/usr/local/bin/${name}"
pidfile="/var/run/${name}/${name}.pid"

load_rc_config $name

: ${rcommerce_enable="NO"}
: ${rcommerce_config="/usr/local/etc/rcommerce.toml"}
: ${rcommerce_data_dir="/var/db/rcommerce"}
: ${rcommerce_log_file="/var/log/rcommerce/rcommerce.log"}

start_cmd="${name}_start"
stop_cmd="${name}_stop"
status_cmd="${name}_status"

rcommerce_start()
{
    /usr/bin/install -d -o rcommerce -g rcommerce -m 750 /var/run/${name}
    /usr/bin/install -d -o rcommerce -g rcommerce -m 750 ${rcommerce_data_dir}
    /usr/bin/install -d -o rcommerce -g rcommerce -m 750 $(dirname ${rcommerce_log_file})
    
    echo "Starting ${desc}."
    
    /usr/sbin/daemon \
        -f \
        -S \
        -p ${pidfile} \
        -u rcommerce \
        -t ${name} \
        env RCOMMERCE_CONFIG="${rcommerce_config}" \
            ${rcommand}
}

rcommerce_stop()
{
    echo "Stopping ${desc}."
    /usr/bin/kill -TERM $(cat ${pidfile})
}

rcommerce_status()
{
    if [ -f ${pidfile} ]; then
        if /bin/pkill -0 -F ${pidfile} >/dev/null 2>&1; then
            echo "${desc} is running as pid $(cat ${pidfile})."
            return 0
        else
            echo "${desc} is not running but pid file exists."
            return 1
        fi
    else
        echo "${desc} is not running."
        return 1
    fi
}

run_rc_command "$1"
```

**FreeBSD-specific compile flags:**
```bash
# Compile with FreeBSD optimizations
cargo rustc --release -- \
  -C target-cpu=native \
  -C link-arg=-pthread \
  -C link-arg=-lthr
```

#### Running on Low Ports (80/443) without Reverse Proxy

By default, non-root users cannot bind to ports below 1024 on FreeBSD. When running R Commerce directly on ports 80/443 without a reverse proxy, you have several options:

**Option 1: Use `mac_portacl` Module (Recommended)**

The `mac_portacl` kernel module allows specific users to bind to privileged ports:

```bash
# Load the mac_portacl module
kldload mac_portacl

# Add to /boot/loader.conf for persistence
echo 'mac_portacl_load="YES"' >> /boot/loader.conf

# Configure port ACL for rcommerce user (UID 1001)
sysctl net.inet.ip.portacl.port_high=1023
sysctl net.inet.ip.portacl.suser_exempt=1
sysctl net.inet.ip.portacl.autoport_exempt=1

# Allow rcommerce user (UID 1001) to bind ports 80 and 443
sysctl net.inet.ip.portacl.rule="uid:1001:tcp:80,uid:1001:tcp:443"

# Make persistent
echo 'net.inet.ip.portacl.port_high=1023' >> /etc/sysctl.conf
echo 'net.inet.ip.portacl.suser_exempt=1' >> /etc/sysctl.conf
echo 'net.inet.ip.portacl.rule="uid:1001:tcp:80,uid:1001:tcp:443"' >> /etc/sysctl.conf
```

**Option 2: Use `ipfw` Port Redirection**

Redirect traffic from port 80/443 to unprivileged ports:

```bash
# Enable IPFW
echo 'firewall_enable="YES"' >> /etc/rc.conf
echo 'firewall_type="workstation"' >> /etc/rc.conf

# Add port redirection rules (add to /etc/rc.firewall or run manually)
ipfw add 100 fwd 127.0.0.1,8080 tcp from any to any 80 in
ipfw add 101 fwd 127.0.0.1,8443 tcp from any to any 443 in

# Start IPFW
service ipfw start
```

**Option 3: Use `setcap` (if available)**

Grant capability to bind privileged ports (requires package):

```bash
# Install libcap
pkg install libcap

# Grant capability to binary
setcap cap_bind_service=+ep /usr/local/bin/rcommerce
```

**Option 4: Run as Root (Not Recommended)**

```bash
# In rc.d script, run as root (not recommended for production)
# Change -u rcommerce to -u root (not recommended)
```

> **Recommendation:** Use Option 1 (`mac_portacl`) for production deployments without a reverse proxy. For most deployments, using a reverse proxy (Nginx, Caddy, HAProxy) is preferred as it provides additional features like SSL termination, caching, and load balancing.

### Linux Optimization

#### Systemd Service (Modern Linux)

Create `/etc/systemd/system/rcommerce.service`:

```ini
[Unit]
Description=R commerce Headless Ecommerce Platform
Documentation=https://github.com/creativebastard/rcommerce
After=network.target
Wants=network-online.target

[Service]
Type=notify
User=rcommerce
Group=rcommerce
ExecStart=/usr/local/bin/rcommerce
Environment="RCOMMERCE_CONFIG=/etc/rcommerce/config.toml"
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/rcommerce /var/log/rcommerce
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictNamespaces=true
LockPersonality=true
RestrictRealtime=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
SystemCallFilter=@system-service
SystemCallErrorNumber=EPERM

# Resource limits
LimitNOFILE=65536
LimitNPROC=4096

# Restart policy
Restart=on-failure
RestartSec=5
StartLimitInterval=60
StartLimitBurst=3

# Performance
Nice=-5
CPUSchedulingPolicy=fifo
CPUSchedulingPriority=50

[Install]
WantedBy=multi-user.target
```

#### Running on Low Ports (80/443) without Reverse Proxy

Linux provides several options for allowing non-root users to bind to privileged ports:

**Option 1: Use `setcap` (Recommended)**

Grant the binary capability to bind to privileged ports:

```bash
# Grant capability to the rcommerce binary
sudo setcap cap_net_bind_service=+ep /usr/local/bin/rcommerce

# Verify
capsh --print | grep cap_net_bind_service
```

> **Note:** This capability is preserved across system reboots but must be re-applied after upgrading the binary.

**Option 2: Use `sysctl` (Linux 4.11+)**

Starting with Linux 4.11, you can modify the privileged port range:

```bash
# Allow all users to bind ports >= 80
sudo sysctl net.ipv4.ip_unprivileged_port_start=80

# Make persistent
echo 'net.ipv4.ip_unprivileged_port_start=80' | sudo tee /etc/sysctl.d/99-rcommerce.conf
sudo sysctl --system
```

> **Warning:** This affects all applications system-wide. Use with caution.

**Option 3: Use `authbind`**

The `authbind` utility allows specific users to bind to specific ports:

```bash
# Install authbind
sudo apt-get install authbind  # Debian/Ubuntu
sudo yum install authbind      # RHEL/CentOS (EPEL)

# Create authbind configuration
sudo touch /etc/authbind/byport/80
sudo touch /etc/authbind/byport/443
sudo chown rcommerce:rcommerce /etc/authbind/byport/80 /etc/authbind/byport/443
sudo chmod 500 /etc/authbind/byport/80 /etc/authbind/byport/443

# Run with authbind
authbind --deep /usr/local/bin/rcommerce
```

**Option 4: Systemd Socket Activation**

Systemd can bind ports on behalf of the service:

```bash
# Create socket unit /etc/systemd/system/rcommerce.socket:
[Unit]
Description=R Commerce Socket

[Socket]
ListenStream=80
ListenStream=443
NoDelay=true
ReusePort=true

[Install]
WantedBy=sockets.target
```

```bash
# Modify service to use socket activation
# In rcommerce.service [Unit] section:
Requires=rcommerce.socket
After=rcommerce.socket

# Enable and start socket
sudo systemctl enable rcommerce.socket
sudo systemctl start rcommerce.socket
```

**Option 5: `iptables` Port Redirection**

Redirect traffic from port 80/443 to unprivileged ports:

```bash
# Redirect port 80 to 8080
sudo iptables -t nat -A PREROUTING -p tcp --dport 80 -j REDIRECT --to-port 8080
sudo iptables -t nat -A OUTPUT -p tcp --dport 80 -j REDIRECT --to-port 8080

# Redirect port 443 to 8443
sudo iptables -t nat -A PREROUTING -p tcp --dport 443 -j REDIRECT --to-port 8443
sudo iptables -t nat -A OUTPUT -p tcp --dport 443 -j REDIRECT --to-port 8443

# Save rules (Debian/Ubuntu)
sudo apt-get install iptables-persistent
sudo netfilter-persistent save

# Save rules (RHEL/CentOS)
sudo service iptables save
```

> **Recommendation:** Use `setcap` (Option 1) for production deployments without a reverse proxy. It's simple, secure, and doesn't require modifying system-wide settings.

#### Linux Kernel Parameters

```bash
# /etc/sysctl.d/rcommerce.conf

# Network stack tuning
net.core.somaxconn = 65535
net.core.netdev_max_backlog = 65535
net.ipv4.tcp_max_syn_backlog = 65535
net.ipv4.ip_local_port_range = 1024 65535

# TCP tuning
net.ipv4.tcp_fin_timeout = 15
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_keepalive_time = 300
net.ipv4.tcp_keepalive_intvl = 60
net.ipv4.tcp_keepalive_probes = 5

# Increase file descriptors
fs.file-max = 2097152
fs.nr_open = 2097152

# Virtual memory
vm.swappiness = 10
vm.dirty_ratio = 15
vm.dirty_background_ratio = 5

# Apply immediately
sysctl -p /etc/sysctl.d/rcommerce.conf
```

#### Docker Deployment (Linux)

```dockerfile
# Dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r rcommerce && useradd -r -g rcommerce rcommerce

COPY --from=builder /app/target/release/rcommerce /usr/local/bin/
COPY --from=builder /app/config/default.toml /etc/rcommerce/config.toml

RUN mkdir -p /var/lib/rcommerce /var/log/rcommerce && \
    chown -R rcommerce:rcommerce /var/lib/rcommerce /var/log/rcommerce

USER rcommerce

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1

CMD ["rcommerce"]
```

```yaml
# docker-compose.yml
version: '3.8'

services:
  rcommerce:
    build: .
    image: rcommerce:latest
    container_name: rcommerce
    restart: unless-stopped
    ports:
      - "8080:8080"
    environment:
      - RCOMMERCE_CONFIG=/etc/rcommerce/config.toml
      - RUST_LOG=info
      - DATABASE_URL=postgres://user:pass@postgres:5432/rcommerce
    volumes:
      - ./config/production.toml:/etc/rcommerce/config.toml:ro
      - rcommerce_data:/var/lib/rcommerce
      - rcommerce_logs:/var/log/rcommerce
    depends_on:
      - postgres
      - redis
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
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 1G
        reservations:
          cpus: '0.5'
          memory: 512M
    networks:
      - rcommerce_network

  postgres:
    image: postgres:15-alpine
    container_name: rcommerce_db
    restart: unless-stopped
    environment:
      - POSTGRES_DB=rcommerce
      - POSTGRES_USER=rcommerce
      - POSTGRES_PASSWORD=secure_password_here
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./init.sql:/docker-entrypoint-initdb.d/init.sql:ro
    networks:
      - rcommerce_network

  redis:
    image: redis:7-alpine
    container_name: rcommerce_cache
    restart: unless-stopped
    command: redis-server --appendonly yes --maxmemory 256mb --maxmemory-policy allkeys-lru
    volumes:
      - redis_data:/data
    networks:
      - rcommerce_network

volumes:
  postgres_data:
  redis_data:
  rcommerce_data:
  rcommerce_logs:

networks:
  rcommerce_network:
    driver: bridge
```

### macOS Optimization

#### LaunchDaemon Configuration

Create `/Library/LaunchDaemons/com.rcommerce.rcommerce.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.rcommerce.rcommerce</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/rcommerce</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RCOMMERCE_CONFIG</key>
        <string>/usr/local/etc/rcommerce/config.toml</string>
        <key>RUST_LOG</key>
        <string>info</string>
        <key>RUST_BACKTRACE</key>
        <string>1</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>30</integer>
    <key>StandardOutPath</key>
    <string>/var/log/rcommerce/rcommerce.log</string>
    <key>StandardErrorPath</key>
    <string>/var/log/rcommerce/rcommerce.error.log</string>
    <key>WorkingDirectory</key>
    <string>/var/lib/rcommerce</string>
    <key>UserName</key>
    <string>_rcommerce</string>
    <key>GroupName</key>
    <string>_rcommerce</string>
    <key>Nice</key>
    <integer>-5</integer>
    <key>ProcessType</key>
    <string>Background</string>
    <key>LimitLoadToSessionType</key>
    <array>
        <string>System</string>
    </array>
    <key>LowPriorityIO</key>
    <false/>
</dict>
</plist>
```

**Load the service:**
```bash
# Create service user
sudo sysadminctl -addUser _rcommerce -fullName "R commerce Service" -UID 502 -GID 20 -shell /bin/false -home /var/empty -admin false

# Create directories
sudo mkdir -p /usr/local/bin /usr/local/etc/rcommerce /var/lib/rcommerce /var/log/rcommerce
sudo chown _rcommerce:_rcommerce /var/lib/rcommerce /var/log/rcommerce

# Load and start service
sudo launchctl load /Library/LaunchDaemons/com.rcommerce.rcommerce.plist
sudo launchctl start com.rcommerce.rcommerce

# Check status
sudo launchctl list | grep rcommerce
```

#### Running on Low Ports (80/443) without Reverse Proxy

macOS provides limited options for allowing non-root users to bind to privileged ports compared to Linux:

**Option 1: Run with Root Privileges (Not Recommended)**

The LaunchDaemon can run as root:

```xml
<!-- Change these keys in the plist -->
<key>UserName</key>
<string>root</string>
<key>GroupName</key>
<string>wheel</string>
```

> **Warning:** Running as root is not recommended for production. Consider using a reverse proxy instead.

**Option 2: Use `pfctl` Port Forwarding**

macOS uses Packet Filter (PF) for port forwarding:

```bash
# Create anchor file
sudo tee /etc/pf.anchors/rcommerce << 'EOF'
rdr pass on lo0 inet proto tcp from any to any port 80 -> 127.0.0.1 port 8080
rdr pass on lo0 inet proto tcp from any to any port 443 -> 127.0.0.1 port 8443
rdr pass on en0 inet proto tcp from any to any port 80 -> 127.0.0.1 port 8080
rdr pass on en0 inet proto tcp from any to any port 443 -> 127.0.0.1 port 8443
EOF

# Enable PF
sudo pfctl -e

# Load anchor
sudo pfctl -a rcommerce -f /etc/pf.anchors/rcommerce

# Make persistent (add to /etc/pf.conf)
echo 'rdr-anchor "rcommerce"' | sudo tee -a /etc/pf.conf
echo 'load anchor "rcommerce" from "/etc/pf.anchors/rcommerce"' | sudo tee -a /etc/pf.conf
```

**Option 3: Use `socat` as a Bridge**

Run a privileged helper that forwards to the unprivileged service:

```bash
# Install socat
brew install socat

# Run port forwarder (requires root)
sudo socat TCP-LISTEN:80,fork TCP:localhost:8080 &
sudo socat TCP-LISTEN:443,fork TCP:localhost:8443 &
```

**Option 4: LaunchDaemon with Socket Activation**

macOS LaunchDaemons support socket activation:

```xml
<!-- Add these keys to the plist -->
<key>Sockets</key>
<dict>
    <key>Listeners</key>
    <dict>
        <key>SockServiceName</key>
        <string>80</string>
        <key>SockType</key>
        <string>stream</string>
        <key>SockFamily</key>
        <string>IPv4</string>
    </dict>
    <key>ListenersSSL</key>
    <dict>
        <key>SockServiceName</key>
        <string>443</string>
        <key>SockType</key>
        <string>stream</string>
        <key>SockFamily</key>
        <string>IPv4</string>
    </dict>
</dict>
```

> **Note:** Your application must be modified to support socket activation via the `launch_activate_socket()` API.

> **Recommendation:** Use a reverse proxy (Nginx, Caddy) on macOS for production deployments requiring ports 80/443. The `pfctl` option (Option 2) is the best alternative if you must run without a reverse proxy.

#### macOS Performance Tuning

```bash
# Increase file descriptor limits
echo 'ulimit -n 65536' >> ~/.zshrc  # or ~/.bash_profile

# Network optimizations (if not using Docker)
sudo sysctl -w kern.maxfiles=65536
sudo sysctl -w kern.maxfilesperproc=32768
sudo sysctl -w net.inet.tcp.keepalive=300000

# Make persistent
echo 'kern.maxfiles=65536' | sudo tee -a /etc/sysctl.conf
echo 'kern.maxfilesperproc=32768' | sudo tee -a /etc/sysctl.conf
echo 'net.inet.tcp.keepalive=300000' | sudo tee -a /etc/sysctl.conf

# For Apple Silicon (M1/M2), ensure native compilation
cargo build --release --target aarch64-apple-darwin
```

## Platform-Specific Database Setup

### PostgreSQL on FreeBSD

```bash
# Install PostgreSQL
pkg install -y postgresql15-server postgresql15-contrib

# Initialize database
/usr/local/etc/rc.d/postgresql initdb

# Configure PostgreSQL for production
# Edit /var/db/postgres/data15/postgresql.conf

echo "
# R commerce Optimizations
max_connections = 200
shared_buffers = 2GB
effective_cache_size = 6GB
maintenance_work_mem = 512MB
checkpoint_completion_target = 0.9
wal_buffers = 16MB
default_statistics_target = 100
random_page_cost = 1.1
effective_io_concurrency = 200
work_mem = 10485kB
min_wal_size = 1GB
max_wal_size = 4GB
max_worker_processes = 8
max_parallel_workers_per_gather = 4
max_parallel_workers = 8
max_parallel_maintenance_workers = 4
" >> /var/db/postgres/data15/postgresql.conf

# Enable and start PostgreSQL
sysrc postgresql_enable="YES"
service postgresql start

# Create database and user
sudo -u postgres psql <<EOF
CREATE DATABASE rcommerce;
CREATE USER rcommerce WITH PASSWORD 'secure_password';
GRANT ALL PRIVILEGES ON DATABASE rcommerce TO rcommerce;
\c rcommerce
GRANT ALL ON SCHEMA public TO rcommerce;
EOF

# Configure pg_hba.conf for local connections
echo "host    rcommerce    rcommerce    127.0.0.1/32    md5" >> /var/db/postgres/data15/pg_hba.conf
service postgresql restart
```

### PostgreSQL on Linux (Ubuntu/Debian)

```bash
# Install PostgreSQL
sudo apt-get update
sudo apt-get install -y postgresql-15 postgresql-contrib

# Configure PostgreSQL
sudo -u postgres psql <<EOF
ALTER USER postgres PASSWORD 'secure_password';
CREATE DATABASE rcommerce;
CREATE USER rcommerce WITH PASSWORD 'secure_password';
GRANT ALL PRIVILEGES ON DATABASE rcommerce TO rcommerce;
\c rcommerce
GRANT ALL ON SCHEMA public TO rcommerce;
EOF

# Edit /etc/postgresql/15/main/postgresql.conf
sudo tee -a /etc/postgresql/15/main/postgresql.conf <<EOF
# R commerce Optimizations
max_connections = 200
shared_buffers = 2GB
effective_cache_size = 6GB
maintenance_work_mem = 512MB
checkpoint_completion_target = 0.9
wal_buffers = 16MB
EOF

sudo systemctl restart postgresql
```

### PostgreSQL on macOS (Homebrew)

```bash
# Install PostgreSQL
brew install postgresql@15
brew services start postgresql@15

# Configure
/opt/homebrew/bin/psql postgres <<EOF
CREATE DATABASE rcommerce;
CREATE USER rcommerce WITH PASSWORD 'secure_password';
GRANT ALL PRIVILEGES ON DATABASE rcommerce TO rcommerce;
\c rcommerce
GRANT ALL ON SCHEMA public TO rcommerce;
EOF

# Optimize configuration
echo "
# R commerce Optimizations
max_connections = 200
shared_buffers = 2GB
effective_cache_size = 6GB
" >> /opt/homebrew/var/postgresql@15/postgresql.conf

brew services restart postgresql@15
```

## Platform-Specific Nginx Configuration

### FreeBSD Nginx with R commerce

```bash
# Install Nginx
pkg install -y nginx

# Edit /usr/local/etc/nginx/rcommerce.conf
```

```nginx
# /usr/local/etc/nginx/rcommerce.conf

upstream rcommerce_backend {
    server 127.0.0.1:8080;
    keepalive 32;
}

server {
    listen 80;
    server_name api.yourstore.com;
    
    access_log /var/log/nginx/rcommerce_access.log;
    error_log /var/log/nginx/rcommerce_error.log;
    
    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "no-referrer-when-downgrade" always;
    add_header Content-Security-Policy "default-src 'self' http: https: data: blob: 'unsafe-inline'" always;
    
    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api:10m rate=100r/s;
    limit_req zone=api burst=200 nodelay;
    
    location / {
        limit_req zone=api;
        
        proxy_pass http://rcommerce_backend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_cache_bypass $http_upgrade;
        
        # Timeouts
        proxy_connect_timeout 5s;
        proxy_send_timeout 30s;
        proxy_read_timeout 30s;
        
        # Buffer settings
        proxy_buffering on;
        proxy_buffer_size 4k;
        proxy_buffers 8 4k;
        proxy_busy_buffers_size 8k;
    }
    
    location ~ ^/(health|metrics)$ {
        proxy_pass http://rcommerce_backend;
        access_log off;
    }
}
```

```bash
# Enable Nginx and start
sysrc nginx_enable="YES"
service nginx start

# Test configuration
nginx -t
```

### Linux Nginx Configuration

Same configuration as FreeBSD, but paths differ:
- Config: `/etc/nginx/sites-available/rcommerce`
- Logs: `/var/log/nginx/`
- Enable site: `ln -s /etc/nginx/sites-available/rcommerce /etc/nginx/sites-enabled/`

### macOS Nginx (Homebrew)

```bash
brew install nginx

# Copy same config to
/opt/homebrew/etc/nginx/servers/rcommerce.conf

brew services start nginx
```

## Platform Testing Checklist

Use this checklist to verify deployment on each platform:

### Pre-Deployment
- [ ] Operating system version meets requirements
- [ ] Sufficient disk space (minimum 2GB)
- [ ] Sufficient RAM (minimum 1GB, 2GB recommended)
- [ ] Network connectivity verified
- [ ] Database server installed and configured
- [ ] Backup strategy in place

### Build Phase
- [ ] Rust toolchain installed (1.70+)
- [ ] All dependencies available
- [ ] Binary compiles without errors
- [ ] Tests pass (`cargo test`)
- [ ] Binary size reasonable (<50MB)

### Configuration Phase
- [ ] Configuration file created and validated
- [ ] Database connection tested
- [ ] File permissions set correctly
- [ ] Log directory created and writable
- [ ] Service user created (non-root)

### Deployment Phase
- [ ] Service/Daemon configured
- [ ] Service starts successfully
- [ ] Health check endpoint responds
- [ ] Logs written without errors
- [ ] Memory usage within limits

### Post-Deployment
- [ ] API endpoints accessible
- [ ] Database migrations applied
- [ ] Performance acceptable
- [ ] Monitoring alerts configured
- [ ] Backup restoration tested

## Troubleshooting

### FreeBSD Issues

**Problem: High latency on FreeBSD**
```bash
# Check if kern.ipc.soacceptqueue is too low
sysctl kern.ipc.soacceptqueue

# Increase it
sysctl kern.ipc.soacceptqueue=1024
echo 'kern.ipc.soacceptqueue=1024' >> /etc/sysctl.conf
```

**Problem: Jails can't bind to ports**
```bash
# Allow raw sockets in jail
iocage set allow_raw_sockets=1 rcommerce
```

### Linux Issues

**Problem: Too many open files**
```bash
# Check current limits
ulimit -n

# Increase for service user
sudo tee -a /etc/security/limits.conf <<EOF
rcommerce soft nofile 65536
rcommerce hard nofile 65536
EOF

# System-wide
sudo tee /etc/systemd/system/rcommerce.service.d/limits.conf <<EOF
[Service]
LimitNOFILE=65536
EOF

sudo systemctl daemon-reload
sudo systemctl restart rcommerce
```

### macOS Issues

**Problem: Port 8080 already in use**
```bash
# Find process using port
lsof -i :8080

# Kill it or use different port
launchctl unload /Library/LaunchDaemons/com.rcommerce.rcommerce.plist
# Edit plist to use different port
launchctl load /Library/LaunchDaemons/com.rcommerce.rcommerce.plist
```

**Problem: Permission denied on log files**
```bash
# Fix permissions
sudo chown -R _rcommerce:_rcommerce /var/log/rcommerce
sudo chmod 755 /var/log/rcommerce
```

---

Next: [Data Modeling](../architecture/02-data-modeling.md) - Complete data model documentation
