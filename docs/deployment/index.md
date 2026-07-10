# Deployment Documentation

This section covers deploying R Commerce in various environments.

## Deployment Options

| Document | Description |
|----------|-------------|
| [01-docker.md](01-docker.md) | Docker and Docker Compose deployment |
| [01-cross-platform.md](01-cross-platform.md) | Cross-platform deployment guide |
| [02-reverse-proxies.md](02-reverse-proxies.md) | Nginx, Caddy, HAProxy configuration |
| [04-security.md](04-security.md) | Security best practices |
| [BUILD.md](BUILD.md) | Building from source |
| [redis-setup.md](redis-setup.md) | Redis configuration for caching |

## Platform-Specific Guides

### Linux
- [systemd](linux/systemd.md) - Systemd service configuration
- [manual](linux/manual.md) - Manual installation

### FreeBSD
- [standalone](freebsd/standalone.md) - FreeBSD standalone deployment
- [jails](freebsd/jails.md) - FreeBSD jail deployment
- [rc.d](freebsd/rc.d.md) - rc.d service script

### macOS
- [launchd](macos/launchd.md) - macOS service configuration

### Cloud
- [Kubernetes](kubernetes.md) - K8s deployment
- [Binary](binary.md) - Static binary deployment

## Quick Start

### Docker (Recommended)

```bash
git clone https://github.com/creativebastard/rcommerce.git
cd rcommerce
docker-compose up -d
```

### Binary

```bash
# Download latest release
curl -L https://github.com/creativebastard/rcommerce/releases/latest/download/rcommerce-linux-amd64 -o rcommerce
chmod +x rcommerce
./rcommerce server
```

## Configuration

See [Configuration Reference](../development/configuration-reference.md) for all available options.

## Related Documentation

- [Architecture Overview](../architecture/index.md) - System architecture
- [Development Guide](../development/developer-guide.md) - Local development setup
