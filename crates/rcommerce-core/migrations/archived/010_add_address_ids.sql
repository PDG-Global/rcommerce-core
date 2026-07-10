-- Add shipping_address_id and billing_address_id columns

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'shipping_address_id') THEN
        ALTER TABLE orders ADD COLUMN shipping_address_id UUID;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'billing_address_id') THEN
        ALTER TABLE orders ADD COLUMN billing_address_id UUID;
    END IF;
END $$;
