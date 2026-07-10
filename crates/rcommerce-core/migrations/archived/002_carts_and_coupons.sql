-- Migration: Carts and Coupons
-- Creates tables for shopping carts and discount coupons

-- ============================================
-- Cart Tables
-- ============================================

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

-- Index for looking up carts by customer
CREATE INDEX IF NOT EXISTS idx_carts_customer_id ON carts(customer_id) WHERE customer_id IS NOT NULL;

-- Index for looking up carts by session token (guest carts)
CREATE INDEX IF NOT EXISTS idx_carts_session_token ON carts(session_token) WHERE session_token IS NOT NULL;

-- Index for cart expiration cleanup
CREATE INDEX IF NOT EXISTS idx_carts_expires_at ON carts(expires_at) WHERE expires_at IS NOT NULL;

-- Index for converted carts
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

-- Index for cart items
CREATE INDEX IF NOT EXISTS idx_cart_items_cart_id ON cart_items(cart_id);
CREATE INDEX IF NOT EXISTS idx_cart_items_product_id ON cart_items(product_id);

-- Unique constraint to prevent duplicate items in cart
CREATE UNIQUE INDEX IF NOT EXISTS idx_cart_items_unique 
ON cart_items(cart_id, product_id, COALESCE(variant_id, '00000000-0000-0000-0000-000000000000'));

-- ============================================
-- Coupon Tables
-- ============================================

DO $$ BEGIN
    CREATE TYPE discount_type AS ENUM ('percentage', 'fixed_amount', 'free_shipping', 'buy_x_get_y');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

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

-- Index for active coupons
CREATE INDEX IF NOT EXISTS idx_coupons_active ON coupons(is_active) WHERE is_active = TRUE;

-- Index for coupon code lookup
CREATE INDEX IF NOT EXISTS idx_coupons_code ON coupons(code);

CREATE TABLE IF NOT EXISTS coupon_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    coupon_id UUID NOT NULL REFERENCES coupons(id) ON DELETE CASCADE,
    product_id UUID REFERENCES products(id) ON DELETE CASCADE,
    collection_id UUID REFERENCES product_categories(id) ON DELETE CASCADE,
    is_exclusion BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Ensure either product_id or collection_id is set, but not both
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

-- ============================================
-- Update Orders table to track coupon usage
-- ============================================

ALTER TABLE orders ADD COLUMN IF NOT EXISTS coupon_code VARCHAR(50);
ALTER TABLE orders ADD COLUMN IF NOT EXISTS coupon_id UUID REFERENCES coupons(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_orders_coupon ON orders(coupon_id) WHERE coupon_id IS NOT NULL;
