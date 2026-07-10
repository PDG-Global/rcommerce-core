-- Add all missing order columns to match the Order model

-- Add payment_method column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'payment_method') THEN
        ALTER TABLE orders ADD COLUMN payment_method VARCHAR(100);
    END IF;
END $$;

-- Add shipping_method column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'shipping_method') THEN
        ALTER TABLE orders ADD COLUMN shipping_method VARCHAR(100);
    END IF;
END $$;

-- Add subscription_id column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'subscription_id') THEN
        ALTER TABLE orders ADD COLUMN subscription_id UUID;
    END IF;
END $$;

-- Add billing_cycle column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'billing_cycle') THEN
        ALTER TABLE orders ADD COLUMN billing_cycle INTEGER;
    END IF;
END $$;
