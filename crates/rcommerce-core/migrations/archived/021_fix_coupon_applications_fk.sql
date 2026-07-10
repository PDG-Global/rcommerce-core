-- Fix foreign key in coupon_applications table
-- The table currently references product_categories(id) but should reference collections(id)
-- based on the model structure where coupons apply to collections

-- First, drop the existing foreign key constraint if it exists
-- Note: We need to find the constraint name dynamically
DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    -- Find the FK constraint on collection_id
    SELECT tc.constraint_name INTO constraint_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu 
        ON tc.constraint_name = kcu.constraint_name
    WHERE tc.table_name = 'coupon_applications'
        AND tc.constraint_type = 'FOREIGN KEY'
        AND kcu.column_name = 'collection_id';
    
    IF constraint_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE coupon_applications DROP CONSTRAINT %I', constraint_name);
    END IF;
END $$;

-- Add the new foreign key constraint referencing collections(id)
-- This will fail if there are existing invalid references
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu 
            ON tc.constraint_name = kcu.constraint_name
        WHERE tc.table_name = 'coupon_applications'
            AND tc.constraint_type = 'FOREIGN KEY'
            AND kcu.column_name = 'collection_id'
    ) THEN
        ALTER TABLE coupon_applications 
            ADD CONSTRAINT fk_coupon_applications_collection 
            FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE;
    END IF;
END $$;
