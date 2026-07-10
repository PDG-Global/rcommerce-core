-- Add missing columns to orders table for MVP

-- Create order_type enum first
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'order_type') THEN
        CREATE TYPE order_type AS ENUM ('one_time', 'subscription_initial', 'subscription_renewal');
    END IF;
END $$;

-- Add order_type column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'order_type') THEN
        ALTER TABLE orders ADD COLUMN order_type order_type NOT NULL DEFAULT 'one_time';
    END IF;
END $$;

-- Add tags column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'tags') THEN
        ALTER TABLE orders ADD COLUMN tags TEXT[] DEFAULT ARRAY[]::TEXT[];
    END IF;
END $$;

-- Add metadata column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'metadata') THEN
        ALTER TABLE orders ADD COLUMN metadata JSONB DEFAULT '{}'::JSONB;
    END IF;
END $$;

-- Add draft column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'draft') THEN
        ALTER TABLE orders ADD COLUMN draft BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;
