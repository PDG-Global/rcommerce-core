# API Documentation

This section contains comprehensive documentation for the R Commerce REST and GraphQL APIs.

## Overview

R Commerce provides a complete API-first ecommerce platform with:

- **REST API** - Traditional HTTP endpoints for all operations
- **GraphQL API** - Flexible query language for efficient data fetching
- **Webhooks** - Real-time event notifications
- **Authentication** - JWT and API key-based security

## Quick Reference

| Document | Description |
|----------|-------------|
| [01-api-design.md](01-api-design.md) | API design principles, REST/GraphQL specifications |
| [02-error-codes.md](02-error-codes.md) | Complete error code reference |
| [03-cart-api.md](03-cart-api.md) | Shopping cart API endpoints |
| [04-coupon-api.md](04-coupon-api.md) | Coupon and discount API |
| [statistics.md](statistics.md) | Statistics and analytics API |

## API Endpoints

### Core Resources

- **Products** - Catalog management, variants, inventory
- **Orders** - Order lifecycle, fulfillment, refunds
- **Customers** - Customer accounts, addresses, groups
- **Cart** - Shopping cart operations
- **Coupons** - Discount codes and promotions
- **Payments** - Payment processing and refunds
- **Statistics** - Analytics, reporting, and metrics

### Authentication

All API requests require authentication via:
- **JWT Token** - For customer/user sessions
- **API Key** - For server-to-server integration

See [Authentication](../development/cli-reference.md#api-key-management) for details.

## Getting Started

1. Review the [API Design](01-api-design.md) document
2. Check [Error Codes](02-error-codes.md) for response handling
3. Set up [Authentication](../development/cli-reference.md#api-key-management)
4. Explore specific endpoint documentation

## External References

- [API Reference (Website)](https://github.com/creativebastard/rcommerce)
- [Postman Collection](https://github.com/creativebastard/rcommerce/tree/main/docs/postman)
