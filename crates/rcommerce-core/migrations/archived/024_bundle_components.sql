-- Migration: Bundle Products Support
-- Adds bundle component management and pricing strategies

-- Bundle components table
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

CREATE INDEX idx_bundle_components_bundle ON bundle_components(bundle_product_id);
CREATE INDEX idx_bundle_components_component ON bundle_components(component_product_id);

-- Add bundle pricing fields to products
ALTER TABLE products ADD COLUMN IF NOT EXISTS bundle_pricing_strategy VARCHAR(20);
ALTER TABLE products ADD COLUMN IF NOT EXISTS bundle_discount_percentage DECIMAL(5,2);

-- Add bundle reference to order items
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS bundle_parent_id UUID REFERENCES order_items(id) ON DELETE SET NULL;
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS is_bundle_component BOOLEAN DEFAULT FALSE;

CREATE INDEX idx_order_items_bundle_parent ON order_items(bundle_parent_id);
