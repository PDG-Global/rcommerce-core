-- Accounting / reconciliation export support.
--
-- Adds the columns the accounting export API and Airwallex settlement
-- reconciliation need: store-prefixed invoice numbers, test-mode flags, and
-- fee/settlement capture on payments. See docs/api/05-accounting-export-api.md.

-- Stable store-prefixed reference (e.g. "88-1024") that matches the Xero
-- invoice number and the order_id sent to payment providers.
ALTER TABLE orders ADD COLUMN IF NOT EXISTS invoice_number VARCHAR(32);

CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_invoice_number
    ON orders(invoice_number) WHERE invoice_number IS NOT NULL;

-- Test-mode flag (spec 2.2): set for sandbox-mode, admin test orders, or
-- flagged test payment methods. Never hidden by default in exports.
ALTER TABLE orders ADD COLUMN IF NOT EXISTS test BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE payments ADD COLUMN IF NOT EXISTS test BOOLEAN NOT NULL DEFAULT FALSE;

-- Settlement reconciliation captured from the Airwallex settlement_records
-- API (or equivalent). All fee amounts are in settlement_currency.
ALTER TABLE payments ADD COLUMN IF NOT EXISTS transaction_id VARCHAR(255);
ALTER TABLE payments ADD COLUMN IF NOT EXISTS fee_amount NUMERIC(20,2);
ALTER TABLE payments ADD COLUMN IF NOT EXISTS fee_currency VARCHAR(3);
ALTER TABLE payments ADD COLUMN IF NOT EXISTS fee_components JSONB;
ALTER TABLE payments ADD COLUMN IF NOT EXISTS settlement_id VARCHAR(255);
ALTER TABLE payments ADD COLUMN IF NOT EXISTS settled_at TIMESTAMPTZ;
ALTER TABLE payments ADD COLUMN IF NOT EXISTS settlement_currency VARCHAR(3);
ALTER TABLE payments ADD COLUMN IF NOT EXISTS fx_rate NUMERIC(20,8);

ALTER TABLE refunds ADD COLUMN IF NOT EXISTS settled_at TIMESTAMPTZ;

-- Per-store sequence backing orders.invoice_number (one DB == one store).
CREATE SEQUENCE IF NOT EXISTS order_invoice_seq START 1;

CREATE INDEX IF NOT EXISTS idx_payments_settlement ON payments(settlement_id)
    WHERE settlement_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_orders_updated_at ON orders(updated_at);
