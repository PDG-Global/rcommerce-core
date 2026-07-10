-- Create fulfillments table for order fulfillment tracking
-- Based on models/order.rs Fulfillment struct

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

-- Indexes
CREATE INDEX IF NOT EXISTS idx_fulfillments_order_id ON fulfillments(order_id);
CREATE INDEX IF NOT EXISTS idx_fulfillments_status ON fulfillments(status);
CREATE INDEX IF NOT EXISTS idx_fulfillments_tracking_number ON fulfillments(tracking_number) WHERE tracking_number IS NOT NULL;

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_fulfillments_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS fulfillments_updated_at ON fulfillments;
CREATE TRIGGER fulfillments_updated_at
    BEFORE UPDATE ON fulfillments
    FOR EACH ROW
    EXECUTE FUNCTION update_fulfillments_updated_at();
