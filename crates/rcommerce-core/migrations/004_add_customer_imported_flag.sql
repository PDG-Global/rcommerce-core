-- Add is_imported flag to customers table
-- Used to identify accounts imported from external platforms (e.g. WooCommerce)
-- that do not have a local password hash and must reset their password to log in.
ALTER TABLE customers
    ADD COLUMN IF NOT EXISTS is_imported BOOLEAN NOT NULL DEFAULT false;

-- Backfill existing imported customers: any customer without a password hash
-- is considered imported until they set a password via reset flow.
UPDATE customers
SET is_imported = true
WHERE password_hash IS NULL;
