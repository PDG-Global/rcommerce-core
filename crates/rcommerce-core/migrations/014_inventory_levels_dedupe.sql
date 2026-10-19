-- Migration 014: deduplicate inventory_levels per (product_id, variant_id, location_id)
--
-- The UNIQUE(product_id, variant_id, location_id) index treats NULL variant_id
-- as distinct, so legacy code paths that upserted without a variant filter
-- silently inserted extra rows for simple products instead of conflicting.
-- One long-lived production database accumulated 326 rows for 62 distinct
-- keys this way, and readers using fetch_optional/LIMIT 1 would return an
-- arbitrary (sometimes zero-stock) row.
--
-- This consolidates each key to a single row, summing the quantities, keeping
-- the oldest row (by created_at, id). The EXISTS guard makes it a no-op on
-- clean databases (fresh installs and stores provisioned from migrations)
-- and idempotent everywhere.

DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM (
      SELECT product_id, variant_id, location_id
      FROM inventory_levels
      GROUP BY product_id, variant_id, location_id
      HAVING COUNT(*) > 1
    ) duplicates
  ) THEN
    -- Fold duplicate rows into the oldest row per key, summing quantities.
    WITH ranked AS (
      SELECT
        id,
        ROW_NUMBER() OVER (
          PARTITION BY product_id, variant_id, location_id
          ORDER BY created_at, id
        ) AS rn,
        SUM(available_quantity) OVER (PARTITION BY product_id, variant_id, location_id) AS sum_available,
        SUM(reserved_quantity) OVER (PARTITION BY product_id, variant_id, location_id) AS sum_reserved,
        SUM(incoming_quantity) OVER (PARTITION BY product_id, variant_id, location_id) AS sum_incoming
      FROM inventory_levels
    )
    UPDATE inventory_levels il
    SET available_quantity = r.sum_available,
        reserved_quantity = r.sum_reserved,
        incoming_quantity = r.sum_incoming,
        updated_at = NOW()
    FROM ranked r
    WHERE il.id = r.id AND r.rn = 1;

    -- Remove the remaining duplicate rows.
    WITH ranked AS (
      SELECT
        id,
        ROW_NUMBER() OVER (
          PARTITION BY product_id, variant_id, location_id
          ORDER BY created_at, id
        ) AS rn
      FROM inventory_levels
    )
    DELETE FROM inventory_levels il
    USING ranked r
    WHERE il.id = r.id AND r.rn > 1;
  END IF;
END $$;
