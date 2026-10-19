-- Migration 020: Subscription money movement
--
-- Adds the columns needed to charge stored-card consents for renewals:
-- - subscriptions.quantity: how many units of the product each cycle delivers
-- - subscriptions.shipping_address: delivery address snapshot for renewals
-- - customers.airwallex_customer_id: Airwallex customer for consent creation
-- Plus an index for the due-for-billing sweep.
--
-- Idempotent: safe to re-run (IF NOT EXISTS everywhere).

ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS quantity INTEGER NOT NULL DEFAULT 1;

ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS shipping_address JSONB;

ALTER TABLE customers ADD COLUMN IF NOT EXISTS airwallex_customer_id VARCHAR(255);

CREATE INDEX IF NOT EXISTS idx_subscriptions_due ON subscriptions(next_billing_at)
    WHERE status IN ('active', 'trialing');
