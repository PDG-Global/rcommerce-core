-- Initial schema migration for R Commerce
-- Creates core tables for products, customers, orders

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ====================
-- ENUM TYPES
-- ====================

-- Currency type
CREATE TYPE currency AS ENUM ('USD', 'EUR', 'GBP', 'JPY', 'AUD', 'CAD', 'CNY', 'HKD', 'SGD');

-- Weight unit type
CREATE TYPE weight_unit AS ENUM ('g', 'kg', 'oz', 'lb');

-- Length unit type
CREATE TYPE length_unit AS ENUM ('cm', 'm', 'in', 'ft');

-- Inventory policy type
CREATE TYPE inventory_policy AS ENUM ('deny', 'continue');

-- Product type
CREATE TYPE product_type AS ENUM ('simple', 'variable', 'subscription', 'digital', 'bundle');

-- Subscription interval type
CREATE TYPE subscription_interval AS ENUM ('daily', 'weekly', 'bi_weekly', 'monthly', 'quarterly', 'bi_annually', 'annually');

-- Order status type
CREATE TYPE order_status AS ENUM ('pending', 'confirmed', 'processing', 'on_hold', 'completed', 'cancelled', 'refunded');

-- Fulfillment status type
CREATE TYPE fulfillment_status AS ENUM ('pending', 'processing', 'partial', 'shipped', 'delivered', 'cancelled', 'returned');

-- Payment status type
CREATE TYPE payment_status AS ENUM ('pending', 'authorized', 'paid', 'failed', 'cancelled', 'refunded');

-- Payment method type
CREATE TYPE payment_method_type AS ENUM ('credit_card', 'debit_card', 'bank_transfer', 'cash_on_delivery', 'digital_wallet', 'cryptocurrency');

-- ====================
-- PRODUCT TABLES
-- ====================

-- Products
CREATE TABLE IF NOT EXISTS products (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    sku VARCHAR(100),
    product_type product_type NOT NULL DEFAULT 'simple',
    price DECIMAL(20, 2) NOT NULL,
    compare_at_price DECIMAL(20, 2),
    cost_price DECIMAL(20, 2),
    currency currency NOT NULL DEFAULT 'USD',
    inventory_quantity INTEGER NOT NULL DEFAULT 0,
    inventory_policy inventory_policy NOT NULL DEFAULT 'deny',
    inventory_management BOOLEAN NOT NULL DEFAULT false,
    continues_selling_when_out_of_stock BOOLEAN NOT NULL DEFAULT false,
    weight DECIMAL(10, 4),
    weight_unit weight_unit,
    requires_shipping BOOLEAN NOT NULL DEFAULT true,
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_featured BOOLEAN NOT NULL DEFAULT false,
    seo_title VARCHAR(255),
    seo_description TEXT,
    -- Subscription fields
    subscription_interval subscription_interval,
    subscription_interval_count INTEGER,
    subscription_trial_days INTEGER,
    subscription_setup_fee DECIMAL(20, 2),
    subscription_min_cycles INTEGER,
    subscription_max_cycles INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ
);

CREATE INDEX idx_products_slug ON products(slug);
CREATE INDEX idx_products_is_active ON products(is_active);
CREATE INDEX idx_products_created_at ON products(created_at);
CREATE INDEX idx_products_price ON products(price);
CREATE INDEX idx_products_product_type ON products(product_type);

-- Product variants
CREATE TABLE IF NOT EXISTS product_variants (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    sku VARCHAR(100),
    price DECIMAL(20, 2) NOT NULL,
    compare_at_price DECIMAL(20, 2),
    cost_price DECIMAL(20, 2),
    currency currency NOT NULL DEFAULT 'USD',
    inventory_quantity INTEGER NOT NULL DEFAULT 0,
    inventory_policy inventory_policy NOT NULL DEFAULT 'deny',
    weight DECIMAL(10, 4),
    weight_unit weight_unit,
    requires_shipping BOOLEAN NOT NULL DEFAULT true,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_product_variants_product_id ON product_variants(product_id);

-- Product images
CREATE TABLE IF NOT EXISTS product_images (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    variant_id UUID REFERENCES product_variants(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    src VARCHAR(500) NOT NULL,
    alt_text VARCHAR(255),
    width INTEGER,
    height INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_product_images_product_id ON product_images(product_id);

-- Product options (e.g., Color, Size for variable products)
CREATE TABLE IF NOT EXISTS product_options (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_product_options_product_id ON product_options(product_id);

-- Product option values
CREATE TABLE IF NOT EXISTS product_option_values (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    option_id UUID NOT NULL REFERENCES product_options(id) ON DELETE CASCADE,
    variant_id UUID NOT NULL REFERENCES product_variants(id) ON DELETE CASCADE,
    value VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_product_option_values_option_id ON product_option_values(option_id);
CREATE INDEX idx_product_option_values_variant_id ON product_option_values(variant_id);

-- ====================
-- CUSTOMER TABLES
-- ====================

-- Customers
CREATE TABLE IF NOT EXISTS customers (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) NOT NULL UNIQUE,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    phone VARCHAR(50),
    accepts_marketing BOOLEAN NOT NULL DEFAULT false,
    password_hash VARCHAR(255),
    is_verified BOOLEAN NOT NULL DEFAULT false,
    last_login_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_customers_email ON customers(email);
CREATE INDEX idx_customers_created_at ON customers(created_at);

-- Customer addresses
CREATE TABLE IF NOT EXISTS addresses (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    company VARCHAR(100),
    address1 VARCHAR(255) NOT NULL,
    address2 VARCHAR(255),
    city VARCHAR(100) NOT NULL,
    province VARCHAR(100),
    country VARCHAR(100) NOT NULL,
    zip VARCHAR(20) NOT NULL,
    phone VARCHAR(50),
    is_default BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_addresses_customer_id ON addresses(customer_id);

-- ====================
-- ORDER TABLES
-- ====================

-- Orders
CREATE TABLE IF NOT EXISTS orders (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_number VARCHAR(50) NOT NULL UNIQUE,
    customer_id UUID REFERENCES customers(id) ON DELETE SET NULL,
    email VARCHAR(255) NOT NULL,
    currency currency NOT NULL DEFAULT 'USD',
    subtotal DECIMAL(20, 2) NOT NULL DEFAULT 0,
    tax_total DECIMAL(20, 2) NOT NULL DEFAULT 0,
    shipping_total DECIMAL(20, 2) NOT NULL DEFAULT 0,
    discount_total DECIMAL(20, 2) NOT NULL DEFAULT 0,
    total DECIMAL(20, 2) NOT NULL DEFAULT 0,
    status order_status NOT NULL DEFAULT 'pending',
    payment_status payment_status NOT NULL DEFAULT 'pending',
    fulfillment_status fulfillment_status,
    shipping_address JSONB,
    billing_address JSONB,
    notes TEXT,
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ
);

CREATE INDEX idx_orders_customer_id ON orders(customer_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created_at ON orders(created_at);
CREATE INDEX idx_orders_order_number ON orders(order_number);

-- Order items
CREATE TABLE IF NOT EXISTS order_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id UUID REFERENCES products(id) ON DELETE SET NULL,
    variant_id UUID REFERENCES product_variants(id) ON DELETE SET NULL,
    title VARCHAR(255) NOT NULL,
    variant_title VARCHAR(255),
    sku VARCHAR(100),
    quantity INTEGER NOT NULL,
    price DECIMAL(20, 2) NOT NULL,
    total DECIMAL(20, 2) NOT NULL,
    requires_shipping BOOLEAN NOT NULL DEFAULT true,
    is_subscription BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_order_items_order_id ON order_items(order_id);

-- ====================
-- PAYMENT TABLES
-- ====================

-- Payments
CREATE TABLE IF NOT EXISTS payments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    amount DECIMAL(20, 2) NOT NULL,
    currency currency NOT NULL DEFAULT 'USD',
    status payment_status NOT NULL DEFAULT 'pending',
    gateway VARCHAR(50) NOT NULL,
    gateway_payment_id VARCHAR(255),
    payment_method payment_method_type,
    card_last_four VARCHAR(4),
    card_brand VARCHAR(50),
    error_message TEXT,
    processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_payments_order_id ON payments(order_id);
CREATE INDEX idx_payments_status ON payments(status);

-- Refunds
CREATE TABLE IF NOT EXISTS refunds (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    payment_id UUID NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    amount DECIMAL(20, 2) NOT NULL,
    currency currency NOT NULL DEFAULT 'USD',
    reason TEXT,
    status payment_status NOT NULL DEFAULT 'pending',
    gateway_refund_id VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_refunds_payment_id ON refunds(payment_id);
CREATE INDEX idx_refunds_order_id ON refunds(order_id);
