-- Add email_verified flag to customers table
ALTER TABLE customers
    ADD COLUMN IF NOT EXISTS email_verified BOOLEAN NOT NULL DEFAULT false;
