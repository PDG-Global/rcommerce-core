-- Add missing columns to order_items table

-- Add subtotal column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'order_items' AND column_name = 'subtotal') THEN
        ALTER TABLE order_items ADD COLUMN subtotal DECIMAL(20, 2) NOT NULL DEFAULT 0;
    END IF;
END $$;

-- Add tax_amount column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'order_items' AND column_name = 'tax_amount') THEN
        ALTER TABLE order_items ADD COLUMN tax_amount DECIMAL(20, 2) NOT NULL DEFAULT 0;
    END IF;
END $$;

-- Add is_gift_card column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'order_items' AND column_name = 'is_gift_card') THEN
        ALTER TABLE order_items ADD COLUMN is_gift_card BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

-- Add weight column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'order_items' AND column_name = 'weight') THEN
        ALTER TABLE order_items ADD COLUMN weight DECIMAL(10, 2);
    END IF;
END $$;

-- Add weight_unit column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'order_items' AND column_name = 'weight_unit') THEN
        ALTER TABLE order_items ADD COLUMN weight_unit VARCHAR(20);
    END IF;
END $$;

-- Add image_url column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'order_items' AND column_name = 'image_url') THEN
        ALTER TABLE order_items ADD COLUMN image_url VARCHAR(500);
    END IF;
END $$;
