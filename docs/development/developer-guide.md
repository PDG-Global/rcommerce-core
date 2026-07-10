# Developer Guide

This guide helps developers understand how to set up, develop, test, and contribute to R commerce.

## Prerequisites

- **Rust 1.70+** (install from [rustup.rs](https://rustup.rs/))
- **PostgreSQL 13+**
- **Redis 6+** (optional, for caching)
- **Node.js 16+** (for frontend tooling, optional)

### Install Rust

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add Rust to PATH
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version

# Install helpful tools
cargo install cargo-watch
cargo install cargo-edit
cargo install cargo-audit
cargo install cargo-outdated
```

## Development Environment Setup

### 1. Clone Repository

```bash
git clone https://github.com/creativebastard/rcommerce.git
cd gocart
```

### 2. Database Setup

#### PostgreSQL

```bash
# Install PostgreSQL (macOS)
brew install postgresql@15
brew services start postgresql@15

# Install PostgreSQL (Ubuntu)
sudo apt-get update
sudo apt-get install postgresql-15 postgresql-contrib

# Create database
psql -U postgres -c "CREATE DATABASE rcommerce_dev;"
psql -U postgres -c "CREATE USER rcommerce_dev WITH PASSWORD 'devpass';"
psql -U postgres -c "GRANT ALL PRIVILEGES ON DATABASE rcommerce_dev TO rcommerce_dev;"

# Test connection
psql -U rcommerce_dev -d rcommerce_dev -h localhost -W
```

### 3. Redis Setup (Optional)

```bash
# Install Redis
brew install redis  # macOS
sudo apt-get install redis-server  # Ubuntu

# Start Redis
redis-server

# Test Redis
redis-cli ping  # Should return "PONG"
```

### 4. Local Configuration

Create development configuration:

```toml
# config/development.toml
[server]
host = "127.0.0.1"
port = 8080
log_level = "debug"

[database]
type = "postgres"
host = "localhost"
port = 5432
username = "rcommerce_dev"
password = "devpass"
database = "rcommerce_dev"
pool_size = 5

[cache]
provider = "memory"  # Use in-memory cache for development

[payments]
default_gateway = "mock"

[logging]
level = "debug"
format = "text"

[features]
development_mode = true
debug_api = true
```

### 5. Environment Variables

Create `.env` file in project root:

```bash
# .env
DATABASE_URL=postgres://rcommerce_dev:devpass@localhost/rcommerce_dev
REDIS_URL=redis://localhost:6379
RUST_LOG=debug
RUST_BACKTRACE=1

# API Keys for testing
STRIPE_TEST_SECRET_KEY=sk_test_your_key_here
STRIPE_TEST_WEBHOOK_SECRET=whsec_your_secret_here

# Development mode
RCOMMERCE_ENV=development
```

## Build and Run

### Compile and Run

```bash
# Build debug version
cargo build

# Run with automatic restart on changes (using cargo-watch)
cargo watch -x run

# Or manually run after changes
cargo run

# Run with specific config
cargo run -- --config config/development.toml

# Run with environment variables
RUST_LOG=debug cargo run
```

### Build Release Version

```bash
# Build optimized release version
cargo build --release

# Release binary location
target/release/rcommerce
```

### Cross-Compilation

```bash
# For different platforms
cargo build --target x86_64-unknown-linux-gnu
cargo build --target x86_64-pc-windows-gnu
cargo build --target aarch64-apple-darwin
```

## Project Structure

```
gocart/
├── src/
│   ├── api/              # HTTP API layer (handlers, middleware)
│   │   ├── mod.rs
│   │   ├── handlers.rs
│   │   └── middleware/
│   ├── models/           # Data models (entities)
│   │   ├── mod.rs
│   │   ├── product.rs
│   │   ├── order.rs
│   │   └── customer.rs
│   ├── services/         # Business logic services
│   │   ├── mod.rs
│   │   ├── product_service.rs
│   │   ├── order_service.rs
│   │   └── customer_service.rs
│   ├── db/               # Database layer
│   │   ├── mod.rs
│   │   ├── connection.rs
│   │   ├── migrations/
│   │   └── repositories/
│   ├── payments/         # Payment integration
│   │   ├── mod.rs
│   │   ├── gateway.rs
│   │   └── providers/
│   ├── shipping/         # Shipping integration
│   │   ├── mod.rs
│   │   ├── provider.rs
│   │   └── providers/
│   ├── notifications/    # Email/SMS/webhook system
│   └── main.rs           # Application entry point
├── tests/                # Integration tests
├── benches/              # Benchmarks
├── migrations/           # Database migrations
├── config/               # Configuration files
│   ├── development.toml
│   ├── production.toml
│   └── test.toml
├── Cargo.toml           # Rust project manifest
└── README.md
```

## Development Workflow

### 1. Feature Development

```bash
# Create feature branch
git checkout -b feature/new-feature-name

# Create migration for database changes
cargo sqlx migrate add create_new_table

# Edit migration file in migrations/

# Apply migration
cargo sqlx migrate run

# Develop feature
# Edit source files...

# Run tests frequently
cargo test

# Check code formatting and linting
cargo fmt --check
cargo clippy

# Build to ensure no compilation errors
cargo build

# Commit changes
git add .
git commit -m "feat: descriptive commit message"
```

### 2. Database Migrations

```bash
# Create new migration
cargo sqlx migrate add add_users_table

# Edit the generated migration file
# migrations/YYYYMMDDHHMMSS_add_users_table.sql

# Run migrations
cargo sqlx migrate run

# Check migration status
cargo sqlx migrate info

# Revert last migration
cargo sqlx migrate revert

# Run migrations in test database
cargo sqlx migrate run --database-url "postgres://test:test@localhost/testdb"
```

### 3. Testing Your Changes

#### Unit Tests

```rust
// tests/unit/order_service_test.rs
use rcommerce::services::OrderService;
use rcommerce::models::Order;

#[tokio::test]
async fn test_create_order() {
    // Arrange
    let order_service = OrderService::new(/* ... */);
    let order_input = CreateOrderInput {
        customer_id: Uuid::new_v4(),
        line_items: vec![
            LineItemInput {
                product_id: Uuid::new_v4(),
                quantity: 2,
            }
        ],
    };
    
    // Act
    let result = order_service.create_order(order_input).await;
    
    // Assert
    assert!(result.is_ok());
    let order = result.unwrap();
    assert_eq!(order.status, OrderStatus::Pending);
}
```

Run unit tests:
```bash
# Run all tests
cargo test

# Run specific test
cargo test test_create_order

# Run with output
cargo test -- --nocapture

# Run with coverage
cargo tarpaulin --out Html
```

#### Integration Tests

```bash
# Start test database
docker run -d --name test-db -p 5434:5432 -e POSTGRES_PASSWORD=test postgres:15

# Run integration tests
cargo test --test integration

# Integration test example
cargo test test_complete_order_flow -- --nocapture
```

#### API Testing with curl

```bash
# Test health endpoint
curl http://localhost:8080/health

# Create product
curl -X POST http://localhost:8080/v1/products \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test Product",
    "price": 29.99,
    "status": "active"
  }'

# Get products
curl http://localhost:8080/v1/products \
  -H "Authorization: Bearer YOUR_API_KEY"
```

### 4. Debugging

```bash
# Enable debug logging
RUST_LOG=debug cargo run

# Enable backtrace on panic
RUST_BACKTRACE=1 cargo run

# Full backtrace
RUST_BACKTRACE=full cargo run

# Specific module logging
RUST_LOG=rcommerce_api=debug,sqlx=warn cargo run

# Use a debugger (lldb/gdb)
rust-lldb ./target/debug/rcommerce
# Or
rust-gdb ./target/debug/rcommerce
```

### 5. Code Formatting and Linting

```bash
# Format code
cargo fmt

# Check formatting (without making changes)
cargo fmt --check

# Run linter
cargo clippy

# Run linter with warnings as errors
cargo clippy -- -D warnings

# Auto-fix clippy warnings where possible
cargo clippy --fix
```

## Testing Strategies

### Test Database Setup

```bash
# Create test database
psql -U postgres -c "CREATE DATABASE rcommerce_test;"
psql -U postgres -c "CREATE USER rcommerce_test WITH PASSWORD 'testpass';"
psql -U postgres -c "GRANT ALL PRIVILEGES ON DATABASE rcommerce_test TO rcommerce_test;"

# Set test database URL
export TEST_DATABASE_URL="postgres://rcommerce_test:testpass@localhost/rcommerce_test"
```

### Writing Tests

#### Unit Test Example

```rust
// src/services/product_service.rs

#[cfg(test)]
mod tests {
    use super::*;
    use rcommerce::db::MockDatabase;
    
    #[tokio::test]
    async fn test_create_product_with_valid_data() {
        let db = MockDatabase::new();
        let service = ProductService::new(db);
        
        let input = CreateProductInput {
            name: "Test Product".to_string(),
            price: Decimal::from(29.99),
            sku: Some("TEST-001".to_string()),
        };
        
        let result = service.create_product(input).await;
        assert!(result.is_ok());
        
        let product = result.unwrap();
        assert_eq!(product.name, "Test Product");
        assert_eq!(product.price, Decimal::from(29.99));
        assert!(product.id.to_string().len() > 0);
    }
    
    #[tokio::test]
    async fn test_create_product_with_invalid_price() {
        let db = MockDatabase::new();
        let service = ProductService::new(db);
        
        let input = CreateProductInput {
            name: "Test Product".to_string(),
            price: Decimal::from(-5.00),  // Invalid negative price
            sku: Some("TEST-001".to_string()),
        };
        
        let result = service.create_product(input).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert_eq!(error, ValidationError::InvalidPrice);
    }
}
```

#### Integration Test Example

```rust
// tests/integration/order_flow_test.rs

use rcommerce::api::App;
use rcommerce::models::CreateOrderInput;

#[tokio::test]
async fn complete_order_flow() {
    // Setup
    let app = App::new_for_testing();
    let api_key = app.create_test_api_key();
    
    // 1. Create customer
    let customer = app.create_test_customer().await;
    
    // 2. Create product
    let product = app.create_test_product().await;
    
    // 3. Create order
    let order_input = CreateOrderInput {
        customer_id: customer.id,
        line_items: vec![
            LineItemInput {
                product_id: product.id,
                quantity: 2,
            }
        ],
    };
    
    let order = app.create_order(order_input).await.unwrap();
    assert_eq!(order.status, OrderStatus::Pending);
    assert_eq!(order.line_items.len(), 1);
    
    // 4. Process payment
    let payment_result = app.process_test_payment(order.id).await.unwrap();
    assert_eq!(payment_result.status, PaymentStatus::Paid);
    
    // 5. Fulfill order
    let fulfillment = app.create_fulfillment(order.id).await.unwrap();
    assert_eq!(fulfillment.status, FulfillmentStatus::Pending);
    
    // Cleanup
    app.cleanup().await;
}
```

#### Load Testing

```bash
# Install wrk (HTTP benchmarking tool)
brew install wrk  # macOS
sudo apt-get install wrk  # Ubuntu

# Run load tests
wrk -t12 -c400 -d30s http://localhost:8080/health

# Custom Lua script for complex requests
cat > load_test.lua <<EOF
wrk.method = "POST"
wrk.body = '{"name":"Load Test Product","price":9.99}'
wrk.headers["Authorization"] = "Bearer sk_test_xxx"
wrk.headers["Content-Type"] = "application/json"
EOF

wrk -t12 -c100 -d30s -s load_test.lua http://localhost:8080/v1/products
```

## Debugging Database Issues

### Enable SQL Logging

```rust
// In your config for development
[database]
log_queries = true
log_slow_queries = 1000  # Log queries slower than 1s
```

### Common Database Issues

1. **Connection Pool Exhausted**
   ```bash
   # Increase pool size in config
   pool_size = 50
   ```

2. **Slow Queries**
   ```bash
   # Analyze slow queries
   psql rcommerce_dev -c "SELECT now() - query_start AS duration, query 
   FROM pg_stat_activity WHERE state != 'idle' ORDER BY duration DESC;"
   
   # Add missing indexes
   cargo sqlx prepare
   ```

3. **Deadlocks**
   ```bash
   # Check PostgreSQL logs for deadlocks
   tail -f /usr/local/var/log/postgresql.log
   
   # Use row-level locking where appropriate
   FOR UPDATE
   ```

## Performance Optimization

### 1. Query Optimization

```rust
// Use query builders for dynamic queries
let mut query = QueryBuilder::new("SELECT * FROM products");

if let Some(category) = filter.category {
    query.push(" WHERE category_id = ").push_bind(category);
}

if let Some(search) = filter.search {
    query.push(" AND name ILIKE ").push_bind(format!("%{}%", search));
}

query.push(" ORDER BY created_at DESC");
query.push(" LIMIT ").push_bind(filter.limit.unwrap_or(20));
```

### 2. Connection Pool Tuning

```toml
# config/production.toml
[database]
pool_size = 50                    # Higher for production
max_lifetime = "30min"
idle_timeout = "10min"
connection_timeout = "30s"
```

### 3. Caching Strategy

```rust
// Cache expensive computations
use cached::proc_macro::cached;

#[cached(result = true, size = 100)]
fn calculate_shipping_cost_from_cache(
    from_zip: &str,
    to_zip: &str,
    weight: f64,
) -> Result<Decimal, Error> {
    // Expensive calculation
}
```

### 4. Async Optimization

```rust
// Use concurrent requests where possible
use futures::future::join_all;

async fn fetch_multiple_products(ids: &[Uuid]) -> Vec<Product> {
    let futures: Vec<_> = ids.iter()
        .map(|id| product_repo.find_by_id(*id))
        .collect();
    
    join_all(futures).await
        .into_iter()
        .filter_map(Result::ok)
        .collect()
}
```

## API Development

### Adding a New API Endpoint

```rust
// src/api/handlers/product_handlers.rs

use axum::{extract::{Path, State}, http::StatusCode, Json};
use rcommerce::{models::CreateProductInput, services::ProductService};

pub async fn create_product(
    State(product_service): State<Arc<dyn ProductService>>,
    Json(input): Json<CreateProductInput>,
) -> Result<(StatusCode, Json<Product>), ApiError> {
    // Service call
    let product = product_service.create_product(input).await?;
    
    Ok((StatusCode::CREATED, Json(product)))
}

// Register route in main.rs
use axum::Router;

let app = Router::new()
    .route("/v1/products", post(create_product))
    .with_state(product_service);
```

### Request/Response Validation

```rust
use validator::{Validate, ValidationError};

#[derive(Debug, Validate)]
pub struct CreateProductInput {
    #[validate(length(min = 1, max = 500))]
    pub name: String,
    
    #[validate(range(min = 0))]
    pub price: Decimal,
    
    #[validate(length(min = 1, max = 100))]
    #[validate(custom = "validate_sku")]
    pub sku: Option<String>,
}

fn validate_sku(sku: &str) -> Result<(), ValidationError> {
    if !sku.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(ValidationError::new("SKU must be alphanumeric"));
    }
    Ok(())
}
```

## Error Handling

### Custom Error Types

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Product not found: {0}")]
    ProductNotFound(String),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Invalid input: {0}")]
    ValidationError(String),
    
    #[error("Order cannot be modified in current status: {status}")]
    OrderNotEditable { status: OrderStatus },
    
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::ProductNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::DatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::ValidationError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::OrderNotEditable { .. } => (StatusCode::CONFLICT, self.to_string()),
            ApiError::UnexpectedError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        
        let body = Json(json!({
            "error": {
                "message": error_message,
                "code": self.error_code(),
            }
        }));
        
        (status, body).into_response()
    }
}
```

## Contributing Guidelines

### 1. Code Style

- **Format**: Always run `cargo fmt` before committing
- **Lint**: Address all `cargo clippy` warnings
- **Document**: Add rustdoc comments to public functions
- **Test**: Include tests for new functionality

```rust
/// Creates a new product in the catalog
/// 
/// # Arguments
/// 
/// * `input` - Product creation data including name, price, and inventory
/// 
/// # Returns
/// 
/// Returns the created product with generated ID
/// 
/// # Errors
/// 
/// Returns an error if:
/// - Product with same SKU already exists
/// - Price is negative
/// - Required fields are missing
/// 
/// # Example
/// 
/// ```
/// let input = CreateProductInput {
///     name: "T-Shirt".to_string(),
///     price: 29.99.into(),
///     sku: Some("TSHIRT-001".to_string()),
/// };
/// let product = service.create_product(input).await?;
/// ```
pub async fn create_product(
    &self,
    input: CreateProductInput
) -> Result<Product, ApiError> {
    // Implementation
}
```

### 2. Commit Messages

Follow conventional commits:

```bash
feat: add support for digital products
fix: resolve inventory sync issue with concurrent orders
docs: update API documentation for orders
test: add integration tests for payment flow
refactor: simplify order status transition logic
perf: optimize product search with database indexes
chore: update dependencies
```

### 3. Pull Request Process

1. **Fork the repository**
2. **Create feature branch**: `git checkout -b feature/your-feature`
3. **Make changes**: Implement feature with tests
4. **Add tests**: Ensure all new code has test coverage
5. **Run full test suite**: `cargo test`
6. **Check formatting**: `cargo fmt --check`
7. **Run linter**: `cargo clippy`
8. **Commit changes**: `git commit -m "feat: your feature"`
9. **Push to fork**: `git push origin feature/your-feature`
10. **Create Pull Request**: Include detailed description

### 4. Code Review Guidelines

- **Be constructive**: Suggest improvements, don't just point out issues
- **Be specific**: Reference line numbers and suggest concrete changes
- **Test coverage**: Ensure new code has adequate tests
- **Performance**: Consider performance implications
- **Documentation**: Check that changes are documented

## Release Process

### Versioning

We use semantic versioning:

- **MAJOR**: Breaking changes
- **MINOR**: New features (backwards compatible)
- **PATCH**: Bug fixes (backwards compatible)

```bash
# Update version in Cargo.toml
[package]
version = "0.1.0"

# Create release
git tag -a v0.1.0 -m "Release version 0.1.0"
git push origin v0.1.0
```

### Changelog

Maintain CHANGELOG.md:

```markdown
# Changelog

## [Unreleased]

### Added
- New feature X

### Changed
- Updated behavior Y

### Fixed
- Bug fix Z

## [0.1.0] - 2024-01-23

### Added
- Initial release
```

## Getting Help

### Resources

- **Documentation**: See `docs/` directory
- **API Reference**: `docs/api/01-api-design.md`
- **Architecture**: `docs/architecture/01-overview.md`
- **Discord**: [R commerce Community](https://discord.gg/rcommerce)
- **Issues**: Create GitHub issues for bugs/features
- **Discussions**: GitHub Discussions for questions

### Troubleshooting Common Issues

1. **Compiler Error: async recursion**
   ```rust
   // Use Box::pin for recursive async calls
   async fn recursive_function() {
       Box::pin(async move {
           // Recursion here
       }).await
   }
   ```

2. **Lifetime issues with database connections**
   ```rust
   // Use connection pooling
   let pool = PgPool::connect(&database_url).await?;
   let result = pool.acquire().await?.query_as::<_, Order>(/* ... */).await?;
   ```

3. **Deadlocks with concurrent access**
   ```rust
   // Use proper transaction isolation
   let mut tx = pool.begin().await?;
   tx.execute("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").await?;
   ```

## Performance Profiling

```bash
# Install profiling tools
cargo install flamegraph

# Profile release build
cargo flamegraph --bin rcommerce

# View flamegraph
open flamegraph.svg

# Memory profiling
valgrind --tool=massif ./target/release/rcommerce
cargo install cargo-profiler
```

## Security Guidelines

- **Never commit secrets**: Use `.env` files or environment variables
- **Validate all inputs**: Use `validator` crate
- **Use parameterized queries**: Prevent SQL injection
- **Sanitize output**: Prevent XSS
- **Implement rate limiting**: Prevent abuse
- **CORS configuration**: Restrict origins appropriately

```bash
# Scan for secrets before committing
git-secrets --scan

# Check dependencies for vulnerabilities
cargo audit

# Update dependencies
cargo update
```

This guide covers the essentials of R commerce development. For platform-specific details, see other documentation files in the `docs/` directory.
