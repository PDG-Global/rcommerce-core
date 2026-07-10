-- Migration: Digital Products Support
-- Adds file handling and download management for digital products

-- Add digital product fields to products table
ALTER TABLE products ADD COLUMN IF NOT EXISTS file_url TEXT;
ALTER TABLE products ADD COLUMN IF NOT EXISTS file_size BIGINT;
ALTER TABLE products ADD COLUMN IF NOT EXISTS file_hash TEXT;
ALTER TABLE products ADD COLUMN IF NOT EXISTS download_limit INTEGER DEFAULT NULL;
ALTER TABLE products ADD COLUMN IF NOT EXISTS license_key_enabled BOOLEAN DEFAULT FALSE;
ALTER TABLE products ADD COLUMN IF NOT EXISTS download_expiry_days INTEGER DEFAULT NULL;

-- Create index for digital product queries
CREATE INDEX IF NOT EXISTS idx_products_product_type ON products(product_type);
