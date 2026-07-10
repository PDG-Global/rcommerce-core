# Product Types and Subscriptions

## Overview

R Commerce supports multiple product types to accommodate different business models:

1. **Simple Products** - Single SKU, no variants
2. **Variable Products** - Multiple variants (sizes, colors)
3. **Subscription Products** - Recurring billing
4. **Digital Products** - Downloadable, no shipping
5. **Bundle Products** - Collection of other products

## Product Types

### Simple Product

A basic product with a single SKU and no variants.

```rust
pub struct Product {
    pub id: Uuid,
    pub title: String,
    pub slug: String,
    pub product_type: ProductType::Simple,
    pub price: Decimal,
    pub sku: Option<String>,
    // ... other fields
}
```

**Use Cases:**
- Books
- Electronics with no options
- Single-size consumables
- Digital downloads

### Variable Product

A product with multiple variants based on attributes like size, color, material.

```rust
pub struct Product {
    pub id: Uuid,
    pub title: String,
    pub product_type: ProductType::Variable,
    // ... base product fields
}

pub struct ProductAttribute {
    pub id: Uuid,
    pub product_id: Uuid,
    pub name: String,      // e.g., "Color", "Size"
    pub slug: String,      // e.g., "color", "size"
    pub visible: bool,     // Show on product page
    pub variation: bool,   // Used for creating variations
}

pub struct ProductAttributeValue {
    pub id: Uuid,
    pub attribute_id: Uuid,
    pub value: String,          // e.g., "Red", "XL"
    pub color_code: Option<String>, // Hex code for colors
    pub image_url: Option<String>,  // Swatch image
}

pub struct ProductVariant {
    pub id: Uuid,
    pub product_id: Uuid,
    pub title: String,     // e.g., "T-Shirt - Red / Large"
    pub sku: Option<String>,
    pub price: Decimal,    // Can differ from base product
    pub inventory_quantity: i32,
    // ... other fields
}
```

**Example: T-Shirt with Size and Color**

```json
{
  "title": "Premium Cotton T-Shirt",
  "product_type": "variable",
  "attributes": [
    {
      "name": "Color",
      "slug": "color",
      "values": [
        {"value": "Red", "color_code": "#FF0000"},
        {"value": "Blue", "color_code": "#0000FF"},
        {"value": "Green", "color_code": "#00FF00"}
      ]
    },
    {
      "name": "Size",
      "slug": "size",
      "values": [
        {"value": "Small"},
        {"value": "Medium"},
        {"value": "Large"},
        {"value": "X-Large"}
      ]
    }
  ],
  "variants": [
    {
      "title": "Premium Cotton T-Shirt - Red / Small",
      "sku": "TSHIRT-RED-SM",
      "price": "29.99",
      "attribute_values": ["Red", "Small"]
    },
    // ... 11 more variants (3 colors × 4 sizes)
  ]
}
```

**Use Cases:**
- Clothing (sizes, colors)
- Shoes (sizes, widths, colors)
- Furniture (colors, materials)
- Electronics (storage capacity, colors)

### Subscription Product

A product that creates a recurring billing subscription.

```rust
pub struct Product {
    pub id: Uuid,
    pub title: String,
    pub product_type: ProductType::Subscription,
    pub price: Decimal,              // Price per billing cycle
    pub subscription_interval: SubscriptionInterval,
    pub subscription_interval_count: i32,  // e.g., every 3 months
    pub subscription_trial_days: Option<i32>,
    pub subscription_setup_fee: Option<Decimal>,
    pub subscription_min_cycles: Option<i32>,
    pub subscription_max_cycles: Option<i32>,
}

pub enum SubscriptionInterval {
    Daily,
    Weekly,
    BiWeekly,
    Monthly,
    Quarterly,
    BiAnnually,
    Annually,
}
```

**Example: Monthly Coffee Subscription**

```json
{
  "title": "Premium Coffee Subscription",
  "product_type": "subscription",
  "price": "29.99",
  "currency": "USD",
  "subscription_interval": "monthly",
  "subscription_interval_count": 1,
  "subscription_trial_days": 7,
  "subscription_setup_fee": "5.00",
  "subscription_min_cycles": 3,
  "subscription_max_cycles": null
}
```

**Use Cases:**
- Subscription boxes
- Software licenses
- Membership fees
- Recurring donations
- Magazine/newspaper subscriptions

### Digital Product

A downloadable product with no physical shipping.

```rust
pub struct Product {
    pub id: Uuid,
    pub title: String,
    pub product_type: ProductType::Digital,
    pub price: Decimal,
    pub requires_shipping: false,  // Always false for digital
    // ...
}

pub struct DigitalDownload {
    pub id: Uuid,
    pub product_id: Uuid,
    pub file_url: String,
    pub file_name: String,
    pub file_size: i64,
    pub download_limit: Option<i32>,  // Max downloads per purchase
    pub expiry_days: Option<i32>,     // Link expiry in days
}
```

**Use Cases:**
- E-books
- Software
- Music/Audio files
- Video courses
- Digital art/assets

### Bundle Product

A collection of other products sold together at a discounted price.

```rust
pub struct Product {
    pub id: Uuid,
    pub title: String,
    pub product_type: ProductType::Bundle,
    pub price: Decimal,  // Bundle price (typically discounted)
}

pub struct BundleItem {
    pub id: Uuid,
    pub bundle_id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub quantity: i32,
    pub included: bool,  // Optional item
}
```

**Use Cases:**
- Product kits
- "Buy together and save"
- Starter packs
- Gift sets

## Subscription System

### Subscription Lifecycle

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Pending   │───▶│  Trialing   │───▶│   Active    │
└─────────────┘    └─────────────┘    └─────────────┘
                                              │
                    ┌──────────────────────────┼──────────┐
                    │                          │          │
                    ▼                          ▼          ▼
            ┌─────────────┐           ┌─────────────┐  ┌─────────────┐
            │   Paused    │           │  Past Due   │  │  Cancelled  │
            └─────────────┘           └─────────────┘  └─────────────┘
                                                           │
                                                           ▼
                                                    ┌─────────────┐
                                                    │   Expired   │
                                                    └─────────────┘
```

### Subscription Entity

```rust
pub struct Subscription {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub order_id: Uuid,              // Original order
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    
    // Configuration
    pub status: SubscriptionStatus,
    pub interval: SubscriptionInterval,
    pub interval_count: i32,
    
    // Pricing
    pub currency: Currency,
    pub amount: Decimal,
    pub setup_fee: Option<Decimal>,
    
    // Trial
    pub trial_days: i32,
    pub trial_ends_at: Option<DateTime<Utc>>,
    
    // Billing cycle tracking
    pub current_cycle: i32,
    pub min_cycles: Option<i32>,
    pub max_cycles: Option<i32>,
    
    // Important dates
    pub starts_at: DateTime<Utc>,
    pub next_billing_at: DateTime<Utc>,
    pub last_billing_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    
    // Payment
    pub payment_method_id: Option<String>,
    pub gateway: String,
}
```

### Subscription Invoice

Each billing cycle generates an invoice:

```rust
pub struct SubscriptionInvoice {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub order_id: Option<Uuid>,
    
    pub cycle_number: i32,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub total: Decimal,
    
    pub status: InvoiceStatus,
    pub paid_at: Option<DateTime<Utc>>,
    pub failed_attempts: i32,
}
```

### Payment Gateway Subscription Support

#### Stripe Subscriptions

```rust
impl PaymentGateway for StripeGateway {
    async fn create_subscription(
        &self,
        customer_id: &str,
        price_id: &str,
        interval: SubscriptionInterval,
    ) -> Result<Subscription> {
        // Create Stripe subscription
        let params = stripe::CreateSubscription::new(
            customer_id.to_string(),
        )
        .add_item(stripe::CreateSubscriptionItems {
            price: Some(price_id.to_string()),
            ..Default::default()
        })
        .expand(&["latest_invoice.payment_intent"]);
        
        let subscription = stripe::Subscription::create(&self.client, params).await?;
        
        Ok(Subscription {
            // ... map Stripe subscription to our model
        })
    }
}
```

#### WeChat Pay Subscriptions

WeChat Pay supports recurring payments through their "Entrusted Payment" feature:

```rust
impl PaymentGateway for WeChatPayGateway {
    async fn create_subscription(
        &self,
        contract: SubscriptionContract,
    ) -> Result<Subscription> {
        // Create WeChat Pay contract for recurring payments
        let body = json!({
            "sp_appid": self.app_id,
            "sp_mchid": self.mch_id,
            "contract_display_account": contract.customer_name,
            "plan_id": contract.plan_id,
            "contract_notify_url": self.webhook_url,
        });
        
        // ... API call to create contract
    }
}
```

## Database Schema Additions

### Product Types

```sql
-- Add product type enum
CREATE TYPE product_type AS ENUM ('simple', 'variable', 'subscription', 'digital', 'bundle');

-- Add subscription interval enum
CREATE TYPE subscription_interval AS ENUM (
    'daily', 'weekly', 'bi_weekly', 'monthly', 'quarterly', 'bi_annually', 'annually'
);

-- Alter products table
ALTER TABLE products ADD COLUMN product_type product_type NOT NULL DEFAULT 'simple';
ALTER TABLE products ADD COLUMN subscription_interval subscription_interval;
ALTER TABLE products ADD COLUMN subscription_interval_count INTEGER DEFAULT 1;
ALTER TABLE products ADD COLUMN subscription_trial_days INTEGER DEFAULT 0;
ALTER TABLE products ADD COLUMN subscription_setup_fee DECIMAL(20, 2);
ALTER TABLE products ADD COLUMN subscription_min_cycles INTEGER;
ALTER TABLE products ADD COLUMN subscription_max_cycles INTEGER;

-- Product attributes for variable products
CREATE TABLE product_attributes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    visible BOOLEAN NOT NULL DEFAULT true,
    variation BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_product_attributes_product_id ON product_attributes(product_id);

-- Product attribute values
CREATE TABLE product_attribute_values (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    attribute_id UUID NOT NULL REFERENCES product_attributes(id) ON DELETE CASCADE,
    value VARCHAR(100) NOT NULL,
    color_code VARCHAR(7),
    image_url VARCHAR(500),
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_product_attribute_values_attribute_id ON product_attribute_values(attribute_id);

-- Link variants to attribute values
CREATE TABLE product_variant_attributes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    variant_id UUID NOT NULL REFERENCES product_variants(id) ON DELETE CASCADE,
    attribute_value_id UUID NOT NULL REFERENCES product_attribute_values(id) ON DELETE CASCADE,
    UNIQUE(variant_id, attribute_value_id)
);
```

### Subscriptions

```sql
-- Subscription status enum
CREATE TYPE subscription_status AS ENUM (
    'active', 'paused', 'cancelled', 'expired', 'past_due', 'trialing', 'pending'
);

-- Cancellation reason enum
CREATE TYPE cancellation_reason AS ENUM (
    'customer_requested', 'payment_failed', 'fraudulent', 'too_expensive', 'not_useful', 'other'
);

-- Subscription order type
CREATE TYPE order_type AS ENUM ('one_time', 'subscription_initial', 'subscription_renewal');

-- Alter orders table
ALTER TABLE orders ADD COLUMN order_type order_type NOT NULL DEFAULT 'one_time';
ALTER TABLE orders ADD COLUMN subscription_id UUID;
ALTER TABLE orders ADD COLUMN billing_cycle INTEGER;

-- Subscriptions table
CREATE TABLE subscriptions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    order_id UUID NOT NULL REFERENCES orders(id),
    product_id UUID NOT NULL REFERENCES products(id),
    variant_id UUID REFERENCES product_variants(id),
    
    status subscription_status NOT NULL DEFAULT 'pending',
    interval subscription_interval NOT NULL,
    interval_count INTEGER NOT NULL DEFAULT 1,
    
    currency currency NOT NULL,
    amount DECIMAL(20, 2) NOT NULL,
    setup_fee DECIMAL(20, 2),
    
    trial_days INTEGER DEFAULT 0,
    trial_ends_at TIMESTAMPTZ,
    
    current_cycle INTEGER DEFAULT 0,
    min_cycles INTEGER,
    max_cycles INTEGER,
    
    starts_at TIMESTAMPTZ NOT NULL,
    next_billing_at TIMESTAMPTZ NOT NULL,
    last_billing_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    cancellation_reason cancellation_reason,
    
    payment_method_id VARCHAR(255),
    gateway VARCHAR(50) NOT NULL,
    
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_subscriptions_customer_id ON subscriptions(customer_id);
CREATE INDEX idx_subscriptions_product_id ON subscriptions(product_id);
CREATE INDEX idx_subscriptions_status ON subscriptions(status);
CREATE INDEX idx_subscriptions_next_billing ON subscriptions(next_billing_at);

-- Subscription invoices
CREATE TABLE subscription_invoices (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    order_id UUID REFERENCES orders(id),
    
    cycle_number INTEGER NOT NULL,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    
    subtotal DECIMAL(20, 2) NOT NULL,
    tax_total DECIMAL(20, 2) NOT NULL DEFAULT 0,
    total DECIMAL(20, 2) NOT NULL,
    
    status invoice_status NOT NULL DEFAULT 'pending',
    paid_at TIMESTAMPTZ,
    payment_id VARCHAR(255),
    
    failed_attempts INTEGER DEFAULT 0,
    last_failed_at TIMESTAMPTZ,
    failure_reason TEXT,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_subscription_invoices_subscription_id ON subscription_invoices(subscription_id);
CREATE INDEX idx_subscription_invoices_status ON subscription_invoices(status);
```

## API Examples

### Create Variable Product

```bash
POST /api/v1/products
Content-Type: application/json

{
  "title": "Premium T-Shirt",
  "slug": "premium-t-shirt",
  "product_type": "variable",
  "description": "High-quality cotton t-shirt",
  "base_price": "29.99",
  "currency": "USD",
  "attributes": [
    {
      "name": "Color",
      "slug": "color",
      "position": 1,
      "visible": true,
      "variation": true,
      "values": [
        {"value": "Red", "color_code": "#FF0000", "position": 1},
        {"value": "Blue", "color_code": "#0000FF", "position": 2}
      ]
    },
    {
      "name": "Size",
      "slug": "size",
      "position": 2,
      "visible": true,
      "variation": true,
      "values": [
        {"value": "Small", "position": 1},
        {"value": "Large", "position": 2}
      ]
    }
  ],
  "variants": [
    {
      "title": "Premium T-Shirt - Red / Small",
      "sku": "TSHIRT-RED-SM",
      "price": "29.99",
      "inventory_quantity": 100
    },
    {
      "title": "Premium T-Shirt - Blue / Large",
      "sku": "TSHIRT-BLUE-LG",
      "price": "32.99",
      "inventory_quantity": 50
    }
  ]
}
```

### Create Subscription Product

```bash
POST /api/v1/products
Content-Type: application/json

{
  "title": "Premium Coffee Subscription",
  "slug": "coffee-subscription",
  "product_type": "subscription",
  "description": "Monthly delivery of premium coffee beans",
  "price": "29.99",
  "currency": "USD",
  "subscription_interval": "monthly",
  "subscription_interval_count": 1,
  "subscription_trial_days": 7,
  "subscription_setup_fee": "5.00",
  "subscription_min_cycles": 3
}
```

### Create Subscription

```bash
POST /api/v1/subscriptions
Content-Type: application/json

{
  "customer_id": "550e8400-e29b-41d4-a716-446655440000",
  "product_id": "550e8400-e29b-41d4-a716-446655440001",
  "interval": "monthly",
  "interval_count": 1,
  "currency": "USD",
  "amount": "29.99",
  "trial_days": 7,
  "payment_method_id": "pm_1234567890",
  "gateway": "stripe"
}
```

### Cancel Subscription

```bash
POST /api/v1/subscriptions/{id}/cancel
Content-Type: application/json

{
  "reason": "customer_requested",
  "reason_details": "Moving to a different service",
  "cancel_at_end": true
}
```

## Best Practices

### Product Attributes

1. **Keep it Simple**: Don't create too many attributes - it creates variant explosion
2. **Use Swatches**: For colors, provide hex codes and swatch images
3. **Attribute Order**: Position matters - put most important attributes first
4. **SKU Strategy**: Use consistent SKU formats (e.g., `PRODUCT-COLOR-SIZE`)

### Subscriptions

1. **Trial Periods**: Offer trials to reduce friction
2. **Min/Max Cycles**: Set reasonable minimums to prevent abuse
3. **Grace Periods**: Allow a few days past due before cancellation
4. **Proration**: Handle mid-cycle upgrades/downgrades fairly
5. **Payment Methods**: Store payment methods securely with gateways
6. **Notifications**: Email before billing, on success, and on failure

### Inventory Management

1. **Track by Variant**: Each variant should have its own inventory
2. **Low Stock Alerts**: Set thresholds per variant
3. **Overselling**: Decide policy (deny vs. allow backorders)
4. **Bundle Inventory**: Decrease component stock when bundle sells

---

*Inventory management strategies are documented in the Order Management section.*
