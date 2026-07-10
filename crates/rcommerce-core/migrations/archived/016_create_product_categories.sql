-- Create product_categories table
-- This table is referenced by coupon_applications.collection_id foreign key
-- and is also needed for product categorization

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

-- Add is_active column if table exists but column doesn't (for backwards compatibility)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'product_categories')
       AND NOT EXISTS (SELECT 1 FROM information_schema.columns 
                       WHERE table_name = 'product_categories' AND column_name = 'is_active') THEN
        ALTER TABLE product_categories ADD COLUMN is_active BOOLEAN NOT NULL DEFAULT true;
    END IF;
END $$;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_product_categories_slug ON product_categories(slug);
CREATE INDEX IF NOT EXISTS idx_product_categories_parent_id ON product_categories(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_product_categories_is_active ON product_categories(is_active) WHERE is_active = true;

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_product_categories_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS product_categories_updated_at ON product_categories;
CREATE TRIGGER product_categories_updated_at
    BEFORE UPDATE ON product_categories
    FOR EACH ROW
    EXECUTE FUNCTION update_product_categories_updated_at();
