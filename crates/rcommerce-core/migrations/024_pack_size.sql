-- Pack-size pricing (issue #49).
--
-- Semantics:
--   * products.pack_size  = units per pack (NOT NULL, default 1 = sell single units).
--   * product_variants.pack_size = NULL means "inherit the product's value";
--     effective pack size = COALESCE(variant.pack_size, product.pack_size).
--   * Cart/order `quantity` counts PACKS; `price` stays the UNIT price.
--     Line total = unit_price * pack_size * quantity, computed server-side.
--   * Inventory is counted in UNITS everywhere; stock checks/reservations/
--     restocks convert packs -> units (quantity * pack_size).
--   * order_items.pack_size is a SNAPSHOT of the effective pack size at order
--     time; refund restock and invoices read the snapshot, never current values.

ALTER TABLE products
    ADD COLUMN IF NOT EXISTS pack_size INTEGER NOT NULL DEFAULT 1;

ALTER TABLE products
    DROP CONSTRAINT IF EXISTS products_pack_size_range;
ALTER TABLE products
    ADD CONSTRAINT products_pack_size_range
    CHECK (pack_size >= 1 AND pack_size <= 1000);

ALTER TABLE product_variants
    ADD COLUMN IF NOT EXISTS pack_size INTEGER;

ALTER TABLE product_variants
    DROP CONSTRAINT IF EXISTS product_variants_pack_size_range;
ALTER TABLE product_variants
    ADD CONSTRAINT product_variants_pack_size_range
    CHECK (pack_size IS NULL OR (pack_size >= 1 AND pack_size <= 1000));

ALTER TABLE order_items
    ADD COLUMN IF NOT EXISTS pack_size INTEGER NOT NULL DEFAULT 1;

ALTER TABLE order_items
    DROP CONSTRAINT IF EXISTS order_items_pack_size_range;
ALTER TABLE order_items
    ADD CONSTRAINT order_items_pack_size_range
    CHECK (pack_size >= 1 AND pack_size <= 1000);

COMMENT ON COLUMN products.pack_size IS
    'Units per pack (1..1000). quantity counts packs; line total = price * pack_size * quantity.';
COMMENT ON COLUMN product_variants.pack_size IS
    'Units per pack; NULL inherits products.pack_size. Effective = COALESCE(variant, product).';
COMMENT ON COLUMN order_items.pack_size IS
    'Snapshot of effective pack size at order time; used by refund restock and invoices.';
