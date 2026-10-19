-- Fix order items that were created with generic placeholder titles so they show the
-- actual product/variant title and SKU.
WITH product_details AS (
    SELECT
        p.id AS product_id,
        p.title AS product_title,
        p.sku AS product_sku,
        NULL::uuid AS variant_id,
        NULL::text AS variant_title
    FROM products p
    UNION ALL
    SELECT
        p.id AS product_id,
        p.title AS product_title,
        p.sku AS product_sku,
        pv.id AS variant_id,
        pv.title AS variant_title
    FROM products p
    JOIN product_variants pv ON pv.product_id = p.id
)
UPDATE order_items oi
SET
    title = pd.product_title,
    sku = COALESCE(NULLIF(oi.sku, ''), pd.product_sku),
    variant_title = COALESCE(oi.variant_title, pd.variant_title)
FROM product_details pd
WHERE oi.product_id = pd.product_id
  AND oi.variant_id IS NOT DISTINCT FROM pd.variant_id
  AND (
      oi.title ~ '^Item [0-9]+$'
      OR oi.title ~ '^Product [0-9]+$'
  );
