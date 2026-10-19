-- Migration 016: reconcile fresh-install schema drift with the original
-- long-lived production schema
--
-- Databases provisioned straight from migrations 001-015 drifted from the
-- original production schema in four places, all of which broke real
-- flows on freshly provisioned stores (discovered on a newly provisioned
-- store, where guest cart creation 500'd):
--
-- 1. carts.email: 001 declared customer_email, but the cart repository has
--    always written `email` (the original production database has it from
--    the pre-consolidation
--    migration path). Guest cart creation fails with "column email does not
--    exist" on fresh installs.
-- 2. carts.currency: 001 declared VARCHAR(8), but the Cart model decodes the
--    `currency` ENUM, so reading a cart back fails with a type mismatch.
-- 3. orders.currency: fresh installs kept the ENUM column with a VARCHAR
--    default cast left over from migration 010's partial conversion. The
--    code binds orders.currency as text (same as 010's premise), so align
--    the column and default fully.
-- 4. shipment_tracking: fresh installs lack the id/created_at/date_shipped
--    column defaults the live schema has, so inserts that rely on them fail.

-- 1. carts.customer_email -> carts.email
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'carts' AND column_name = 'customer_email'
  ) AND NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'carts' AND column_name = 'email'
  ) THEN
    ALTER TABLE carts RENAME COLUMN customer_email TO email;
  END IF;
END $$;

-- 2. carts.currency VARCHAR -> currency ENUM
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'carts' AND column_name = 'currency' AND data_type = 'character varying'
  ) THEN
    ALTER TABLE carts ALTER COLUMN currency DROP DEFAULT;
    ALTER TABLE carts ALTER COLUMN currency TYPE currency USING currency::text::currency;
    ALTER TABLE carts ALTER COLUMN currency SET DEFAULT 'USD'::currency;
  END IF;
END $$;

-- 3. orders.currency: full VARCHAR alignment (column and default)
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'orders' AND column_name = 'currency' AND data_type = 'USER-DEFINED'
  ) THEN
    ALTER TABLE orders ALTER COLUMN currency DROP DEFAULT;
    ALTER TABLE orders ALTER COLUMN currency TYPE VARCHAR(3) USING currency::text;
    ALTER TABLE orders ALTER COLUMN currency SET DEFAULT 'USD';
  END IF;
END $$;

-- 4. shipment_tracking column defaults
ALTER TABLE shipment_tracking ALTER COLUMN id SET DEFAULT uuid_generate_v4();
ALTER TABLE shipment_tracking ALTER COLUMN created_at SET DEFAULT now();
ALTER TABLE shipment_tracking ALTER COLUMN date_shipped SET DEFAULT now();
