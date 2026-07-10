-- Fix address table columns to match model
-- Model expects: is_default_shipping and is_default_billing (booleans)
-- Table only has: is_default (single boolean)

-- Add is_default_shipping column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'addresses' AND column_name = 'is_default_shipping') THEN
        ALTER TABLE addresses ADD COLUMN is_default_shipping BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

-- Add is_default_billing column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'addresses' AND column_name = 'is_default_billing') THEN
        ALTER TABLE addresses ADD COLUMN is_default_billing BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

-- Migrate existing data: if is_default was true, set both new columns to true
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns 
               WHERE table_name = 'addresses' AND column_name = 'is_default') THEN
        UPDATE addresses SET 
            is_default_shipping = is_default,
            is_default_billing = is_default
        WHERE is_default = true;
    END IF;
END $$;

-- Drop the old is_default column
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns 
               WHERE table_name = 'addresses' AND column_name = 'is_default') THEN
        ALTER TABLE addresses DROP COLUMN is_default;
    END IF;
END $$;
