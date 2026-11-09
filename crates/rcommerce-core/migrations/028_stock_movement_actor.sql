-- ============================================================================
-- Migration 028: who changed stock, and why
-- ============================================================================
-- Stock adjustments and goods receipts made in the back-office record the
-- staff account that made them and a reason code (receipt, correction,
-- damage, found, return, count). Movements written by orders keep both NULL.
-- ============================================================================

ALTER TABLE stock_movements ADD COLUMN IF NOT EXISTS reason VARCHAR(32);
ALTER TABLE stock_movements
    ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES customers(id) ON DELETE SET NULL;

-- Movement history per location, newest first
CREATE INDEX IF NOT EXISTS idx_stock_movements_location_created
    ON stock_movements(location_id, created_at DESC);
