-- Create order_notes table for order notes
-- Based on models/order.rs OrderNote struct

CREATE TABLE IF NOT EXISTS order_notes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    author VARCHAR(255),
    note TEXT NOT NULL,
    is_customer_notified BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_order_notes_order_id ON order_notes(order_id);
CREATE INDEX IF NOT EXISTS idx_order_notes_created_at ON order_notes(created_at);
