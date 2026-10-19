# R Commerce Core - Headless E-Commerce Platform

A high-performance, Rust-based headless e-commerce platform designed for multi-platform deployment and enterprise-scale operations.

This repository contains the core engine: the Rust workspace with models, services, API server, and CLI.

## Features

- **Headless Architecture**: API-first design for maximum flexibility
- **Multi-Platform Support**: Linux (Systemd), macOS (LaunchDaemon), FreeBSD
- **Performance**: Sub-10ms API response, 10-50MB memory footprint, ~20MB binary
- **PostgreSQL Database**: Optimized for PostgreSQL 14+ with advanced features
- **Redis Caching**: Optional Redis support for high-performance caching
- **Extensible Plugin System**: Payment gateways, shipping providers, storage backends
- **Enterprise Ready**: Audit logs, API key auth, rate limiting, WebSocket support, comprehensive logging
- **Interactive CLI**: Full-featured CLI with interactive prompts for product and customer creation

## Project Structure

```
crates/
├── rcommerce-core/     # Core library - models, traits, config, repositories
├── rcommerce-api/      # HTTP API server (Axum)
├── rcommerce-cli/      # Command-line management tool with interactive prompts (produces the rcommerce binary)
├── rcommerce/          # Application crate (work in progress)
├── rcommerce-demo/     # Demo frontend server (secure API proxy with static file serving)
└── rcommerce-demo-server/ # Dynamic frontend server with SSR, routing, and edge caching

storefront/             # Whitelabel Astro storefront (generic demo template;
                        # brand via PUBLIC_* build-time env vars)
```

### Core Modules

- **Models**: Product, Customer, Order, Address, Payment, API Keys, etc.
- **Config**: Multi-format configuration (TOML, env vars)
- **Traits**: Repository pattern, plugin architecture
- **Repositories**: PostgreSQL repository implementations
- **Error Handling**: Comprehensive error types with HTTP mapping
- **WebSocket**: Real-time updates and notifications
- **Cache**: Redis and in-memory caching support

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   API Layer     │───▶│  Service Layer   │───▶│ Repository Layer│
│  (Axum/Tokio)   │    │  (Business Logic)│    │  (PostgreSQL)   │
└─────────────────┘    └──────────────────┘    └─────────────────┘
        │                       │                       │
        ▼                       ▼                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│  WebSocket      │    │   Plugin System  │    │   Redis Cache   │
│  (Real-time)    │    │ (Payments, Ship, │    │   (Optional)    │
│                 │    │  Storage, etc.)  │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## Quick Start

### Prerequisites

- Rust 1.70+
- PostgreSQL 14+
- (Optional) Redis 7+ for caching

### Building

```bash
# Build all crates
cargo build --workspace

# Build release optimized binary
cargo build --release

# Run tests
cargo test --workspace

# Check without building
cargo check --workspace
```

### Configuration

Create a `config.toml` file:

```toml
[server]
host = "0.0.0.0"
port = 8080

[database]
db_type = "Postgres"
host = "localhost"
port = 5432
database = "rcommerce"
username = "rcommerce"
password = "yourpassword"
pool_size = 20

[logging]
level = "info"
format = "Json"

[media]
storage_type = "Local"
local_path = "./uploads"

[cache]
cache_type = "Redis"  # Or "Memory" for in-memory
redis_url = "redis://localhost:6379"
max_size_mb = 100
```

Run with:

```bash
export RCOMMERCE_CONFIG=./config.toml
cargo run --bin rcommerce server
```

## Database Setup

### PostgreSQL (Required)

```bash
# Create database
createdb rcommerce

# Run migrations via CLI
rcommerce db migrate -c config.toml

# Or seed with demo data
rcommerce db seed -c config.toml
```

### Schema

Core tables:
- `products`, `product_variants`, `product_images`, `product_attributes`
- `customers`, `addresses`, `customer_groups`
- `orders`, `order_items`, `fulfillments`, `order_notes`
- `payments`, `payment_methods`
- `audit_logs`
- `api_keys`
- `jobs` (background job processing)

## API Authentication

### JWT Authentication

R Commerce uses JWT tokens for user authentication.

```bash
# Register
POST /api/v1/auth/register
{
  "email": "user@example.com",
  "password": "securepassword123",
  "first_name": "John",
  "last_name": "Doe"
}

# Login
POST /api/v1/auth/login
{
  "email": "user@example.com",
  "password": "securepassword123"
}
# Response: { "access_token": "...", "refresh_token": "..." }

# Use token
GET /api/v1/customers
Authorization: Bearer <access_token>
```

### API Key Management (CLI)

```bash
# Create API key
rcommerce api-key create -c config.toml --name "My App" --scopes "read,write"

# List keys
rcommerce api-key list -c config.toml

# Revoke key
rcommerce api-key revoke -c config.toml <prefix>

# Delete key permanently
rcommerce api-key delete -c config.toml <prefix>
```

## CLI Reference

The R Commerce CLI provides interactive prompts for common operations:

### Interactive Product Creation

```bash
rcommerce product create -c config.toml
```

This will prompt for:
- Product title and URL slug
- Product type (Simple/Variable/Digital/Bundle)
- Price and currency
- SKU and inventory quantity
- Description
- Active/featured status

### Interactive Customer Creation

```bash
rcommerce customer create -c config.toml
```

This will prompt for:
- Email address
- First and last name
- Phone number (optional)
- Preferred currency
- Marketing consent
- Password with confirmation

### Database Operations

```bash
# Run migrations
rcommerce db migrate -c config.toml

# Check database status
rcommerce db status -c config.toml

# Reset database (with confirmation)
rcommerce db reset -c config.toml

# Seed with demo data
rcommerce db seed -c config.toml
```

### Product Management

```bash
# List all products
rcommerce product list -c config.toml

# Get product details
rcommerce product get -c config.toml <product-id>

# Delete product (with confirmation)
rcommerce product delete -c config.toml <product-id>
```

### Order Management

```bash
# List all orders
rcommerce order list -c config.toml
```

### Customer Management

```bash
# List all customers
rcommerce customer list -c config.toml
```

## Plugin System

### Payment Gateways
- Stripe (implemented)
- Airwallex (implemented)
- Alipay (implemented)
- WeChat Pay (implemented)
- Mock gateway (testing)
- PayPal (planned)

### Shipping Providers
- ShipStation (implemented)
- EasyPost (implemented)
- Dianxiaomi ERP (planned)

### Storage Backends
- Local filesystem (implemented)
- S3-compatible / AWS / MinIO (configurable; upload implementation pending)
- Google Cloud Storage (planned)
- Azure Blob Storage (planned)

## Performance Targets

| Metric | Target |
|--------|--------|
| Binary Size | ~20MB (release build) |
| Memory Usage | 10-50MB runtime |
| API Response | Sub-10ms average |
| Concurrent Users | 10,000+ per instance |
| Startup Time | < 1 second |

## Testing

```bash
# Unit tests
cargo test --lib

# Integration tests (require test DB)
cargo test --test 'integration'

# Run with test coverage
cargo tarpaulin --workspace
```

## Roadmap

### Phase 0: Foundation
- Core data models
- Repository pattern
- Configuration system
- Error handling
- Database migrations
- Basic repositories

### Phase 1: MVP
- API server setup (Axum)
- Product CRUD endpoints
- Customer management
- Order lifecycle
- Basic authentication
- Interactive CLI
- API key management

### Phase 2: Core E-Commerce (in progress)
- Payment integration
- Tax calculation
- Email notifications
- Inventory management
- Shipping integration (in progress)
- Background job processing (pending)

### Phase 3: Production Hardening & Security
- TLS / security middleware
- Rate limiting
- WebSocket support
- Redis caching
- Background jobs
- Multi-tenancy, search, and analytics (planned)

## Documentation

Full documentation is available at **[docs.rcommerce.app](https://docs.rcommerce.app)**.

- [API Reference](https://docs.rcommerce.app/api/)
- [Architecture](https://docs.rcommerce.app/architecture/)
- [Deployment](https://docs.rcommerce.app/deployment/)
- [Plugins](https://docs.rcommerce.app/plugins/)

## Deployment

### Deployment Guides

Platform-specific deployment guidance is documented in `docs/deployment/`:

- **Linux**: Systemd service guide
- **FreeBSD**: rc.d guidance
- **macOS**: LaunchDaemon configuration guide
- **Kubernetes**: Coming soon

### Quick Local Start

```bash
git clone https://github.com/PDG-Global/rcommerce-core.git
cd rcommerce-core

# Build and run locally (requires PostgreSQL)
export RCOMMERCE_CONFIG=./config.toml
cargo run --bin rcommerce server
```

See [deployment documentation](docs/deployment/) for detailed platform-specific guides.

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

Quick start:
1. Read the [CLA](CLA.md) - all contributors must sign
2. Fork the repository
3. Create feature branch (`git checkout -b feature/amazing-feature`)
4. Commit changes (`git commit -m 'Add amazing feature'`)
5. Push to branch (`git push origin feature/amazing-feature`)
6. Open Pull Request

## License

R Commerce is dual-licensed. The source code in this public repository is available under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.

- **[AGPL-3.0](LICENSE)** - Free for open source and non-commercial use
- **[Commercial License](https://rcommerce.app/licensing)** - For proprietary use, premium support, and white-label rights

See [LICENSE.md](LICENSE.md) for details.

### Quick License Guide

| Use Case | License Required |
|----------|-----------------|
| Personal projects | AGPL-3.0 (free) |
| Open source SaaS | AGPL-3.0 (free) |
| Commercial SaaS with modifications kept private | Commercial License |
| Enterprise with premium support | Commercial License |
| White-label/OEM | Commercial License |

Contact [licensing@rcommerce.app](mailto:licensing@rcommerce.app) for commercial licensing.

## Support

- GitHub Issues: [Issue Tracker](https://github.com/PDG-Global/rcommerce-core/issues)
- Documentation: [docs.rcommerce.app](https://docs.rcommerce.app)
- Email: support@rcommerce.app

---

**R Commerce** - Headless e-commerce redefined with Rust performance and reliability.
