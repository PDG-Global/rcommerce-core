-- Migration 025: Fix All Schema Issues
-- This is a comprehensive, idempotent migration that ensures all tables and columns exist
-- It can be safely run multiple times

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ====================
-- ENUM TYPES (Idempotent)
-- ====================

DO $$ BEGIN
    CREATE TYPE currency AS ENUM ('USD', 'EUR', 'GBP', 'JPY', 'AUD', 'CAD', 'CNY', 'HKD', 'SGD');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE weight_unit AS ENUM ('g', 'kg', 'oz', 'lb');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE length_unit AS ENUM ('cm', 'm', 'in', 'ft');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE inventory_policy AS ENUM ('deny', 'continue');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE product_type AS ENUM ('simple', 'variable', 'subscription', 'digital', 'bundle');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE subscription_interval AS ENUM ('daily', 'weekly', 'bi_weekly', 'monthly', 'quarterly', 'bi_annually', 'annually');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE order_status AS ENUM ('pending', 'confirmed', 'processing', 'on_hold', 'completed', 'cancelled', 'refunded');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE fulfillment_status AS ENUM ('pending', 'processing', 'partial', 'shipped', 'delivered', 'cancelled', 'returned');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE payment_status AS ENUM ('pending', 'authorized', 'paid', 'failed', 'cancelled', 'refunded');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE payment_method_type AS ENUM ('credit_card', 'debit_card', 'bank_transfer', 'cash_on_delivery', 'digital_wallet', 'cryptocurrency');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE discount_type AS ENUM ('percentage', 'fixed_amount', 'free_shipping', 'buy_x_get_y');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE customer_role AS ENUM ('customer', 'manager', 'admin');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE subscription_status AS ENUM ('active', 'paused', 'cancelled', 'expired', 'pending');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ====================
-- CORE TABLES
-- ====================

-- Products table
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

-- Add digital product columns
ALTER TABLE products ADD COLUMN IF NOT EXISTS file_url TEXT;
ALTER TABLE products ADD COLUMN IF NOT EXISTS file_size BIGINT;
ALTER TABLE products ADD COLUMN IF NOT EXISTS file_hash TEXT;
ALTER TABLE products ADD COLUMN IF NOT EXISTS download_limit INTEGER DEFAULT NULL;
ALTER TABLE products ADD COLUMN IF NOT EXISTS license_key_enabled BOOLEAN DEFAULT FALSE;
ALTER TABLE products ADD COLUMN IF NOT EXISTS download_expiry_days INTEGER DEFAULT NULL;

-- Add bundle pricing columns
ALTER TABLE products ADD COLUMN IF NOT EXISTS bundle_pricing_strategy VARCHAR(20);
ALTER TABLE products ADD COLUMN IF NOT EXISTS bundle_discount_percentage DECIMAL(5,2);

CREATE INDEX IF NOT EXISTS idx_products_slug ON products(slug);
CREATE INDEX IF NOT EXISTS idx_products_is_active ON products(is_active);
CREATE INDEX IF NOT EXISTS idx_products_created_at ON products(created_at);
CREATE INDEX IF NOT EXISTS idx_products_price ON products(price);
CREATE INDEX IF NOT EXISTS idx_products_product_type ON products(product_type);

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

CREATE INDEX IF NOT EXISTS idx_product_variants_product_id ON product_variants(product_id);

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

CREATE INDEX IF NOT EXISTS idx_product_images_product_id ON product_images(product_id);

-- Product options
CREATE TABLE IF NOT EXISTS product_options (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_product_options_product_id ON product_options(product_id);

-- Product option values
CREATE TABLE IF NOT EXISTS product_option_values (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    option_id UUID NOT NULL REFERENCES product_options(id) ON DELETE CASCADE,
    variant_id UUID NOT NULL REFERENCES product_variants(id) ON DELETE CASCADE,
    value VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_product_option_values_option_id ON product_option_values(option_id);
CREATE INDEX IF NOT EXISTS idx_product_option_values_variant_id ON product_option_values(variant_id);

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

-- Add role column
ALTER TABLE customers ADD COLUMN IF NOT EXISTS role customer_role NOT NULL DEFAULT 'customer';

CREATE INDEX IF NOT EXISTS idx_customers_email ON customers(email);
CREATE INDEX IF NOT EXISTS idx_customers_created_at ON customers(created_at);

-- Addresses
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Fix address columns: add is_default_shipping and is_default_billing
ALTER TABLE addresses ADD COLUMN IF NOT EXISTS is_default_shipping BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE addresses ADD COLUMN IF NOT EXISTS is_default_billing BOOLEAN NOT NULL DEFAULT false;

-- Migrate existing data from old is_default column if it exists
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns 
               WHERE table_name = 'addresses' AND column_name = 'is_default') THEN
        UPDATE addresses SET 
            is_default_shipping = is_default,
            is_default_billing = is_default
        WHERE is_default = true;
        ALTER TABLE addresses DROP COLUMN is_default;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_addresses_customer_id ON addresses(customer_id);

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

-- Add coupon columns
ALTER TABLE orders ADD COLUMN IF NOT EXISTS coupon_code VARCHAR(50);
ALTER TABLE orders ADD COLUMN IF NOT EXISTS coupon_id UUID REFERENCES coupons(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_orders_customer_id ON orders(customer_id);
CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status);
CREATE INDEX IF NOT EXISTS idx_orders_created_at ON orders(created_at);
CREATE INDEX IF NOT EXISTS idx_orders_order_number ON orders(order_number);
CREATE INDEX IF NOT EXISTS idx_orders_coupon ON orders(coupon_id) WHERE coupon_id IS NOT NULL;

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

-- Add digital/bundle columns to order_items
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS is_digital BOOLEAN DEFAULT FALSE;
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS download_url TEXT;
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS license_key TEXT;
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS bundle_parent_id UUID REFERENCES order_items(id) ON DELETE SET NULL;
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS is_bundle_component BOOLEAN DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_order_items_order_id ON order_items(order_id);
CREATE INDEX IF NOT EXISTS idx_order_items_bundle_parent ON order_items(bundle_parent_id);

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

CREATE INDEX IF NOT EXISTS idx_payments_order_id ON payments(order_id);
CREATE INDEX IF NOT EXISTS idx_payments_status ON payments(status);

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

CREATE INDEX IF NOT EXISTS idx_refunds_payment_id ON refunds(payment_id);
CREATE INDEX IF NOT EXISTS idx_refunds_order_id ON refunds(order_id);

-- ====================
-- CART TABLES
-- ====================

CREATE TABLE IF NOT EXISTS carts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID REFERENCES customers(id) ON DELETE SET NULL,
    session_token VARCHAR(255),
    currency currency NOT NULL DEFAULT 'USD',
    subtotal DECIMAL(19, 4) NOT NULL DEFAULT 0,
    discount_total DECIMAL(19, 4) NOT NULL DEFAULT 0,
    tax_total DECIMAL(19, 4) NOT NULL DEFAULT 0,
    shipping_total DECIMAL(19, 4) NOT NULL DEFAULT 0,
    total DECIMAL(19, 4) NOT NULL DEFAULT 0,
    coupon_code VARCHAR(50),
    email VARCHAR(255),
    shipping_address_id UUID REFERENCES addresses(id) ON DELETE SET NULL,
    billing_address_id UUID REFERENCES addresses(id) ON DELETE SET NULL,
    shipping_method VARCHAR(100),
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    converted_to_order BOOLEAN NOT NULL DEFAULT FALSE,
    order_id UUID REFERENCES orders(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_carts_customer_id ON carts(customer_id) WHERE customer_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_carts_session_token ON carts(session_token) WHERE session_token IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_carts_expires_at ON carts(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_carts_converted ON carts(converted_to_order) WHERE converted_to_order = TRUE;

CREATE TABLE IF NOT EXISTS cart_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cart_id UUID NOT NULL REFERENCES carts(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    variant_id UUID REFERENCES product_variants(id) ON DELETE SET NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0 AND quantity <= 9999),
    unit_price DECIMAL(19, 4) NOT NULL,
    original_price DECIMAL(19, 4) NOT NULL,
    subtotal DECIMAL(19, 4) NOT NULL,
    discount_amount DECIMAL(19, 4) NOT NULL DEFAULT 0,
    total DECIMAL(19, 4) NOT NULL,
    sku VARCHAR(255),
    title VARCHAR(500) NOT NULL,
    variant_title VARCHAR(500),
    image_url TEXT,
    requires_shipping BOOLEAN NOT NULL DEFAULT TRUE,
    is_gift_card BOOLEAN NOT NULL DEFAULT FALSE,
    custom_attributes JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cart_items_cart_id ON cart_items(cart_id);
CREATE INDEX IF NOT EXISTS idx_cart_items_product_id ON cart_items(product_id);

-- ====================
-- COUPON TABLES
-- ====================

CREATE TABLE IF NOT EXISTS coupons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code VARCHAR(50) NOT NULL UNIQUE,
    description TEXT,
    discount_type discount_type NOT NULL,
    discount_value DECIMAL(19, 4) NOT NULL,
    minimum_purchase DECIMAL(19, 4),
    maximum_discount DECIMAL(19, 4),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    starts_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    usage_limit INTEGER,
    usage_limit_per_customer INTEGER,
    usage_count INTEGER NOT NULL DEFAULT 0,
    applies_to_specific_products BOOLEAN NOT NULL DEFAULT FALSE,
    applies_to_specific_collections BOOLEAN NOT NULL DEFAULT FALSE,
    can_combine BOOLEAN NOT NULL DEFAULT FALSE,
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_coupons_active ON coupons(is_active) WHERE is_active = TRUE;
CREATE INDEX IF NOT EXISTS idx_coupons_code ON coupons(code);

CREATE TABLE IF NOT EXISTS coupon_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    coupon_id UUID NOT NULL REFERENCES coupons(id) ON DELETE CASCADE,
    product_id UUID REFERENCES products(id) ON DELETE CASCADE,
    collection_id UUID REFERENCES collections(id) ON DELETE CASCADE,
    is_exclusion BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_coupon_application_target CHECK (
        (product_id IS NOT NULL AND collection_id IS NULL) OR
        (product_id IS NULL AND collection_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_coupon_applications_coupon ON coupon_applications(coupon_id);
CREATE INDEX IF NOT EXISTS idx_coupon_applications_product ON coupon_applications(product_id) WHERE product_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_coupon_applications_collection ON coupon_applications(collection_id) WHERE collection_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS coupon_usages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    coupon_id UUID NOT NULL REFERENCES coupons(id) ON DELETE CASCADE,
    customer_id UUID REFERENCES customers(id) ON DELETE SET NULL,
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    discount_amount DECIMAL(19, 4) NOT NULL,
    used_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_coupon_usages_coupon ON coupon_usages(coupon_id);
CREATE INDEX IF NOT EXISTS idx_coupon_usages_customer ON coupon_usages(customer_id) WHERE customer_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_coupon_usages_order ON coupon_usages(order_id);

-- ====================
-- API KEYS TABLE
-- ====================

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID REFERENCES customers(id) ON DELETE CASCADE,
    key_prefix VARCHAR(16) NOT NULL UNIQUE,
    key_hash VARCHAR(64) NOT NULL,
    name VARCHAR(100) NOT NULL DEFAULT 'API Key',
    scopes TEXT[] NOT NULL DEFAULT ARRAY['read'],
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    last_used_ip VARCHAR(45),
    rate_limit_per_minute INTEGER,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_keys_customer_id ON api_keys(customer_id) WHERE customer_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_api_keys_key_prefix ON api_keys(key_prefix);
CREATE INDEX IF NOT EXISTS idx_api_keys_is_active ON api_keys(is_active) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_api_keys_expires_at ON api_keys(expires_at) WHERE expires_at IS NOT NULL;

-- ====================
-- SUBSCRIPTION TABLES
-- ====================

CREATE TABLE IF NOT EXISTS subscriptions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    status subscription_status NOT NULL DEFAULT 'pending',
    currency currency NOT NULL DEFAULT 'USD',
    subtotal DECIMAL(20, 2) NOT NULL DEFAULT 0,
    tax_total DECIMAL(20, 2) NOT NULL DEFAULT 0,
    total DECIMAL(20, 2) NOT NULL DEFAULT 0,
    interval subscription_interval NOT NULL,
    interval_count INTEGER NOT NULL DEFAULT 1,
    trial_ends_at TIMESTAMPTZ,
    current_period_starts_at TIMESTAMPTZ NOT NULL,
    current_period_ends_at TIMESTAMPTZ NOT NULL,
    cancelled_at TIMESTAMPTZ,
    cancel_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_subscriptions_customer_id ON subscriptions(customer_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_status ON subscriptions(status);

-- Subscription items
CREATE TABLE IF NOT EXISTS subscription_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    variant_id UUID REFERENCES product_variants(id) ON DELETE SET NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    price DECIMAL(20, 2) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_subscription_items_subscription_id ON subscription_items(subscription_id);
CREATE INDEX IF NOT EXISTS idx_subscription_items_product_id ON subscription_items(product_id);

-- ====================
-- FULFILLMENTS TABLE
-- ====================

-- Fix existing fulfillments table by adding missing columns
DO $$
BEGIN
    -- Add missing columns if table exists but columns don't
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'fulfillments') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'fulfillments' AND column_name = 'status') THEN
            ALTER TABLE fulfillments ADD COLUMN status fulfillment_status NOT NULL DEFAULT 'pending';
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'fulfillments' AND column_name = 'tracking_number') THEN
            ALTER TABLE fulfillments ADD COLUMN tracking_number VARCHAR(255);
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'fulfillments' AND column_name = 'tracking_url') THEN
            ALTER TABLE fulfillments ADD COLUMN tracking_url TEXT;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'fulfillments' AND column_name = 'tracking_company') THEN
            ALTER TABLE fulfillments ADD COLUMN tracking_company VARCHAR(100);
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'fulfillments' AND column_name = 'shipped_at') THEN
            ALTER TABLE fulfillments ADD COLUMN shipped_at TIMESTAMPTZ;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'fulfillments' AND column_name = 'delivered_at') THEN
            ALTER TABLE fulfillments ADD COLUMN delivered_at TIMESTAMPTZ;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'fulfillments' AND column_name = 'created_at') THEN
            ALTER TABLE fulfillments ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'fulfillments' AND column_name = 'updated_at') THEN
            ALTER TABLE fulfillments ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
        END IF;
    END IF;
END $$;

-- Create table if it doesn't exist
CREATE TABLE IF NOT EXISTS fulfillments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    status fulfillment_status NOT NULL DEFAULT 'pending',
    tracking_number VARCHAR(255),
    tracking_url TEXT,
    tracking_company VARCHAR(100),
    shipped_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_fulfillments_order_id ON fulfillments(order_id);
CREATE INDEX IF NOT EXISTS idx_fulfillments_status ON fulfillments(status);
CREATE INDEX IF NOT EXISTS idx_fulfillments_tracking_number ON fulfillments(tracking_number) WHERE tracking_number IS NOT NULL;

-- ====================
-- ORDER NOTES TABLE
-- ====================

CREATE TABLE IF NOT EXISTS order_notes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    author VARCHAR(255),
    note TEXT NOT NULL,
    is_customer_notified BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_order_notes_order_id ON order_notes(order_id);
CREATE INDEX IF NOT EXISTS idx_order_notes_created_at ON order_notes(created_at);

-- ====================
-- PRODUCT CATEGORIES TABLE
-- ====================

CREATE TABLE IF NOT EXISTS product_categories (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    parent_id UUID REFERENCES product_categories(id) ON DELETE SET NULL,
    image_url TEXT,
    seo_title VARCHAR(255),
    seo_description TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_product_categories_slug ON product_categories(slug);
CREATE INDEX IF NOT EXISTS idx_product_categories_parent_id ON product_categories(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_product_categories_is_active ON product_categories(is_active) WHERE is_active = true;

-- ====================
-- COLLECTIONS TABLE
-- ====================

CREATE TABLE IF NOT EXISTS collections (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title VARCHAR(255) NOT NULL,
    handle VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    seo_title VARCHAR(255),
    seo_description TEXT,
    sort_order VARCHAR(50) NOT NULL DEFAULT 'manual',
    published_at TIMESTAMPTZ,
    template_suffix VARCHAR(100),
    disjunctive BOOLEAN NOT NULL DEFAULT false,
    published_scope VARCHAR(20) NOT NULL DEFAULT 'web',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_collections_handle ON collections(handle);
CREATE INDEX IF NOT EXISTS idx_collections_published_at ON collections(published_at) WHERE published_at IS NOT NULL;

-- Collection products join table
CREATE TABLE IF NOT EXISTS collection_products (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(collection_id, product_id)
);

CREATE INDEX IF NOT EXISTS idx_collection_products_collection_id ON collection_products(collection_id);
CREATE INDEX IF NOT EXISTS idx_collection_products_product_id ON collection_products(product_id);

-- ====================
-- BUNDLE COMPONENTS TABLE
-- ====================

CREATE TABLE IF NOT EXISTS bundle_components (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    bundle_product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    component_product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    quantity INTEGER NOT NULL DEFAULT 1,
    is_optional BOOLEAN DEFAULT FALSE,
    sort_order INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(bundle_product_id, component_product_id)
);

CREATE INDEX IF NOT EXISTS idx_bundle_components_bundle ON bundle_components(bundle_product_id);
CREATE INDEX IF NOT EXISTS idx_bundle_components_component ON bundle_components(component_product_id);

-- ====================
-- DIGITAL PRODUCTS TABLES
-- ====================

CREATE TABLE IF NOT EXISTS order_item_downloads (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_item_id UUID NOT NULL REFERENCES order_items(id) ON DELETE CASCADE,
    customer_id UUID REFERENCES customers(id) ON DELETE SET NULL,
    download_token TEXT UNIQUE NOT NULL,
    download_count INTEGER DEFAULT 0,
    download_limit INTEGER,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_order_item_downloads_token ON order_item_downloads(download_token);
CREATE INDEX IF NOT EXISTS idx_order_item_downloads_order_item ON order_item_downloads(order_item_id);
CREATE INDEX IF NOT EXISTS idx_order_item_downloads_customer ON order_item_downloads(customer_id);

CREATE TABLE IF NOT EXISTS license_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    order_item_id UUID REFERENCES order_items(id) ON DELETE SET NULL,
    customer_id UUID REFERENCES customers(id) ON DELETE SET NULL,
    license_key TEXT NOT NULL,
    is_used BOOLEAN DEFAULT FALSE,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(product_id, license_key)
);

CREATE INDEX IF NOT EXISTS idx_license_keys_product ON license_keys(product_id);
CREATE INDEX IF NOT EXISTS idx_license_keys_order_item ON license_keys(order_item_id);
CREATE INDEX IF NOT EXISTS idx_license_keys_customer ON license_keys(customer_id);
CREATE INDEX IF NOT EXISTS idx_license_keys_key ON license_keys(license_key);

-- ====================
-- TRIGGERS FOR UPDATED_AT
-- ====================

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Products trigger
DROP TRIGGER IF EXISTS products_updated_at ON products;
CREATE TRIGGER products_updated_at
    BEFORE UPDATE ON products
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Product variants trigger
DROP TRIGGER IF EXISTS product_variants_updated_at ON product_variants;
CREATE TRIGGER product_variants_updated_at
    BEFORE UPDATE ON product_variants
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Customers trigger
DROP TRIGGER IF EXISTS customers_updated_at ON customers;
CREATE TRIGGER customers_updated_at
    BEFORE UPDATE ON customers
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Addresses trigger
DROP TRIGGER IF EXISTS addresses_updated_at ON addresses;
CREATE TRIGGER addresses_updated_at
    BEFORE UPDATE ON addresses
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Orders trigger
DROP TRIGGER IF EXISTS orders_updated_at ON orders;
CREATE TRIGGER orders_updated_at
    BEFORE UPDATE ON orders
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Order items trigger
DROP TRIGGER IF EXISTS order_items_updated_at ON order_items;
CREATE TRIGGER order_items_updated_at
    BEFORE UPDATE ON order_items
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Payments trigger
DROP TRIGGER IF EXISTS payments_updated_at ON payments;
CREATE TRIGGER payments_updated_at
    BEFORE UPDATE ON payments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Refunds trigger
DROP TRIGGER IF EXISTS refunds_updated_at ON refunds;
CREATE TRIGGER refunds_updated_at
    BEFORE UPDATE ON refunds
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Carts trigger
DROP TRIGGER IF EXISTS carts_updated_at ON carts;
CREATE TRIGGER carts_updated_at
    BEFORE UPDATE ON carts
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Cart items trigger
DROP TRIGGER IF EXISTS cart_items_updated_at ON cart_items;
CREATE TRIGGER cart_items_updated_at
    BEFORE UPDATE ON cart_items
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Coupons trigger
DROP TRIGGER IF EXISTS coupons_updated_at ON coupons;
CREATE TRIGGER coupons_updated_at
    BEFORE UPDATE ON coupons
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- API keys trigger
DROP TRIGGER IF EXISTS api_keys_updated_at ON api_keys;
CREATE TRIGGER api_keys_updated_at
    BEFORE UPDATE ON api_keys
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Subscriptions trigger
DROP TRIGGER IF EXISTS subscriptions_updated_at ON subscriptions;
CREATE TRIGGER subscriptions_updated_at
    BEFORE UPDATE ON subscriptions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Subscription items trigger
DROP TRIGGER IF EXISTS subscription_items_updated_at ON subscription_items;
CREATE TRIGGER subscription_items_updated_at
    BEFORE UPDATE ON subscription_items
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Fulfillments trigger
DROP TRIGGER IF EXISTS fulfillments_updated_at ON fulfillments;
CREATE TRIGGER fulfillments_updated_at
    BEFORE UPDATE ON fulfillments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Product categories trigger
DROP TRIGGER IF EXISTS product_categories_updated_at ON product_categories;
CREATE TRIGGER product_categories_updated_at
    BEFORE UPDATE ON product_categories
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Collections trigger
DROP TRIGGER IF EXISTS collections_updated_at ON collections;
CREATE TRIGGER collections_updated_at
    BEFORE UPDATE ON collections
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Bundle components trigger
DROP TRIGGER IF EXISTS bundle_components_updated_at ON bundle_components;
CREATE TRIGGER bundle_components_updated_at
    BEFORE UPDATE ON bundle_components
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Order item downloads trigger
DROP TRIGGER IF EXISTS order_item_downloads_updated_at ON order_item_downloads;
CREATE TRIGGER order_item_downloads_updated_at
    BEFORE UPDATE ON order_item_downloads
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- License keys trigger
DROP TRIGGER IF EXISTS license_keys_updated_at ON license_keys;
CREATE TRIGGER license_keys_updated_at
    BEFORE UPDATE ON license_keys
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
