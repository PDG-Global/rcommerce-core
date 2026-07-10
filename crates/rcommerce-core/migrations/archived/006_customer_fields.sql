-- Add missing customer fields to match the model

-- First, ensure the currency enum type exists
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'currency') THEN
        CREATE TYPE currency AS ENUM ('USD', 'EUR', 'GBP', 'JPY', 'AUD', 'CAD', 'CNY', 'HKD', 'SGD');
    END IF;
END $$;

-- Add or fix currency field
DO $$
BEGIN
    -- Check if currency column exists
    IF EXISTS (SELECT 1 FROM information_schema.columns 
               WHERE table_name = 'customers' AND column_name = 'currency') THEN
        -- Check if it's VARCHAR (wrong type from previous migration)
        IF EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'currency' 
                   AND data_type = 'character varying') THEN
            -- Drop the VARCHAR column and recreate with proper type
            ALTER TABLE customers DROP COLUMN currency;
            ALTER TABLE customers ADD COLUMN currency currency NOT NULL DEFAULT 'USD';
        END IF;
    ELSE
        -- Add currency column with proper type
        ALTER TABLE customers ADD COLUMN currency currency NOT NULL DEFAULT 'USD';
    END IF;
END $$;

-- Add tax_exempt field
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'tax_exempt') THEN
        ALTER TABLE customers ADD COLUMN tax_exempt BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

-- Add confirmed_at field
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'confirmed_at') THEN
        ALTER TABLE customers ADD COLUMN confirmed_at TIMESTAMPTZ;
    END IF;
END $$;

-- Add timezone field
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'timezone') THEN
        ALTER TABLE customers ADD COLUMN timezone VARCHAR(50);
    END IF;
END $$;

-- Add marketing_opt_in field
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'marketing_opt_in') THEN
        ALTER TABLE customers ADD COLUMN marketing_opt_in BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

-- Add email_notifications field
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'email_notifications') THEN
        ALTER TABLE customers ADD COLUMN email_notifications BOOLEAN NOT NULL DEFAULT true;
    END IF;
END $$;

-- Add sms_notifications field
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'sms_notifications') THEN
        ALTER TABLE customers ADD COLUMN sms_notifications BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

-- Add push_notifications field
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'push_notifications') THEN
        ALTER TABLE customers ADD COLUMN push_notifications BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;
