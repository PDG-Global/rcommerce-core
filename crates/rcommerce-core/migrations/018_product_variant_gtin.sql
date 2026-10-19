-- Migration 018: add GTIN/UPC/EAN identifier to product variants
--
-- Variant-level SKUs (e.g. wheels sold in distinct durometer stages, decks
-- sold as deck-only or complete setups) carry their own UPCs. Same shape as
-- products.gtin (017): digits only, 8-14 chars; validation lives in the
-- storefront/feed layers.

ALTER TABLE product_variants ADD COLUMN IF NOT EXISTS gtin VARCHAR(14);
