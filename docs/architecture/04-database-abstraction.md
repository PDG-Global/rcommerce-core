# Database Abstraction Layer

## Overview

The Database Abstraction Layer (DAL) provides a unified interface for interacting with multiple database backends, allowing R commerce to support different database systems without changing application code. This enables operators to choose the database that best fits their operational requirements, scaling needs, and existing infrastructure.

**Supported Database:**
- PostgreSQL (Recommended for production)

## Design Goals

### 1. **Unified Interface**
- Single API for all database operations
- Consistent error handling across backends
- Type-safe queries with compile-time checking

### 2. **Performance Optimization**
- Backend-specific query optimization
n- Connection pooling for each database
- Prepared statement caching

### 3. **Migration Support**
- Zero-downtime schema changes
- Version-controlled migrations
- Rollback capabilities

### 4. **Feature Detection**
- Runtime capability detection
- Graceful degradation for missing features
- Clear error messages for unsupported operations

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│           (Services, API Handlers, Background Jobs)          │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│              Repository Pattern Layer                        │
│  OrderRepository, ProductRepository, CustomerRepository    │
│                                                            │
│  Trait Definitions:                                        │
│  - Create, Read, Update, Delete operations                 │
│  - Complex query methods                                   │
│  - Transaction support                                     │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│              Database Abstraction Trait                      │
│  - Unified query builder interface                         │
│  - Transaction management                                  │
│  - Migration runner                                        │
│  - Connection pooling                                      │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│              Database-Specific Implementations             │
│  - PostgreSQL (sqlx::Postgres)                           │

└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│              Database Connection Pool                        │
│  - deadpool or bb8 for connection management               │
└─────────────────────────────────────────────────────────────┘
```

## Technology Stack

### SQLx (Primary)

**Why SQLx?**
- Compile-time query checking
- Async-first design
- Type-safe query results
- Supports PostgreSQL
- Runtime database selection

**Key Features:**
```rust
// Compile-time checked queries
let order = sqlx::query_as!(Order, 
    "SELECT * FROM orders WHERE id = $1", 
    order_id
)
.fetch_optional(pool)
.await?;

// Dynamic queries with query builder
let mut query = QueryBuilder::new("SELECT * FROM orders");
if let Some(status) = filter.status {
    query.push(" WHERE status = ").push_bind(status);
}
```

### Migration Tooling

**Custom Migration System:**
```rust
// Migration trait
pub trait Migration {
    fn version(&self) -> &str;
    fn up(&self, conn: &mut dyn DatabaseConnection) -> Result<()>;
    fn down(&self, conn: &mut dyn DatabaseConnection) -> Result<()>;
}

// Usage
pub struct CreateOrdersTable;
impl Migration for CreateOrdersTable {
    fn version(&self) -> &str { "001" }
    
    fn up(&self, conn: &mut dyn DatabaseConnection) -> Result<()> {
        conn.execute("
            CREATE TABLE orders (
                id UUID PRIMARY KEY,
                order_number VARCHAR(32) UNIQUE NOT NULL,
                customer_id UUID NOT NULL,
                status VARCHAR(20) NOT NULL,
                total_amount DECIMAL(10,2) NOT NULL,
                currency VARCHAR(3) NOT NULL,
                created_at TIMESTAMP NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMP NOT NULL DEFAULT NOW()
            )
        ")
    }
}
```

## Core Data Models

### Order Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub order_number: String,
    pub customer_id: Uuid,
    pub status: OrderStatus,
    pub subtotal: Decimal,
    pub tax_amount: Decimal,
    pub shipping_amount: Decimal,
    pub discount_amount: Decimal,
    pub total: Decimal,
    pub currency: String,
    pub billing_address: Address,
    pub shipping_address: Address,
    pub line_items: Vec<LineItem>,
    pub fulfillments: Vec<Fulfillment>,
    pub notes: Vec<OrderNote>,
    pub meta_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "order_status", rename_all = "snake_case")]
pub enum OrderStatus {
    Pending,
    Confirmed,
    Processing,
    OnHold,
    Completed,
    Cancelled,
    Refunded,
    FraudReview,
}
```

### Product Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub sku: Option<String>,
    pub price: Decimal,
    pub compare_at_price: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub currency: String,
    pub inventory_quantity: i32,
    pub inventory_policy: InventoryPolicy,
    pub weight: Option<f64>,
    pub weight_unit: Option<String>,
    pub status: ProductStatus,
    pub category_id: Option<Uuid>,
    pub images: Vec<ProductImage>,
    pub variants: Vec<ProductVariant>,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub meta_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "inventory_policy", rename_all = "snake_case")]
pub enum InventoryPolicy {
    DenyWhenOversold,  // Don't allow purchasing when out of stock
    ContinueSelling,   // Allow backorders
}
```

### Customer Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    pub id: Uuid,
    pub email: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone: Option<String>,
    pub accepts_marketing: bool,
    pub default_address_id: Option<Uuid>,
    pub addresses: Vec<Address>,
    pub meta_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_order_at: Option<DateTime<Utc>>,
    pub orders_count: i32,
    pub total_spent: Decimal,
}
```

## Repository Pattern Implementation

### Repository Trait

```rust
#[async_trait]
pub trait Repository<T, ID>: Send + Sync {
    async fn find_by_id(&self, id: ID) -> Result<Option<T>>;
    async fn find_all(&self, filter: Filter) -> Result<Vec<T>>;
    async fn create(&self, entity: T) -> Result<T>;
    async fn update(&self, id: ID, entity: T) -> Result<T>;
    async fn delete(&self, id: ID) -> Result<bool>;
}
```

### Available Repositories

R Commerce provides the following repositories for database operations:

| Repository | Purpose | Key Methods |
|------------|---------|-------------|
| `ProductRepository` | Product catalog management | `find_by_slug`, `update_inventory`, `list_with_filter` |
| `CustomerRepository` | Customer account management | `find_by_email`, `update_password`, `add_address` |
| `OrderRepository` | Order lifecycle management | `find_by_order_number`, `update_status`, `list_with_items` |
| `CartRepository` | Shopping cart operations | `find_by_customer`, `add_item`, `apply_coupon` |
| `CouponRepository` | Discount code management | `validate_code`, `increment_usage`, `find_active` |
| `SubscriptionRepository` | Subscription billing | `find_active_by_customer`, `renew`, `cancel` |
| `ApiKeyRepository` | API key management | `validate_key`, `revoke_key`, `list_by_customer` |
| `InventoryRepository` | Stock tracking & reservations | `get_inventory_level`, `create_reservation`, `adjust_stock` |
| `FulfillmentRepository` | Order fulfillment (shipping) | `create`, `update_tracking`, `mark_shipped`, `mark_delivered` |
| `NotificationRepository` | Notification delivery tracking | `create`, `get_pending`, `mark_delivered`, `get_retryable` |
| `CategoryRepository` | Product category management | `get_tree`, `assign_product`, `get_children` |
| `TagRepository` | Product tag management | `get_or_create`, `bulk_assign_to_product`, `get_popular` |
| `StatisticsRepository` | Analytics & reporting | `get_sales_summary`, `get_dashboard_metrics` |

### Repository Examples

#### Order Repository

```rust
#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Order>>;
    async fn find_by_order_number(&self, order_number: &str) -> Result<Option<Order>>;
    async fn find_by_customer(&self, customer_id: Uuid, limit: i64) -> Result<Vec<Order>>;
    async fn find_by_status(&self, status: OrderStatus) -> Result<Vec<Order>>;
    async fn create(&self, order: &Order) -> Result<Order>;
    async fn update(&self, order: &Order) -> Result<Order>;
    async fn update_status(&self, id: Uuid, status: OrderStatus) -> Result<Order>;
    async fn delete(&self, id: Uuid) -> Result<bool>;
    async fn find_with_items(&self, id: Uuid) -> Result<Option<OrderWithItems>>;
    
    // Complex query for order analytics
    async fn get_order_statistics(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<OrderStatistics>;
}
```

#### Inventory Repository

```rust
#[async_trait]
pub trait InventoryRepository: Send + Sync {
    /// Get inventory level for a product
    async fn get_inventory_level(
        &self,
        product_id: Uuid,
        location_id: Option<Uuid>,
    ) -> Result<Option<InventoryLevel>>;
    
    /// Update inventory level
    async fn update_inventory_level(&self, level: &InventoryLevel) -> Result<InventoryLevel>;
    
    /// Create inventory reservation for an order
    async fn create_reservation(&self, reservation: &StockReservation) -> Result<StockReservation>;
    
    /// Release a reservation
    async fn release_reservation(&self, id: Uuid) -> Result<StockReservation>;
    
    /// Adjust stock quantity
    async fn adjust_stock(
        &self,
        product_id: Uuid,
        location_id: Uuid,
        adjustment: i32,
        reason: &str,
    ) -> Result<InventoryLevel>;
    
    /// Record stock movement
    async fn record_movement(&self, movement: &StockMovement) -> Result<StockMovement>;
}
```

#### Category Repository

```rust
#[async_trait]
pub trait CategoryRepository: Send + Sync {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProductCategory>>;
    async fn get_by_slug(&self, slug: &str) -> Result<Option<ProductCategory>>;
    async fn create(&self, category: &ProductCategory) -> Result<ProductCategory>;
    async fn update(&self, category: &ProductCategory) -> Result<ProductCategory>;
    async fn delete(&self, id: Uuid) -> Result<bool>;
    
    /// Get hierarchical category tree
    async fn get_tree(&self, root_id: Option<Uuid>) -> Result<Vec<CategoryTreeNode>>;
    
    /// Get child categories
    async fn get_children(&self, parent_id: Uuid) -> Result<Vec<ProductCategory>>;
    
    /// Assign product to category
    async fn assign_product(&self, product_id: Uuid, category_id: Uuid) -> Result<()>;
    
    /// Get products in category
    async fn get_products(&self, category_id: Uuid, limit: i64, offset: i64) -> Result<Vec<Product>>;
}
```

#### Notification Repository

```rust
#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn create(&self, notification: &Notification) -> Result<Notification>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Notification>>;
    async fn update_status(&self, id: Uuid, status: DeliveryStatus) -> Result<Notification>;
    
    /// Get pending notifications ready to be sent
    async fn get_pending(&self, limit: i64) -> Result<Vec<Notification>>;
    
    /// Get scheduled notifications that are due
    async fn get_due(&self, limit: i64) -> Result<Vec<Notification>>;
    
    /// Get failed notifications that should be retried
    async fn get_retryable(&self, limit: i64) -> Result<Vec<Notification>>;
    
    /// Mark notification as delivered
    async fn mark_delivered(&self, id: Uuid) -> Result<Notification>;
    
    /// Mark notification as failed
    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<Notification>;
    
    /// Clean up old delivered notifications
    async fn cleanup_old(&self, before: DateTime<Utc>) -> Result<u64>;
}
```

### PostgreSQL Implementation

```rust
pub struct PgOrderRepository {
    pool: PgPool,
}

#[async_trait]
impl OrderRepository for PgOrderRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Order>> {
        let order = sqlx::query_as::<_, Order>(
            r#"
            SELECT 
                id, order_number, customer_id, status, subtotal, tax_amount,
                shipping_amount, discount_amount, total, currency,
                billing_address, shipping_address, meta_data,
                created_at, updated_at
            FROM orders 
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(order)
    }
    
    async fn create(&self, order: Order) -> Result<Order> {
        let mut tx = self.pool.begin().await?;
        
        // Insert order
        let order = sqlx::query_as::<_, Order>(
            r#"
            INSERT INTO orders (
                id, order_number, customer_id, status, subtotal, tax_amount,
                shipping_amount, discount_amount, total, currency,
                billing_address, shipping_address, meta_data
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING *
            "#
        )
        .bind(order.id)
        .bind(&order.order_number)
        // ... bind all fields
        .fetch_one(&mut *tx)
        .await?;
        
        // Insert line items
        for item in &order.line_items {
            sqlx::query(
                "INSERT INTO order_line_items (...) VALUES (...)"
            )
            .bind(order.id)
            // ... bind item fields
            .execute(&mut *tx)
            .await?;
        }
        
        tx.commit().await?;
        Ok(order)
    }
    
    // ... other methods
}
```

## Database Configuration

### TOML Configuration

```toml
[database]
# Type: "postgres"
type = "postgres"

# Connection settings
host = "localhost"
port = 5432
username = "rcommerce"
password = "secret"
database = "rcommerce_prod"

# Connection Pool
pool_size = 20
max_lifetime = "30min"
idle_timeout = "10min"
connection_timeout = "30s"

[database.postgres]
# PostgreSQL-specific settings
ssl_mode = "prefer"  # disable, prefer, require
application_name = "rcommerce"
```

### Environment Variable Override

```bash
export RCOMMERCE_DATABASE_TYPE=postgres
export RCOMMERCE_DATABASE_HOST=prod-db.example.com
export RCOMMERCE_DATABASE_USERNAME=rcommerce
export RCOMMERCE_DATABASE_PASSWORD=secret
export RCOMMERCE_DATABASE_POOL_SIZE=50
```

## Multi-Database Support Strategies

### 1. **Feature Detection Pattern**

```rust
pub trait DatabaseCapabilities {
    fn supports_json_operations(&self) -> bool;
    fn supports_cte(&self) -> bool;
    fn supports_window_functions(&self) -> bool;
    fn max_identifier_length(&self) -> usize;
}

// Usage in repository
pub async fn find_with_stats(&self, id: Uuid) -> Result<OrderWithStats> {
    if self.db.capabilities().supports_window_functions() {
        self.find_with_stats_using_window(id).await
    } else {
        self.find_with_stats_manually(id).await
    }
}
```

### 2. **Backend-Specific Optimizations**

```rust
// PostgreSQL: Use JSONB operations
pub async fn update_meta_json(&self, id: Uuid, path: &str, value: serde_json::Value) -> Result<()> {
    sqlx::query("UPDATE orders SET meta_data = jsonb_set(meta_data, $1, $2) WHERE id = $3")
        .bind(path)
        .bind(value)
        .bind(id)
        .execute(&self.pool)
        .await?;
    Ok(())
}

```

### 3. **Query Builder Abstraction**

```rust
pub struct QueryBuilder {
    db_type: DatabaseType,
    sql: String,
    bindings: Vec<serde_json::Value>,
}

impl QueryBuilder {
    pub fn for_db(db_type: DatabaseType) -> Self {
        Self {
            db_type,
            sql: String::new(),
            bindings: vec![],
        }
    }
    
    pub fn push(&mut self, sql: &str) -> &mut Self {
        self.sql.push_str(sql);
        self
    }
    
    pub fn push_bind<T: ToSql>(&mut self, value: T) -> &mut Self {
        self.bindings.push(json!(value));
        match self.db_type {
            DatabaseType::Postgres => self.sql.push_str(&format!("${}", self.bindings.len())),

        }
        self
    }
    
    // Additional methods for paging
    pub fn push_pagination(&mut self, limit: i64, offset: i64) -> &mut Self {
        match self.db_type {
            DatabaseType::Postgres => {
                self.push(" LIMIT ").push_bind(limit);
                self.push(" OFFSET ").push_bind(offset);
            }

        }
        self
    }
}
```

## Performance Considerations

### 1. **Connection Pooling**

```rust
use deadpool_postgres::{Config, Pool, Runtime};

pub async fn create_pool(config: &DatabaseConfig) -> Result<Pool> {
    let mut cfg = Config::new();
    cfg.host = Some(config.host.clone());
    cfg.port = Some(config.port);
    cfg.user = Some(config.username.clone());
    cfg.password = Some(config.password.clone());
    cfg.dbname = Some(config.database.clone());
    
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
    pool.resize(config.pool_size);
    
    Ok(pool)
}
```

### 2. **Read Replicas**

```toml
[database]
type = "postgres"

[database.primary]
host = "primary.db.example.com"
role = "write"

[[database.replicas]]
host = "replica1.db.example.com"
role = "read"
weight = 80

[[database.replicas]]
host = "replica2.db.example.com"
role = "read"
weight = 20
```

```rust
pub struct OrderRepository {
    writer: PgPool,   // Primary connection for writes
    reader: PgPool,   // Replica connection for reads
}

impl OrderRepository {
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Order>> {
        // Read from replica
        self.reader.query_as(...)
    }
    
    pub async fn create(&self, order: Order) -> Result<Order> {
        // Write to primary
        self.writer.query_as(...)
    }
}
```

### 3. **Query Optimization**

**Indexing Strategy:**
```sql
-- orders table indexes
CREATE INDEX idx_orders_customer_id ON orders(customer_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created_at ON orders(created_at DESC);
CREATE INDEX idx_orders_order_number ON orders(order_number);

-- Composite indexes for common queries
CREATE INDEX idx_orders_customer_status ON orders(customer_id, status);
CREATE INDEX idx_orders_date_status ON orders(created_at DESC, status);

-- Partial indexes for common filters
CREATE INDEX idx_orders_pending ON orders(id) WHERE status = 'pending';
```

**N+1 Query Prevention:**
```rust
// BAD: N+1 queries
for order in orders {
    let items = get_line_items(order.id).await?; // N queries
}

// GOOD: Single query with JOIN
let orders_with_items = sqlx::query_as::<_, OrderWithItems>(
    r#"
    SELECT o.*, json_agg(li) as line_items
    FROM orders o
    LEFT JOIN order_line_items li ON o.id = li.order_id
    WHERE o.id = ANY($1)
    GROUP BY o.id
    "#
)
.bind(&order_ids)
.fetch_all(&pool)
.await?;
```

## Testing Strategy

### 1. **Integration Tests with Docker**

```rust
#[cfg(test)]
mod integration_tests {
    use testcontainers::clients::Cli;
    use testcontainers::images::postgres::Postgres;
    
    #[tokio::test]
    async fn test_with_real_postgres() {
        let docker = Cli::default();
        let postgres = docker.run(Postgres::default());
        
        let port = postgres.get_host_port_ipv4(5432);
        let dsn = format!("postgres://postgres@localhost:{}/test", port);
        let pool = PgPool::connect(&dsn).await.unwrap();
        
        // Run tests...
    }
}
```

## Migration Management

### Command-Line Interface

```bash
# Create new migration
rcommerce migrate create add_customer_note_field

# Run pending migrations
rcommerce migrate run

# Rollback last migration
rcommerce migrate rollback

# Check migration status
rcommerce migrate status

# Generate migration from entity changes (future)
rcommerce migrate generate --from-entities
```

### Migration Files

R Commerce uses SQL migrations with automatic versioning:

```
crates/rcommerce-core/migrations/
├── 001_complete_schema.sql      # Core tables (products, orders, customers)
├── 002_tax_system.sql           # Tax calculation tables
└── 003_inventory_notifications_fulfillment.sql  # Inventory, notifications, webhooks
```

### Core Tables

#### Commerce Core
- `products` - Product catalog
- `product_variants` - Product variations (size, color)
- `product_categories` - Hierarchical categories
- `product_tags` - Product labels
- `customers` - Customer accounts
- `orders` - Order headers
- `order_items` - Order line items
- `carts` - Shopping carts
- `cart_items` - Cart line items
- `coupons` - Discount codes

#### Inventory Management
- `inventory_locations` - Warehouses and stores
- `inventory_levels` - Stock quantities per location
- `stock_reservations` - Reserved stock for orders
- `stock_movements` - Stock change audit trail

#### Payment & Subscriptions
- `payments` - Payment transactions
- `refunds` - Refund records
- `subscriptions` - Subscription billing
- `subscription_invoices` - Subscription invoices
- `payment_retry_attempts` - Failed payment retries
- `dunning_emails` - Dunning campaign emails

#### Notifications & Webhooks
- `notifications` - Email/SMS/push queue
- `notification_templates` - Reusable templates
- `webhooks` - Outgoing webhook configuration
- `webhook_deliveries` - Webhook delivery log
- `customer_notification_preferences` - Opt-in settings

#### Shipping
- `fulfillments` - Shipment records
- `fulfillment_items` - Items in each shipment
- `shipping_carrier_configs` - Carrier API settings
- `shipping_rates_cache` - Cached shipping quotes

### Migration Example

```sql
-- 003_inventory_notifications_fulfillment.sql
-- Inventory locations
CREATE TABLE IF NOT EXISTS inventory_locations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    code VARCHAR(50) UNIQUE NOT NULL,
    address JSONB,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Notifications queue
CREATE TABLE IF NOT EXISTS notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    channel notification_channel NOT NULL,
    recipient VARCHAR(255) NOT NULL,
    subject VARCHAR(500) NOT NULL,
    body TEXT NOT NULL,
    status delivery_status NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    scheduled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Webhooks
CREATE TABLE IF NOT EXISTS webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    url VARCHAR(500) NOT NULL,
    secret VARCHAR(255) NOT NULL,
    events VARCHAR(100)[] NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_triggered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

Next: [05-payment-architecture.md](05-payment-architecture.md) - Payment integration system
