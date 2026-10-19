-- Migration 017: add GTIN/UPC/EAN identifier to products
--
-- Google Search Console product structured data (and the upcoming Google
-- Merchant Center feed) need a real GTIN per product. Stored as digits only
-- (UPC-A 12, EAN-13, ITF-14 all fit); length/format validation lives in the
-- storefront and feed layers, not the database.

ALTER TABLE products ADD COLUMN IF NOT EXISTS gtin VARCHAR(14);
