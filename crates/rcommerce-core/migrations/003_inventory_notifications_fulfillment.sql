-- ============================================================================
-- Migration: Inventory, Notifications, Fulfillment, and Relations Tables
-- ============================================================================
-- This migration creates tables for:
-- - Inventory management (levels, reservations, movements, locations)
-- - Notification system (email, SMS, push)
-- - Fulfillment items
-- - Product category and tag relations
-- ============================================================================

-- ============================================================================
-- INVENTORY SYSTEM
-- ============================================================================

-- Inventory locations (warehouses, stores, etc.)
CREATE TABLE IF NOT EXISTS inventory_locations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    code VARCHAR(50) UNIQUE NOT NULL,
    address JSONB,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_inventory_locations_code ON inventory_locations(code);
CREATE INDEX IF NOT EXISTS idx_inventory_locations_active ON inventory_locations(is_active) WHERE is_active = true;

-- Inventory levels for products at locations
CREATE TABLE IF NOT EXISTS inventory_levels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    variant_id UUID REFERENCES product_variants(id) ON DELETE CASCADE,
    location_id UUID NOT NULL REFERENCES inventory_locations(id) ON DELETE CASCADE,
    available_quantity INTEGER NOT NULL DEFAULT 0,
    reserved_quantity INTEGER NOT NULL DEFAULT 0,
    incoming_quantity INTEGER NOT NULL DEFAULT 0,
    reorder_point INTEGER NOT NULL DEFAULT 0,
    reorder_quantity INTEGER NOT NULL DEFAULT 0,
    cost_per_unit DECIMAL(10,2),
    last_counted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(product_id, variant_id, location_id)
);

CREATE INDEX IF NOT EXISTS idx_inventory_levels_product ON inventory_levels(product_id);
CREATE INDEX IF NOT EXISTS idx_inventory_levels_variant ON inventory_levels(variant_id);
CREATE INDEX IF NOT EXISTS idx_inventory_levels_location ON inventory_levels(location_id);
CREATE INDEX IF NOT EXISTS idx_inventory_levels_low_stock ON inventory_levels(available_quantity, reorder_point) 
    WHERE available_quantity <= reorder_point;

-- Stock reservations for orders
CREATE TABLE IF NOT EXISTS stock_reservations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    variant_id UUID REFERENCES product_variants(id) ON DELETE CASCADE,
    location_id UUID NOT NULL REFERENCES inventory_locations(id) ON DELETE CASCADE,
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'committed', 'released', 'expired')),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_stock_reservations_order ON stock_reservations(order_id);
CREATE INDEX IF NOT EXISTS idx_stock_reservations_product ON stock_reservations(product_id);
CREATE INDEX IF NOT EXISTS idx_stock_reservations_status ON stock_reservations(status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_stock_reservations_expires ON stock_reservations(expires_at) WHERE status = 'active';

-- Stock movement history
CREATE TABLE IF NOT EXISTS stock_movements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    variant_id UUID REFERENCES product_variants(id) ON DELETE CASCADE,
    location_id UUID NOT NULL REFERENCES inventory_locations(id) ON DELETE CASCADE,
    quantity INTEGER NOT NULL,
    movement_type VARCHAR(20) NOT NULL CHECK (movement_type IN ('in', 'out', 'return', 'lost', 'found', 'transfer', 'adjustment')),
    cost_per_unit DECIMAL(10,2),
    reference VARCHAR(255),
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_stock_movements_product ON stock_movements(product_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_location ON stock_movements(location_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_created ON stock_movements(created_at DESC);

-- ============================================================================
-- NOTIFICATION SYSTEM
-- ============================================================================

-- Delivery status enum
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'delivery_status') THEN
        CREATE TYPE delivery_status AS ENUM ('pending', 'sent', 'delivered', 'failed', 'bounced');
    END IF;
END$$;

-- Notification priority enum
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'notification_priority') THEN
        CREATE TYPE notification_priority AS ENUM ('low', 'normal', 'high', 'urgent');
    END IF;
END$$;

-- Notification channel enum
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'notification_channel') THEN
        CREATE TYPE notification_channel AS ENUM ('email', 'sms', 'push', 'webhook');
    END IF;
END$$;

-- Notifications table
CREATE TABLE IF NOT EXISTS notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    channel notification_channel NOT NULL,
    recipient VARCHAR(255) NOT NULL,
    subject VARCHAR(500) NOT NULL,
    body TEXT NOT NULL,
    html_body TEXT,
    priority notification_priority NOT NULL DEFAULT 'normal',
    status delivery_status NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    error_message TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    scheduled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notifications_status ON notifications(status);
CREATE INDEX IF NOT EXISTS idx_notifications_channel ON notifications(channel);
CREATE INDEX IF NOT EXISTS idx_notifications_recipient ON notifications(recipient);
CREATE INDEX IF NOT EXISTS idx_notifications_pending ON notifications(status, scheduled_at, attempt_count, max_attempts) 
    WHERE status = 'pending' AND attempt_count < max_attempts;
CREATE INDEX IF NOT EXISTS idx_notifications_retry ON notifications(status, attempt_count, max_attempts, updated_at) 
    WHERE status = 'failed' AND attempt_count < max_attempts;

-- Notification templates
CREATE TABLE IF NOT EXISTS notification_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) UNIQUE NOT NULL,
    channel notification_channel NOT NULL,
    subject_template VARCHAR(500) NOT NULL,
    body_template TEXT NOT NULL,
    html_template TEXT,
    variables JSONB NOT NULL DEFAULT '[]',
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notification_templates_name ON notification_templates(name);
CREATE INDEX IF NOT EXISTS idx_notification_templates_active ON notification_templates(is_active) WHERE is_active = true;

-- Customer notification preferences
CREATE TABLE IF NOT EXISTS customer_notification_preferences (
    customer_id UUID PRIMARY KEY REFERENCES customers(id) ON DELETE CASCADE,
    email_enabled BOOLEAN NOT NULL DEFAULT true,
    sms_enabled BOOLEAN NOT NULL DEFAULT false,
    push_enabled BOOLEAN NOT NULL DEFAULT false,
    marketing_emails BOOLEAN NOT NULL DEFAULT true,
    order_updates BOOLEAN NOT NULL DEFAULT true,
    shipping_updates BOOLEAN NOT NULL DEFAULT true,
    quiet_hours_start TIME,
    quiet_hours_end TIME,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- FULFILLMENT ITEMS
-- ============================================================================

CREATE TABLE IF NOT EXISTS fulfillment_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fulfillment_id UUID NOT NULL REFERENCES fulfillments(id) ON DELETE CASCADE,
    order_item_id UUID NOT NULL REFERENCES order_items(id) ON DELETE CASCADE,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(fulfillment_id, order_item_id)
);

CREATE INDEX IF NOT EXISTS idx_fulfillment_items_fulfillment ON fulfillment_items(fulfillment_id);
CREATE INDEX IF NOT EXISTS idx_fulfillment_items_order_item ON fulfillment_items(order_item_id);

-- ============================================================================
-- PRODUCT RELATIONS
-- ============================================================================

-- Product-category many-to-many relation
CREATE TABLE IF NOT EXISTS product_category_relations (
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    category_id UUID NOT NULL REFERENCES product_categories(id) ON DELETE CASCADE,
    is_primary BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (product_id, category_id)
);

CREATE INDEX IF NOT EXISTS idx_product_category_category ON product_category_relations(category_id);
CREATE INDEX IF NOT EXISTS idx_product_category_primary ON product_category_relations(product_id, is_primary) WHERE is_primary = true;

-- Product tags
CREATE TABLE IF NOT EXISTS product_tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) UNIQUE NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Product-tag many-to-many relation
CREATE TABLE IF NOT EXISTS product_tag_relations (
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES product_tags(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (product_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_product_tag_tag ON product_tag_relations(tag_id);

-- ============================================================================
-- SHIPPING CARRIER CONFIGURATION
-- ============================================================================

CREATE TABLE IF NOT EXISTS shipping_carrier_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    carrier_code VARCHAR(50) UNIQUE NOT NULL,
    carrier_name VARCHAR(100) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_test_mode BOOLEAN NOT NULL DEFAULT true,
    api_config JSONB NOT NULL DEFAULT '{}',
    supported_services JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_shipping_carrier_code ON shipping_carrier_configs(carrier_code);
CREATE INDEX IF NOT EXISTS idx_shipping_carrier_active ON shipping_carrier_configs(is_active) WHERE is_active = true;

-- Shipping rates cache (for performance)
CREATE TABLE IF NOT EXISTS shipping_rates_cache (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cache_key VARCHAR(255) UNIQUE NOT NULL,
    carrier_code VARCHAR(50) NOT NULL,
    service_code VARCHAR(50) NOT NULL,
    service_name VARCHAR(100) NOT NULL,
    rate_amount DECIMAL(10,2) NOT NULL,
    currency currency NOT NULL DEFAULT 'USD',
    estimated_days INTEGER,
    from_address_hash VARCHAR(64) NOT NULL,
    to_address_hash VARCHAR(64) NOT NULL,
    package_hash VARCHAR(64) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_shipping_rates_cache_key ON shipping_rates_cache(cache_key);
CREATE INDEX IF NOT EXISTS idx_shipping_rates_expires ON shipping_rates_cache(expires_at);

-- ============================================================================
-- SUBSCRIPTION ENHANCEMENTS
-- ============================================================================

-- Subscription payment retry configuration
CREATE TABLE IF NOT EXISTS subscription_retry_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    retry_intervals INTEGER[] NOT NULL DEFAULT ARRAY[1, 3, 7, 14], -- days between retries
    max_retries INTEGER NOT NULL DEFAULT 4,
    is_default BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Dunning campaign templates
CREATE TABLE IF NOT EXISTS dunning_campaigns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    retry_config_id UUID REFERENCES subscription_retry_configs(id),
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Dunning email templates
CREATE TABLE IF NOT EXISTS dunning_email_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    campaign_id UUID NOT NULL REFERENCES dunning_campaigns(id) ON DELETE CASCADE,
    retry_step INTEGER NOT NULL, -- 1, 2, 3, etc.
    days_after_failure INTEGER NOT NULL,
    subject VARCHAR(500) NOT NULL,
    body TEXT NOT NULL,
    html_body TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(campaign_id, retry_step)
);

-- Subscription dunning assignments
CREATE TABLE IF NOT EXISTS subscription_dunning_assignments (
    subscription_id UUID PRIMARY KEY REFERENCES subscriptions(id) ON DELETE CASCADE,
    campaign_id UUID NOT NULL REFERENCES dunning_campaigns(id),
    current_retry_step INTEGER NOT NULL DEFAULT 0,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- WEBHOOK MANAGEMENT
-- ============================================================================

CREATE TABLE IF NOT EXISTS webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    url VARCHAR(500) NOT NULL,
    secret VARCHAR(255) NOT NULL,
    events VARCHAR(100)[] NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_triggered_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhooks_active ON webhooks(is_active) WHERE is_active = true;

-- Webhook delivery log
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL,
    response_status INTEGER,
    response_body TEXT,
    error_message TEXT,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook ON webhook_deliveries(webhook_id);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_created ON webhook_deliveries(created_at DESC);

-- ============================================================================
-- UPDATE TRIGGERS
-- ============================================================================

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create triggers for all tables with updated_at
DO $$
DECLARE
    t text;
    tables text[] := ARRAY[
        'inventory_locations',
        'inventory_levels',
        'stock_reservations',
        'notifications',
        'notification_templates',
        'customer_notification_preferences',
        'product_tags',
        'shipping_carrier_configs',
        'subscription_retry_configs',
        'dunning_campaigns',
        'dunning_email_templates',
        'subscription_dunning_assignments',
        'webhooks'
    ];
BEGIN
    FOREACH t IN ARRAY tables
    LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS update_%s_updated_at ON %s', t, t);
        EXECUTE format('CREATE TRIGGER update_%s_updated_at BEFORE UPDATE ON %s FOR EACH ROW EXECUTE FUNCTION update_updated_at_column()', t, t);
    END LOOP;
END$$;

-- ============================================================================
-- SEED DATA
-- ============================================================================

-- Default inventory location
INSERT INTO inventory_locations (name, code, address)
VALUES ('Default Warehouse', 'DEFAULT', '{"city": "Default", "country": "US"}'::jsonb)
ON CONFLICT (code) DO NOTHING;

-- Default retry configuration
INSERT INTO subscription_retry_configs (name, retry_intervals, max_retries, is_default)
VALUES ('Standard Retry', ARRAY[1, 3, 7, 14], 4, true)
ON CONFLICT DO NOTHING;

-- Default notification templates
INSERT INTO notification_templates (name, channel, subject_template, body_template)
VALUES 
    ('order_confirmation', 'email', 'Order Confirmation: {{order_number}}', 'Thank you for your order {{order_number}}.'),
    ('shipping_notification', 'email', 'Your order has shipped: {{order_number}}', 'Your order {{order_number}} has been shipped.'),
    ('password_reset', 'email', 'Password Reset Request', 'Click here to reset your password: {{reset_link}}')
ON CONFLICT (name) DO NOTHING;

-- Default shipping carrier configs (inactive, need API keys)
INSERT INTO shipping_carrier_configs (carrier_code, carrier_name, is_active, supported_services)
VALUES 
    ('dhl', 'DHL Express', false, '["EXPRESS_WORLDWIDE", "EXPRESS_9:00", "EXPRESS_12:00"]'::jsonb),
    ('fedex', 'FedEx', false, '["GROUND", "EXPRESS_SAVER", "PRIORITY_OVERNIGHT"]'::jsonb),
    ('ups', 'UPS', false, '["GROUND", "STANDARD", "EXPRESS"]'::jsonb),
    ('usps', 'USPS', false, '["FIRST_CLASS", "PRIORITY", "EXPRESS"]'::jsonb)
ON CONFLICT (carrier_code) DO NOTHING;
