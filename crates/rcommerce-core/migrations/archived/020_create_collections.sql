-- Create collections table for product collections
-- Based on models/product.rs Collection struct

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

-- Indexes
CREATE INDEX IF NOT EXISTS idx_collections_handle ON collections(handle);
CREATE INDEX IF NOT EXISTS idx_collections_published_at ON collections(published_at) WHERE published_at IS NOT NULL;

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_collections_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS collections_updated_at ON collections;
CREATE TRIGGER collections_updated_at
    BEFORE UPDATE ON collections
    FOR EACH ROW
    EXECUTE FUNCTION update_collections_updated_at();

-- Create join table for products and collections (many-to-many)
CREATE TABLE IF NOT EXISTS collection_products (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(collection_id, product_id)
);

-- Indexes for join table
CREATE INDEX IF NOT EXISTS idx_collection_products_collection_id ON collection_products(collection_id);
CREATE INDEX IF NOT EXISTS idx_collection_products_product_id ON collection_products(product_id);
