-- Create subscription_items table for subscription line items
-- Based on models/subscription.rs SubscriptionItem struct

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

-- Indexes
CREATE INDEX IF NOT EXISTS idx_subscription_items_subscription_id ON subscription_items(subscription_id);
CREATE INDEX IF NOT EXISTS idx_subscription_items_product_id ON subscription_items(product_id);

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_subscription_items_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS subscription_items_updated_at ON subscription_items;
CREATE TRIGGER subscription_items_updated_at
    BEFORE UPDATE ON subscription_items
    FOR EACH ROW
    EXECUTE FUNCTION update_subscription_items_updated_at();
