-- Migration 010: align orders.currency with the code (String bindings).
-- The orders model binds currency as plain text (production databases use
-- VARCHAR), while every other currency column stays the `currency` ENUM that
-- the models decode. Fresh databases created from the fixed 001 already get
-- VARCHAR for orders; this converts any database created before the fix.

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'orders' AND column_name = 'currency'
          AND data_type = 'USER-DEFINED'
    ) THEN
        ALTER TABLE orders ALTER COLUMN currency TYPE VARCHAR(8) USING currency::text;
    END IF;
END $$;
