-- Add role column to customers table
-- Model has: role: CustomerRole field
-- Table missing: role column

-- Create customer_role enum type if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'customer_role') THEN
        CREATE TYPE customer_role AS ENUM ('customer', 'manager', 'admin');
    END IF;
END $$;

-- Add role column to customers table
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'customers' AND column_name = 'role') THEN
        ALTER TABLE customers ADD COLUMN role customer_role NOT NULL DEFAULT 'customer';
    END IF;
END $$;
