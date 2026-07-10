-- Fix currency column type from VARCHAR to currency enum

-- First ensure the currency enum type exists
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'currency') THEN
        CREATE TYPE currency AS ENUM ('USD', 'EUR', 'GBP', 'JPY', 'AUD', 'CAD', 'CNY', 'HKD', 'SGD');
    END IF;
END $$;

-- Fix currency column if it's VARCHAR
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns 
               WHERE table_name = 'customers' AND column_name = 'currency' 
               AND data_type = 'character varying') THEN
        -- Drop the VARCHAR column and recreate with proper type
        ALTER TABLE customers DROP COLUMN currency;
        ALTER TABLE customers ADD COLUMN currency currency NOT NULL DEFAULT 'USD';
    ELSIF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                      WHERE table_name = 'customers' AND column_name = 'currency') THEN
        -- Add currency column with proper type if missing
        ALTER TABLE customers ADD COLUMN currency currency NOT NULL DEFAULT 'USD';
    END IF;
END $$;
