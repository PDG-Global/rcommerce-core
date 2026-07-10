# Architecture Documentation

This section provides in-depth technical documentation on R Commerce's architecture and design decisions.

## Overview

R Commerce is built with a modular, API-first architecture designed for performance, reliability, and ease of deployment.

## Architecture Documents

| Document | Description |
|----------|-------------|
| [01-overview.md](01-overview.md) | Architectural vision, why Rust, why headless |
| [02-data-modeling.md](02-data-modeling.md) | Core data models and database schema |
| [04-database-abstraction.md](04-database-abstraction.md) | Database layer and repository pattern |
| [05-payment-architecture.md](05-payment-architecture.md) | Payment gateway integration design |
| [06-shipping-integration.md](06-shipping-integration.md) | Shipping provider architecture |
| [07-order-management.md](07-order-management.md) | Order lifecycle and state management |
| [08-compatibility-layer.md](08-compatibility-layer.md) | Platform compatibility and migration |
| [09-product-types-and-subscriptions.md](09-product-types-and-subscriptions.md) | Product types and subscription handling |
| [09-media-storage.md](09-media-storage.md) | Media file storage and CDN integration |
| [10-notifications.md](10-notifications.md) | Email, SMS, and webhook notifications |
| [11-dunning-payment-retry.md](11-dunning-payment-retry.md) | Failed payment retry logic |
| [12-redis-caching.md](12-redis-caching.md) | Caching strategies with Redis |

## Design Principles

1. **Modularity** - Plugin-based architecture
2. **Configuration as Code** - TOML/JSON configuration
3. **API Completeness** - 100% functionality via APIs
4. **Database** - PostgreSQL support
5. **Observability** - Built-in logging and metrics

## Related Documentation

- [API Documentation](../api/index.md) - REST/GraphQL API reference
- [Deployment Guides](../deployment/index.md) - Installation and deployment
- [Development Guide](../development/developer-guide.md) - Contributing and development
