# Data Models & Database Schema

## Overview

This document details the complete data models and database schema for R commerce. The models are designed to be database-agnostic while leveraging platform-specific optimizations where available.

## Entity Relationship Diagram

```
┌─────────────────┐          ┌─────────────────┐
│    Customer     │          │    Address      │
├─────────────────┤          ├─────────────────┤
│ id              │◄─────────┤ id              │
│ email           │          │ customer_id     │
│ first_name      │          │ first_name      │
│ last_name       │          │ last_name       │
│ phone           │          │ company         │
│ default_address │          │ street1         │
│ accepts_market  │          │ street2         │
│ created_at      │          │ city            │
│ updated_at      │          │ state           │
└────────┬────────┘          │ postal_code     │
         │                   │ country         │
         │                   │ phone           │
         │                   │ is_default      │
         │                   │ address_type    │
         └──────────────────►│ created_at      │
                             └─────────────────┘

         │
         │ creates
         ▼

┌─────────────────┐
│     Order       │
├─────────────────┤          ┌─────────────────┐
│ id              │          │   OrderNote     │
│ order_number    │          ├─────────────────┤
│ customer_id     │◄─────────┤ id              │
│ billing_id      │          │ order_id        │
│ shipping_id     │          │ author_id       │
│ subtotal        │          │ author_name     │
│ tax_amount      │          │ content         │
│ shipping_amount │          │ is_customer_vis │
│ discount_amount │          │ created_at      │
│ total           │          └─────────────────┘
│ currency        │                 
│ status          │
│ fraud_score     │
│ created_at      │
│ updated_at      │
└────────┬────────┘
         │
         │ contains
         │
         ▼
┌─────────────────┐          ┌─────────────────┐
│ OrderLineItem   │          │    Payment      │
├─────────────────┤          ├─────────────────┤
│ id              │          │ id              │
│ order_id        │◄─────────┤ order_id        │
│ product_id      │          │ gateway         │
│ variant_id      │          │ amount          │
│ name            │          │ currency        │
│ sku             │          │ method          │
│ quantity        │          │ status          │
│ unit_price      │          │ provider_id     │
│ tax_amount      │          │ fraud_check     │
│ discount_amount │          │ refunded_amount │
│ total           │          │ created_at      │
│ weight          │          │ completed_at    │
│ created_at      │          └─────────────────┘
│ updated_at      │
└────────┬────────┘
         │
         └──────────►┌─────────────────┐
                     │   Fulfillment   │
                     ├─────────────────┤
                     │ id              │
                     │ order_id        │
                     │ provider        │
                     │ carrier         │
                     │ service         │
                     │ tracking_number │
                     │ items           │
                     │ status          │
                     │ created_at      │
                     │ shipped_at      │
                     └─────────────────┘


┌─────────────────┐          ┌─────────────────┐
│    Product      │          │ ProductVariant  │
├─────────────────┤          ├─────────────────┤
│ id              │          │ id              │
│ name            │          │ product_id      │
│ slug            │          │ sku             │
│ description     │          │ price           │
│ sku             │◄─────────┤ compare_price   │
│ price           │          │ cost            │
│ compare_price   │          │ inventory_qty   │
│ cost            │          │ weight          │
│ inventory_qty   │          │ length          │
│ weight          │          │ width           │
│ length          │          │ height          │
│ width           │          │ metadata        │
│ height          │          │ created_at      │
│ status          │          │ updated_at      │
│ category_id     │          └─────────────────┘
│ is_taxable      │
│ requires_ship   │
│ created_at      │
│ updated_at      │
└────────┬────────┘
         │
         └──────────►┌─────────────────┐
                     │  ProductImage   │
                     ├─────────────────┤
                     │ id              │
                     │ product_id      │
                     │ url             │
                     │ alt             │
                     │ position        │
                     │ created_at      │
                     └─────────────────┘


┌─────────────────┐          ┌─────────────────┐
│   Category      │          │    Discount     │
├─────────────────┤          ├─────────────────┤
│ id              │          │ id              │
│ name            │          │ code            │
│ slug            │          │ type            │
│ description     │          │ value           │
│ parent_id       │          │ min_cart_value  │
│ lft             │          │ usage_limit     │
│ rgt             │          │ usage_count     │
│ depth           │          │ starts_at       │
│ created_at      │          │ ends_at         │
│ updated_at      │          │ created_at      │
└─────────────────┘          └─────────────────┘


┌─────────────────┐          ┌─────────────────┐
│   TaxRate       │          │     Cart        │
├─────────────────┤          ├─────────────────┤
│ id              │          │ id              │
│ name            │          │ token           │
│ rate            │          │ customer_id     │
│ country         │          │ email           │
│ state           │          │ region_id       │
│ county          │          │ currency        │
│ created_at      │          │ completed_at    │
│ updated_at      │          │ created_at      │
└─────────────────┘          │ updated_at      │
                             └────────┬────────┘
                                      │
                                      │ contains
                                      ▼
                             ┌─────────────────┐
                             │   CartItem      │
                             ├─────────────────┤
                             │ id              │
                             │ cart_id         │
                             │ product_id      │
                             │ variant_id      │
                             │ quantity        │
                             │ created_at      │
                             │ updated_at      │
                             └─────────────────┘


┌─────────────────┐
│    Webhook      │
├─────────────────┐
│ id              │
│ url             │
│ events          │
│ is_active       │
│ secret          │
│ created_at      │
│ updated_at      │
└─────────────────┘
```

## Detailed Schema

### Customer Table

```sql
-- customers table
CREATE TABLE customers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    first_name VARCHAR(255),
    last_name VARCHAR(255),
    phone VARCHAR(50),
    accepts_marketing BOOLEAN NOT NULL DEFAULT FALSE,
    default_address_id UUID,
    meta_data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_order_at TIMESTAMP WITH TIME ZONE,
    orders_count INTEGER NOT NULL DEFAULT 0,
    total_spent DECIMAL(10,2) NOT NULL DEFAULT 0,
    
    -- Indexes
    CONSTRAINT chk_valid_email CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$')
);

-- Indexes
CREATE UNIQUE INDEX idx_customers_email ON customers(email);
CREATE INDEX idx_customers_created_at ON customers(created_at DESC);
CREATE INDEX idx_customers_last_order_at ON customers(last_order_at DESC NULLS LAST);
CREATE INDEX idx_customers_orders_count ON customers(orders_count);

-- Address table
CREATE TABLE addresses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    first_name VARCHAR(255) NOT NULL,
    last_name VARCHAR(255) NOT NULL,
    company VARCHAR(255),
    street1 VARCHAR(500) NOT NULL,
    street2 VARCHAR(500),
    city VARCHAR(255) NOT NULL,
    state VARCHAR(255),
    postal_code VARCHAR(50) NOT NULL,
    country VARCHAR(2) NOT NULL DEFAULT 'US',
    phone VARCHAR(50),
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    address_type VARCHAR(20) NOT NULL DEFAULT 'shipping', -- 'shipping' or 'billing'
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- Indexes
    CONSTRAINT chk_valid_country CHECK (country ~ '^[A-Z]{2}$')
);

-- Indexes
CREATE INDEX idx_addresses_customer_id ON addresses(customer_id);
CREATE INDEX idx_addresses_country ON addresses(country);
CREATE INDEX idx_addresses_postal_code ON addresses(postal_code);
```

### Order Tables

```sql
-- orders table
CREATE TYPE order_status AS ENUM (
    'pending',
    'confirmed',
    'processing',
    'on_hold',
    'shipped',
    'completed',
    'cancelled',
    'refunded'
);

CREATE TYPE payment_status AS ENUM (
    'pending',
    'authorized',
    'paid',
    'partially_refunded',
    'fully_refunded',
    'failed',
    'cancelled'
);

CREATE TYPE fulfillment_status AS ENUM (
    'not_fulfilled',
    'partially_fulfilled',
    'fulfilled',
    'delivered',
    'returned',
    'partially_returned'
);

CREATE TABLE orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_number VARCHAR(50) UNIQUE NOT NULL,
    customer_id UUID NOT NULL REFERENCES customers(id),
    customer_email VARCHAR(255) NOT NULL,
    billing_address_id UUID REFERENCES addresses(id),
    shipping_address_id UUID REFERENCES addresses(id),
    subtotal DECIMAL(10,2) NOT NULL DEFAULT 0,
    tax_amount DECIMAL(10,2) NOT NULL DEFAULT 0,
    shipping_amount DECIMAL(10,2) NOT NULL DEFAULT 0,
    discount_amount DECIMAL(10,2) NOT NULL DEFAULT 0,
    total DECIMAL(10,2) NOT NULL DEFAULT 0,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    status order_status NOT NULL DEFAULT 'pending',
    payment_status payment_status NOT NULL DEFAULT 'pending',
    fulfillment_status fulfillment_status NOT NULL DEFAULT 'not_fulfilled',
    fraud_score INTEGER,
    fraud_reasons TEXT[],
    tags TEXT[],
    meta_data JSONB NOT NULL DEFAULT '{}',
    source VARCHAR(50) NOT NULL DEFAULT 'web', -- 'web', 'mobile', 'api', 'admin'
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    confirmed_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE,
    cancelled_at TIMESTAMP WITH TIME ZONE,
    cancelled_reason TEXT,
    
    -- Indexes
    CONSTRAINT chk_positive_total CHECK (total >= 0)
);

-- Create order number sequence
CREATE SEQUENCE order_number_seq START 1000;

-- Indexes
CREATE UNIQUE INDEX idx_orders_order_number ON orders(order_number);
CREATE INDEX idx_orders_customer_id ON orders(customer_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_payment_status ON orders(payment_status);
CREATE INDEX idx_orders_fulfillment_status ON orders(fulfillment_status);
CREATE INDEX idx_orders_created_at ON orders(created_at DESC);
CREATE INDEX idx_orders_total ON orders(total);
CREATE INDEX idx_orders_fraud_score ON orders(fraud_score) WHERE fraud_score IS NOT NULL;
CREATE INDEX idx_orders_tags ON orders USING GIN(tags);
CREATE INDEX idx_orders_source ON orders(source);

-- Trigger for updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_orders_updated_at
    BEFORE UPDATE ON orders
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Order line items table
CREATE TABLE order_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id UUID NOT NULL,
    variant_id UUID,
    name VARCHAR(500) NOT NULL,
    sku VARCHAR(100),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    quantity_fulfilled INTEGER NOT NULL DEFAULT 0,
    quantity_returned INTEGER NOT NULL DEFAULT 0,
    unit_price DECIMAL(10,2) NOT NULL,
    original_unit_price DECIMAL(10,2) NOT NULL,
    tax_amount DECIMAL(10,2) NOT NULL DEFAULT 0,
    discount_amount DECIMAL(10,2) NOT NULL DEFAULT 0,
    total DECIMAL(10,2) NOT NULL,
    weight DECIMAL(8,3),
    requires_shipping BOOLEAN NOT NULL DEFAULT TRUE,
    is_gift_card BOOLEAN NOT NULL DEFAULT FALSE,
    is_discountable BOOLEAN NOT NULL DEFAULT TRUE,
    is_taxable BOOLEAN NOT NULL DEFAULT TRUE,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_order_line_items_order_id ON order_line_items(order_id);
CREATE INDEX idx_order_line_items_product_id ON order_line_items(product_id);
CREATE INDEX idx_order_line_items_sku ON order_line_items(sku) WHERE sku IS NOT NULL;
CREATE INDEX idx_order_line_items_created_at ON order_line_items(created_at DESC);

-- Trigger for updated_at
CREATE TRIGGER update_order_line_items_updated_at
    BEFORE UPDATE ON order_line_items
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Order notes table
CREATE TABLE order_notes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    author_id UUID,
    author_name VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    is_customer_visible BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_order_notes_order_id ON order_notes(order_id);
CREATE INDEX idx_order_notes_created_at ON order_notes(created_at DESC);
```

### Product Tables

```sql
-- Products table
CREATE TABLE products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(500) NOT NULL,
    slug VARCHAR(500) NOT NULL UNIQUE,
    description TEXT,
    short_description TEXT,
    sku VARCHAR(100) UNIQUE,
    price DECIMAL(10,2) NOT NULL,
    compare_at_price DECIMAL(10,2),
    cost DECIMAL(10,2),
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    inventory_quantity INTEGER NOT NULL DEFAULT 0,
    inventory_policy VARCHAR(20) NOT NULL DEFAULT 'deny', -- 'deny' or 'continue'
    barcode VARCHAR(100),
    weight DECIMAL(8,3),
    length DECIMAL(8,3),
    width DECIMAL(8,3),
    height DECIMAL(8,3),
    status VARCHAR(20) NOT NULL DEFAULT 'draft', -- 'active', 'draft', 'archived'
    category_id UUID,
    is_taxable BOOLEAN NOT NULL DEFAULT TRUE,
    requires_shipping BOOLEAN NOT NULL DEFAULT TRUE,
    is_gift_card BOOLEAN NOT NULL DEFAULT FALSE,
    seo_title VARCHAR(500),
    seo_description VARCHAR(1000),
    meta_data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    published_at TIMESTAMP WITH TIME ZONE,
    
    -- Indexes
    CONSTRAINT chk_positive_price CHECK (price >= 0),
    CONSTRAINT chk_positive_inventory CHECK (inventory_quantity >= 0)
);

-- Indexes
CREATE UNIQUE INDEX idx_products_slug ON products(slug);
CREATE INDEX idx_products_sku ON products(sku) WHERE sku IS NOT NULL;
CREATE INDEX idx_products_status ON products(status);
CREATE INDEX idx_products_category_id ON products(category_id);
CREATE INDEX idx_products_created_at ON products(created_at DESC);
CREATE INDEX idx_products_price ON products(price);
CREATE INDEX idx_products_search ON products USING gin(
    to_tsvector('english', name || ' ' || description)
);

-- Full text search for products
CREATE INDEX idx_products_name_trgm ON products USING gin(name gin_trgm_ops);
CREATE INDEX idx_products_description_trgm ON products USING gin(description gin_trgm_ops);

-- Trigger for updated_at
CREATE TRIGGER update_products_updated_at
    BEFORE UPDATE ON products
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Product variants table
CREATE TABLE product_variants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    sku VARCHAR(100) UNIQUE,
    price DECIMAL(10,2),
    compare_at_price DECIMAL(10,2),
    cost DECIMAL(10,2),
    inventory_quantity INTEGER NOT NULL DEFAULT 0,
    inventory_policy VARCHAR(20) NOT NULL DEFAULT 'deny',
    barcode VARCHAR(100),
    weight DECIMAL(8,3),
    length DECIMAL(8,3),
    width DECIMAL(8,3),
    height DECIMAL(8,3),
    meta_data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_product_variants_product_id ON product_variants(product_id);
CREATE INDEX idx_product_variants_sku ON product_variants(sku) WHERE sku IS NOT NULL;
CREATE INDEX idx_product_variants_created_at ON product_variants(created_at DESC);

-- Trigger for updated_at
CREATE TRIGGER update_product_variants_updated_at
    BEFORE UPDATE ON product_variants
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Product images table
CREATE TABLE product_images (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    alt TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    meta_data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_product_images_product_id ON product_images(product_id);
CREATE INDEX idx_product_images_position ON product_images(position);

-- Categories table (nested set pattern)
CREATE TABLE categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    parent_id UUID REFERENCES categories(id),
    lft INTEGER NOT NULL,
    rgt INTEGER NOT NULL,
    depth INTEGER NOT NULL DEFAULT 0,
    meta_data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE UNIQUE INDEX idx_categories_slug ON categories(slug);
CREATE INDEX idx_categories_lft ON categories(lft);
CREATE INDEX idx_categories_rgt ON categories(rgt);
CREATE INDEX idx_categories_depth ON categories(depth);
CREATE INDEX idx_categories_parent_id ON categories(parent_id);

-- Trigger for updated_at
CREATE TRIGGER update_categories_updated_at
    BEFORE UPDATE ON categories
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
```

### Payment Tables

```sql
-- Payment gateway table (metadata for gateways)
CREATE TABLE payment_gateways (
    id VARCHAR(50) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    config JSONB NOT NULL DEFAULT '{}',
    supported_currencies VARCHAR(3)[],
    supported_methods VARCHAR(50)[],
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Trigger for updated_at
CREATE TRIGGER update_payment_gateways_updated_at
    BEFORE UPDATE ON payment_gateways
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Payments table
CREATE TABLE payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    gateway_id VARCHAR(50) NOT NULL REFERENCES payment_gateways(id),
    amount DECIMAL(10,2) NOT NULL,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    method VARCHAR(50) NOT NULL, -- 'card', 'bank_transfer', 'wallet', etc.
    status VARCHAR(20) NOT NULL,
    provider_payment_id VARCHAR(255),
    provider_response JSONB,
    provider_metadata JSONB,
    fraud_check_result JSONB,
    refunded_amount DECIMAL(10,2) NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE,
    
    CONSTRAINT chk_positive_amount CHECK (amount >= 0)
);

-- Indexes
CREATE INDEX idx_payments_order_id ON payments(order_id);
CREATE INDEX idx_payments_gateway_id ON payments(gateway_id);
CREATE INDEX idx_payments_status ON payments(status);
CREATE INDEX idx_payments_created_at ON payments(created_at DESC);
```

### Fulfillment Tables

```sql
-- Shipping providers table
CREATE TABLE shipping_providers (
    id VARCHAR(50) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    config JSONB NOT NULL DEFAULT '{}',
    credentials JSONB NOT NULL DEFAULT '{}',
    supported_countries VARCHAR(2)[],
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Trigger for updated_at
CREATE TRIGGER update_shipping_providers_updated_at
    BEFORE UPDATE ON shipping_providers
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Fulfillments table
CREATE TABLE fulfillments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    provider_id VARCHAR(50) NOT NULL REFERENCES shipping_providers(id),
    fulfillment_number VARCHAR(50) NOT NULL UNIQUE,
    tracking_number VARCHAR(255),
    tracking_url TEXT,
    label_url TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    from_address JSONB NOT NULL,
    to_address JSONB NOT NULL,
    package JSONB NOT NULL,
    items JSONB NOT NULL, -- Array of {line_item_id, quantity}
    customs_info JSONB,
    insurance_amount DECIMAL(10,2),
    total_cost DECIMAL(10,2),
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    shipped_at TIMESTAMP WITH TIME ZONE,
    estimated_delivery TIMESTAMP WITH TIME ZONE,
    
    CONSTRAINT chk_positive_cost CHECK (total_cost >= 0)
);

-- Indexes
CREATE UNIQUE INDEX idx_fulfillments_fulfillment_number ON fulfillments(fulfillment_number);
CREATE INDEX idx_fulfillments_order_id ON fulfillments(order_id);
CREATE INDEX idx_fulfillments_tracking_number ON fulfillments(tracking_number) WHERE tracking_number IS NOT NULL;
CREATE INDEX idx_fulfillments_status ON fulfillments(status);
CREATE INDEX idx_fulfillments_created_at ON fulfillments(created_at DESC);
CREATE INDEX idx_fulfillments_shipped_at ON fulfillments(shipped_at) WHERE shipped_at IS NOT NULL;

-- Trigger for updated_at
CREATE TRIGGER update_fulfillments_updated_at
    BEFORE UPDATE ON fulfillments
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
```

### Discount & Promotion Tables

```sql
-- Discount codes table
CREATE TABLE discounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code VARCHAR(100) NOT NULL UNIQUE,
    type VARCHAR(20) NOT NULL, -- 'percentage', 'fixed_amount', 'free_shipping'
    value DECIMAL(10,2),
    value_currency VARCHAR(3) DEFAULT 'USD',
    min_cart_value DECIMAL(10,2),
    max_discount_amount DECIMAL(10,2),
    usage_limit INTEGER,
    usage_count INTEGER NOT NULL DEFAULT 0,
    starts_at TIMESTAMP WITH TIME ZONE,
    ends_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    applies_to_all_products BOOLEAN NOT NULL DEFAULT TRUE,
    applies_to_all_customers BOOLEAN NOT NULL DEFAULT TRUE,
    applies_once_per_customer BOOLEAN NOT NULL DEFAULT FALSE,
    applies_to_shipping BOOLEAN NOT NULL DEFAULT FALSE,
    requires_shipping BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    CONSTRAINT chk_positive_value CHECK (value >= 0),
    CONSTRAINT chk_positive_usage CHECK (usage_limit IS NULL OR usage_limit > 0)
);

-- Indexes
CREATE UNIQUE INDEX idx_discounts_code ON discounts(code);
CREATE INDEX idx_discounts_type ON discounts(type);
CREATE INDEX idx_discounts_active ON discounts(is_active) WHERE is_active = TRUE;
CREATE INDEX idx_discounts_valid ON discounts(is_active, starts_at, ends_at)
    WHERE is_active = TRUE AND (starts_at IS NULL OR starts_at <= NOW())
          AND (ends_at IS NULL OR ends_at >= NOW());
CREATE INDEX idx_discounts_created_at ON discounts(created_at DESC);

-- Trigger for updated_at
CREATE TRIGGER update_discounts_updated_at
    BEFORE UPDATE ON discounts
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Discount products (for targeted discounts)
CREATE TABLE discount_products (
    discount_id UUID NOT NULL REFERENCES discounts(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (discount_id, product_id)
);

-- Discount categories (for category-level discounts)
CREATE TABLE discount_categories (
    discount_id UUID NOT NULL REFERENCES discounts(id) ON DELETE CASCADE,
    category_id UUID NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (discount_id, category_id)
);

-- Customer groups table
CREATE TABLE customer_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    meta_data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Customer group members
CREATE TABLE customer_group_members (
    group_id UUID NOT NULL REFERENCES customer_groups(id) ON DELETE CASCADE,
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, customer_id)
);

CREATE INDEX idx_customer_group_members_customer ON customer_group_members(customer_id);
```

### Tax Tables

```sql
-- Tax rates table
CREATE TABLE tax_rates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    rate DECIMAL(5,4) NOT NULL, -- e.g., 0.0825 for 8.25%
    country VARCHAR(2) NOT NULL,
    state VARCHAR(255),
    county VARCHAR(255),
    city VARCHAR(255),
    zip_code VARCHAR(20),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    CONSTRAINT chk_valid_rate CHECK (rate >= 0 AND rate < 1)
);

-- Indexes
CREATE INDEX idx_tax_rates_country ON tax_rates(country);
CREATE INDEX idx_tax_rates_state ON tax_rates(state) WHERE state IS NOT NULL;
CREATE INDEX idx_tax_rates_active ON tax_rates(is_active) WHERE is_active = TRUE;
CREATE INDEX idx_tax_rates_location ON tax_rates(country, state, city, zip_code);

-- Tax overrides for specific products
CREATE TABLE product_tax_overrides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    tax_rate_id UUID NOT NULL REFERENCES tax_rates(id) ON DELETE CASCADE,
    is_exempt BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_product_tax_overrides_unique ON product_tax_overrides(product_id, tax_rate_id);
```

### Cart Tables

```sql
-- Carts table (sessions)
CREATE TABLE carts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token VARCHAR(255) UNIQUE NOT NULL,
    customer_id UUID REFERENCES customers(id) ON DELETE SET NULL,
    email VARCHAR(255),
    region_id UUID,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    completed_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE UNIQUE INDEX idx_carts_token ON carts(token);
CREATE INDEX idx_carts_customer_id ON carts(customer_id);
CREATE INDEX idx_carts_created_at ON carts(created_at DESC);

-- Cart items table
CREATE TABLE cart_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cart_id UUID NOT NULL REFERENCES carts(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id),
    variant_id UUID REFERENCES product_variants(id),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE UNIQUE INDEX idx_cart_items_unique ON cart_items(cart_id, product_id, variant_id);
CREATE INDEX idx_cart_items_cart_id ON cart_items(cart_id);
CREATE INDEX idx_cart_items_product_id ON cart_items(product_id);

-- Trigger for updated_at
CREATE TRIGGER update_cart_items_updated_at
    BEFORE UPDATE ON cart_items
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
```

### Webhook & Event Tables

```sql
-- Webhooks table
CREATE TABLE webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url TEXT NOT NULL,
    events TEXT[] NOT NULL, -- Array of event names
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    secret VARCHAR(255),
    meta_data JSONB NOT NULL DEFAULT '{}',
    last_triggered_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_webhooks_active ON webhooks(is_active) WHERE is_active = TRUE;
CREATE INDEX idx_webhooks_events ON webhooks USING GIN(events);
CREATE INDEX idx_webhooks_created_at ON webhooks(created_at DESC);

-- Trigger for updated_at
CREATE TRIGGER update_webhooks_updated_at
    BEFORE UPDATE ON webhooks
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Webhook delivery attempts table
CREATE TABLE webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_name VARCHAR(255) NOT NULL,
    event_data JSONB NOT NULL,
    status VARCHAR(20) NOT NULL, -- 'pending', 'success', 'failed'
    response_status INTEGER,
    response_body TEXT,
    error_message TEXT,
    attempt_number INTEGER NOT NULL DEFAULT 1,
    next_retry_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    delivered_at TIMESTAMP WITH TIME ZONE
);

-- Indexes
CREATE INDEX idx_webhook_deliveries_webhook_id ON webhook_deliveries(webhook_id);
CREATE INDEX idx_webhook_deliveries_status ON webhook_deliveries(status);
CREATE INDEX idx_webhook_deliveries_created_at ON webhook_deliveries(created_at DESC);
CREATE INDEX idx_webhook_deliveries_next_retry ON webhook_deliveries(next_retry_at)
    WHERE status = 'pending' AND next_retry_at IS NOT NULL;
```

### Returns & Refunds Tables

```sql
-- Returns table
CREATE TABLE returns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    return_number VARCHAR(50) UNIQUE NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'requested', -- 'requested', 'received', 'processed', 'cancelled'
    return_reason TEXT NOT NULL,
    return_reason_note TEXT,
    note TEXT,
    receive_items BOOLEAN NOT NULL DEFAULT TRUE,
    received_at TIMESTAMP WITH TIME ZONE,
    refund_processed_at TIMESTAMP WITH TIME ZONE,
    total_refund_amount DECIMAL(10,2) NOT NULL DEFAULT 0,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE UNIQUE INDEX idx_returns_return_number ON returns(return_number);
CREATE INDEX idx_returns_order_id ON returns(order_id);
CREATE INDEX idx_returns_status ON returns(status);
CREATE INDEX idx_returns_created_at ON returns(created_at DESC);

-- Return items table
CREATE TABLE return_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    return_id UUID NOT NULL REFERENCES returns(id) ON DELETE CASCADE,
    order_line_item_id UUID NOT NULL REFERENCES order_line_items(id),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    return_reason TEXT NOT NULL,
    note TEXT,
    refund_amount DECIMAL(10,2) NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Refunds table
CREATE TABLE refunds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    refund_number VARCHAR(50) UNIQUE NOT NULL,
    amount DECIMAL(10,2) NOT NULL,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    reason TEXT,
    processed_at TIMESTAMP WITH TIME ZONE,
    refunded_by UUID,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    CONSTRAINT chk_positive_amount CHECK (amount >= 0)
);

-- Indexes
CREATE UNIQUE INDEX idx_refunds_refund_number ON refunds(refund_number);
CREATE INDEX idx_refunds_order_id ON refunds(order_id);
CREATE INDEX idx_refunds_processed_at ON refunds(processed_at);
```

## Indexes Strategy

### Used Indexes
- **Primary Keys**: UUID primary keys for all tables (performance + privacy)
- **Foreign Keys**: Automatic indexes on foreign key columns
- **Composite Indexes**: For common query patterns (e.g., `customer_id + status`)
- **Partial Indexes**: For filtered queries (e.g., `WHERE status = 'active'`)
- **GIN Indexes**: For JSONB fields and array columns
- **Trigram Indexes**: For text search (PostgreSQL-specific)

## Database Migrations

Migration structure:
```
migrations/
├── 001_create_customers_table.sql
├── 002_create_addresses_table.sql
├── 003_create_products_table.sql
├── 004_create_categories_table.sql
├── 005_create_orders_table.sql
├── 006_create_order_line_items_table.sql
├── 007_create_payments_table.sql
├── 008_create_fulfillments_table.sql
├── 009_create_discounts_table.sql
├── 010_create_tax_rates_table.sql
└── ...
```

Migration runner implemented in Rust with SQLx migrations.

---

This completes the data modeling documentation, providing the complete database schema for R commerce across all supported database systems.
