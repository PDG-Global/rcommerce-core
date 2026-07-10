-- Migration: Order Downloads and License Keys
-- Tracks digital product downloads and license key distribution

-- Order item downloads table
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

CREATE INDEX idx_order_item_downloads_token ON order_item_downloads(download_token);
CREATE INDEX idx_order_item_downloads_order_item ON order_item_downloads(order_item_id);
CREATE INDEX idx_order_item_downloads_customer ON order_item_downloads(customer_id);

-- License keys table for digital products
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

CREATE INDEX idx_license_keys_product ON license_keys(product_id);
CREATE INDEX idx_license_keys_order_item ON license_keys(order_item_id);
CREATE INDEX idx_license_keys_customer ON license_keys(customer_id);
CREATE INDEX idx_license_keys_key ON license_keys(license_key);

-- Add download tracking to order items
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS is_digital BOOLEAN DEFAULT FALSE;
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS download_url TEXT;
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS license_key TEXT;
