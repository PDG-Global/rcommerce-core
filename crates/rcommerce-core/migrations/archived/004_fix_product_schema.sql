-- Migration 004: Fix product schema
-- NOTE: This migration is now incorporated into 001_initial_schema.sql
-- This file is kept for backwards compatibility with existing deployments
-- For new installations, all schema is created in migration 001

-- This migration does nothing if the schema is already correct
-- It's safe to run multiple times

DO $$
BEGIN
    -- Check if product_type column exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'products' AND column_name = 'product_type'
    ) THEN
        -- This should not happen with new installations
        -- but handles legacy databases
        RAISE NOTICE 'product_type column missing - please run full migration';
    END IF;
END $$;
