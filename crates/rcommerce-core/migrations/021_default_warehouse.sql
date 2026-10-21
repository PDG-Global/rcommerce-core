-- ============================================================================
-- Migration: Default inventory location
-- ============================================================================
-- Fresh installs previously ended up with no inventory_locations row, which
-- made order creation fail with "No active inventory location configured"
-- (see PDG-Global/rcommerce-core#2). Seed a single default warehouse when
-- the table is empty so every install has a fulfillment location.
-- ============================================================================

INSERT INTO inventory_locations (name, code)
SELECT 'Default Warehouse', 'MAIN'
WHERE NOT EXISTS (SELECT 1 FROM inventory_locations);
