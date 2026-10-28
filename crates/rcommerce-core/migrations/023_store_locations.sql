-- ============================================================================
-- Migration: Store locations (branches, pickup points) and opening hours
-- ============================================================================
-- Inventory locations become storefront-visible branches when marked public:
-- address, contact details, coordinates, services, opening hours and special
-- days (bank holidays, early closing). The pickup lead time drives the
-- "ready for collection at" estimate. Existing locations stay private, so
-- nothing changes for stores that do not use this.
-- ============================================================================

ALTER TABLE inventory_locations
    ADD COLUMN IF NOT EXISTS slug VARCHAR(100),
    ADD COLUMN IF NOT EXISTS is_public BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS is_pickup_point BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS phone VARCHAR(50),
    ADD COLUMN IF NOT EXISTS email VARCHAR(255),
    ADD COLUMN IF NOT EXISTS address_line1 VARCHAR(255),
    ADD COLUMN IF NOT EXISTS address_line2 VARCHAR(255),
    ADD COLUMN IF NOT EXISTS postal_code VARCHAR(20),
    ADD COLUMN IF NOT EXISTS city VARCHAR(100),
    ADD COLUMN IF NOT EXISTS country_code CHAR(2),
    ADD COLUMN IF NOT EXISTS latitude DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS timezone VARCHAR(64) NOT NULL DEFAULT 'UTC',
    ADD COLUMN IF NOT EXISTS pickup_lead_minutes INTEGER NOT NULL DEFAULT 60,
    ADD COLUMN IF NOT EXISTS services TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS manager_name VARCHAR(255),
    ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE inventory_locations
    DROP CONSTRAINT IF EXISTS inventory_locations_pickup_lead_check;
ALTER TABLE inventory_locations
    ADD CONSTRAINT inventory_locations_pickup_lead_check CHECK (pickup_lead_minutes >= 0);

CREATE UNIQUE INDEX IF NOT EXISTS idx_inventory_locations_slug
    ON inventory_locations(slug) WHERE slug IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_inventory_locations_public
    ON inventory_locations(sort_order, name) WHERE is_public = true;

-- Weekly opening hours, in the location's local time. A day can have several
-- ranges (closed at lunch). ISO weekday: 1 = Monday ... 7 = Sunday.
CREATE TABLE IF NOT EXISTS location_opening_hours (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    location_id UUID NOT NULL REFERENCES inventory_locations(id) ON DELETE CASCADE,
    weekday SMALLINT NOT NULL CHECK (weekday BETWEEN 1 AND 7),
    opens_at TIME NOT NULL,
    closes_at TIME NOT NULL,
    CHECK (closes_at > opens_at)
);
CREATE INDEX IF NOT EXISTS idx_location_opening_hours_location
    ON location_opening_hours(location_id, weekday, opens_at);

-- Days that differ from the week: closed all day (no times) or special hours.
CREATE TABLE IF NOT EXISTS location_closures (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    location_id UUID NOT NULL REFERENCES inventory_locations(id) ON DELETE CASCADE,
    date DATE NOT NULL,
    opens_at TIME,
    closes_at TIME,
    reason VARCHAR(255),
    CHECK ((opens_at IS NULL AND closes_at IS NULL)
        OR (opens_at IS NOT NULL AND closes_at IS NOT NULL AND closes_at > opens_at))
);
CREATE INDEX IF NOT EXISTS idx_location_closures_location_date
    ON location_closures(location_id, date);

-- Translated text for a location (description, pickup instructions)
CREATE TABLE IF NOT EXISTS location_translations (
    location_id UUID NOT NULL REFERENCES inventory_locations(id) ON DELETE CASCADE,
    locale VARCHAR(10) NOT NULL,
    description TEXT,
    pickup_instructions TEXT,
    PRIMARY KEY (location_id, locale)
);
