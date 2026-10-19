-- Migration 013: reconcile stock_reservations with the live checkout schema
--
-- The live 88 database predates migration 003 and evolved a different
-- stock_reservations shape; fresh installs of migrations 001-003 created
-- incompatible columns, which broke checkout on any store provisioned after
-- that. This brings fresh databases in line with the code's expectations
-- (and with the proven 88 production schema).
--
-- 1. Cart-level reservations are created before an order exists (checkout
--    reserves stock, then creates the order), so order_id must be nullable.
-- 2. The Rust model decodes `status` as the reservation_status ENUM, not
--    VARCHAR, so the column type (and its partial indexes) must match.

ALTER TABLE stock_reservations ALTER COLUMN order_id DROP NOT NULL;

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'reservation_status') THEN
    CREATE TYPE reservation_status AS ENUM ('active', 'committed', 'released', 'expired');
  END IF;
END $$;

-- VARCHAR -> ENUM conversion: partial index predicates with varchar casts are
-- not immutable, so drop and recreate them around the type change.
DROP INDEX IF EXISTS idx_stock_reservations_expires;
DROP INDEX IF EXISTS idx_stock_reservations_status;
ALTER TABLE stock_reservations ALTER COLUMN status DROP DEFAULT;
ALTER TABLE stock_reservations ALTER COLUMN status TYPE reservation_status USING status::reservation_status;
ALTER TABLE stock_reservations ALTER COLUMN status SET DEFAULT 'active'::reservation_status;
CREATE INDEX idx_stock_reservations_expires ON stock_reservations(expires_at) WHERE status = 'active'::reservation_status;
CREATE INDEX idx_stock_reservations_status ON stock_reservations(status) WHERE status = 'active'::reservation_status;

-- 3. order_items.metadata: the checkout/order pipeline writes per-item
--    metadata (bundle component flags, custom attributes) but no migration
--    ever added the column; it only exists on the live 88 database.
ALTER TABLE order_items ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;
