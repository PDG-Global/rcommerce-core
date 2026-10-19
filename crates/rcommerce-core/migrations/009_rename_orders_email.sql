-- Migration 009: align orders.email with the code's customer_email column.
-- Fresh installs of 001_complete_schema.sql created orders.email, but the
-- order repository reads/writes customer_email (88's production DB was fixed
-- manually). Rename idempotently so existing and new databases converge.

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'orders' AND column_name = 'email'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'orders' AND column_name = 'customer_email'
    ) THEN
        ALTER TABLE orders RENAME COLUMN email TO customer_email;
    END IF;
END $$;
