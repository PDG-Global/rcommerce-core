-- Migration 015: align stock_movements.movement_type with the code's enum
--
-- The Rust model binds movement_type as the `stock_movement_type` ENUM, but
-- migration 003 created the column as VARCHAR(20). The original production
-- database was converted by hand; any database provisioned through the
-- migrator (fresh installs) still has VARCHAR, which breaks receive_stock /
-- movement reads with a type-mismatch error. This is the same drift family
-- as migration 013 (stock_reservations.status).
--
-- Guarded: databases where the column is already a user-defined type (the
-- enum) are left untouched, so this is a no-op there.

DO $$
BEGIN
  IF to_regclass('stock_movements') IS NOT NULL
     AND EXISTS (
       SELECT 1 FROM information_schema.columns
       WHERE table_name = 'stock_movements'
         AND column_name = 'movement_type'
         AND data_type <> 'USER-DEFINED'
     ) THEN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'stock_movement_type') THEN
      -- 'adjustment' is kept so legacy rows with that CHECK-allowed value convert.
      CREATE TYPE stock_movement_type AS ENUM
        ('in', 'out', 'return', 'lost', 'found', 'transfer', 'adjustment');
    END IF;
    -- The VARCHAR CHECK constraint compares against varchar literals and would
    -- break once the column becomes an enum.
    ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_movement_type_check;
    ALTER TABLE stock_movements ALTER COLUMN movement_type DROP DEFAULT;
    ALTER TABLE stock_movements ALTER COLUMN movement_type TYPE stock_movement_type
      USING movement_type::text::stock_movement_type;
  END IF;
END $$;
