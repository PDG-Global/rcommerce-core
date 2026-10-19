-- Migration 007: Notification queue, delivery attempts, and shipment tracking tables
--
-- Creates tables that are queried at runtime but were never created by any
-- previous migration:
--   1. notification_queue  - written by NotificationService::queue()/cancel_queued()
--                            (crates/rcommerce-core/src/notification/service.rs)
--   2. delivery_attempts   - read by NotificationService::get_history()/
--                            get_delivery_stats() (same file)
--   3. shipment_tracking   - WooCommerce Shipment Tracking REST API
--                            (crates/rcommerce-api/src/routes/woocommerce/tracking.rs)
--
-- All statements are idempotent (IF NOT EXISTS) so re-running is safe.

-- delivery_status enum is created by migration 003; guard anyway for standalone use.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'delivery_status') THEN
        CREATE TYPE delivery_status AS ENUM ('pending', 'sent', 'delivered', 'failed', 'bounced');
    END IF;
END$$;

-- ============================================================================
-- NOTIFICATION QUEUE
-- ============================================================================

-- Queued notifications for delayed sending.
-- Query sites (service.rs):
--   INSERT INTO notification_queue (id, notification_id, scheduled_at, status)
--     VALUES ($1, $2, $3, 'queued')
--   DELETE FROM notification_queue WHERE id = $1
CREATE TABLE IF NOT EXISTS notification_queue (
    id UUID PRIMARY KEY,
    notification_id UUID NOT NULL REFERENCES notifications(id) ON DELETE CASCADE,
    scheduled_at TIMESTAMPTZ NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'queued',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notification_queue_status ON notification_queue(status, scheduled_at);
CREATE INDEX IF NOT EXISTS idx_notification_queue_notification ON notification_queue(notification_id);

-- ============================================================================
-- DELIVERY ATTEMPTS
-- ============================================================================

-- One row per delivery attempt of a notification.
-- Query sites (service.rs):
--   JOIN delivery_attempts da ON n.id = da.notification_id
--   SELECT status, COUNT(*) FROM delivery_attempts
--     WHERE channel = '<channel>' AND created_at >= '<date>' GROUP BY status
CREATE TABLE IF NOT EXISTS delivery_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    notification_id UUID NOT NULL REFERENCES notifications(id) ON DELETE CASCADE,
    channel VARCHAR(50) NOT NULL,
    status delivery_status NOT NULL DEFAULT 'pending',
    error TEXT,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_delivery_attempts_notification ON delivery_attempts(notification_id);
CREATE INDEX IF NOT EXISTS idx_delivery_attempts_status ON delivery_attempts(status);
CREATE INDEX IF NOT EXISTS idx_delivery_attempts_created ON delivery_attempts(created_at);

-- ============================================================================
-- SHIPMENT TRACKING (WooCommerce Shipment Tracking REST API compatibility)
-- ============================================================================

-- WooCommerce-compatible shipment tracking records.
-- Query sites (rcommerce-api/src/routes/woocommerce/tracking.rs):
--   INSERT INTO shipment_tracking (id, order_id, tracking_number, tracking_provider, tracking_link, date_shipped)
--     VALUES ($1..$6) RETURNING *
--   SELECT * FROM shipment_tracking WHERE order_id = $1 ORDER BY created_at DESC
--   SELECT * FROM shipment_tracking WHERE id = $1 AND order_id = $2
--   UPDATE shipment_tracking SET tracking_number=$1, tracking_provider=$2,
--     tracking_link=$3, updated_at=NOW() WHERE id=$4
--
-- NOTE: order_id is BIGINT because WooCommerce order IDs are numeric and the
-- API handler parses the path parameter as i64. It is intentionally NOT a
-- foreign key to orders.id (which is UUID in this schema).
CREATE TABLE IF NOT EXISTS shipment_tracking (
    id UUID PRIMARY KEY,
    order_id BIGINT NOT NULL,
    tracking_number VARCHAR(255) NOT NULL,
    tracking_provider VARCHAR(255),
    tracking_link TEXT,
    date_shipped TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Guarded: the table may already exist in provisioned databases owned by another
-- role (e.g. created manually as the postgres superuser), in which case index
-- creation would fail with "must be owner of table". Skip gracefully then.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE schemaname = 'public' AND indexname = 'idx_shipment_tracking_order') THEN
        EXECUTE 'CREATE INDEX idx_shipment_tracking_order ON shipment_tracking(order_id)';
    END IF;
EXCEPTION WHEN insufficient_privilege THEN
    RAISE NOTICE 'skipping idx_shipment_tracking_order: insufficient privileges on shipment_tracking';
END$$;
