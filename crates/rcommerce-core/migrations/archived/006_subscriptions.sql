-- Subscription System Migration
-- Creates tables for subscription management, billing cycles, and dunning

-- Subscription status enum
CREATE TYPE subscription_status AS ENUM (
    'active', 'paused', 'cancelled', 'expired', 'past_due', 'trialing', 'pending'
);

-- Cancellation reason enum
CREATE TYPE cancellation_reason AS ENUM (
    'customer_requested', 'payment_failed', 'fraudulent', 'too_expensive', 'not_useful', 'other'
);

-- Invoice status enum
CREATE TYPE invoice_status AS ENUM (
    'pending', 'billed', 'paid', 'failed', 'past_due', 'cancelled'
);

-- Dunning email type enum
CREATE TYPE dunning_email_type AS ENUM (
    'first_failure', 'retry_failure', 'final_notice', 'cancellation_notice', 'payment_recovered'
);

-- Order type enum (add to existing orders)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'order_type') THEN
        CREATE TYPE order_type AS ENUM ('one_time', 'subscription_initial', 'subscription_renewal');
    END IF;
END $$;

-- Add order_type to orders table if not exists
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'order_type') THEN
        ALTER TABLE orders ADD COLUMN order_type order_type NOT NULL DEFAULT 'one_time';
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'subscription_id') THEN
        ALTER TABLE orders ADD COLUMN subscription_id UUID;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'orders' AND column_name = 'billing_cycle') THEN
        ALTER TABLE orders ADD COLUMN billing_cycle INTEGER;
    END IF;
END $$;

-- Subscriptions table
CREATE TABLE IF NOT EXISTS subscriptions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    order_id UUID NOT NULL REFERENCES orders(id),
    product_id UUID NOT NULL REFERENCES products(id),
    variant_id UUID REFERENCES product_variants(id),
    
    status subscription_status NOT NULL DEFAULT 'pending',
    interval subscription_interval NOT NULL,
    interval_count INTEGER NOT NULL DEFAULT 1,
    
    currency currency NOT NULL,
    amount DECIMAL(20, 2) NOT NULL,
    setup_fee DECIMAL(20, 2),
    
    trial_days INTEGER DEFAULT 0,
    trial_ends_at TIMESTAMPTZ,
    
    current_cycle INTEGER DEFAULT 0,
    min_cycles INTEGER,
    max_cycles INTEGER,
    
    starts_at TIMESTAMPTZ NOT NULL,
    next_billing_at TIMESTAMPTZ NOT NULL,
    last_billing_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    cancellation_reason cancellation_reason,
    
    payment_method_id VARCHAR(255),
    gateway VARCHAR(50) NOT NULL,
    
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Subscription indexes
CREATE INDEX IF NOT EXISTS idx_subscriptions_customer_id ON subscriptions(customer_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_product_id ON subscriptions(product_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_status ON subscriptions(status);
CREATE INDEX IF NOT EXISTS idx_subscriptions_next_billing ON subscriptions(next_billing_at);
CREATE INDEX IF NOT EXISTS idx_subscriptions_status_next_billing ON subscriptions(status, next_billing_at) 
    WHERE status IN ('active', 'trialing');

-- Subscription invoices table
CREATE TABLE IF NOT EXISTS subscription_invoices (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    order_id UUID REFERENCES orders(id),
    
    cycle_number INTEGER NOT NULL,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    
    subtotal DECIMAL(20, 2) NOT NULL,
    tax_total DECIMAL(20, 2) NOT NULL DEFAULT 0,
    total DECIMAL(20, 2) NOT NULL,
    
    status invoice_status NOT NULL DEFAULT 'pending',
    paid_at TIMESTAMPTZ,
    payment_id VARCHAR(255),
    
    failed_attempts INTEGER DEFAULT 0,
    last_failed_at TIMESTAMPTZ,
    failure_reason TEXT,
    
    next_retry_at TIMESTAMPTZ,
    retry_count INTEGER DEFAULT 0,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Invoice indexes
CREATE INDEX IF NOT EXISTS idx_subscription_invoices_subscription_id ON subscription_invoices(subscription_id);
CREATE INDEX IF NOT EXISTS idx_subscription_invoices_status ON subscription_invoices(status);
CREATE INDEX IF NOT EXISTS idx_subscription_invoices_next_retry ON subscription_invoices(next_retry_at) 
    WHERE status IN ('failed', 'past_due');

-- Payment retry attempts table
CREATE TABLE IF NOT EXISTS payment_retry_attempts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES subscription_invoices(id) ON DELETE CASCADE,
    
    attempt_number INTEGER NOT NULL,
    attempted_at TIMESTAMPTZ NOT NULL,
    succeeded BOOLEAN NOT NULL DEFAULT false,
    error_message TEXT,
    error_code VARCHAR(100),
    next_retry_at TIMESTAMPTZ,
    payment_method_id VARCHAR(255),
    gateway_transaction_id VARCHAR(255),
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_payment_retry_attempts_invoice_id ON payment_retry_attempts(invoice_id);
CREATE INDEX IF NOT EXISTS idx_payment_retry_attempts_subscription_id ON payment_retry_attempts(subscription_id);

-- Dunning emails table
CREATE TABLE IF NOT EXISTS dunning_emails (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES subscription_invoices(id) ON DELETE CASCADE,
    
    email_type dunning_email_type NOT NULL,
    subject VARCHAR(500) NOT NULL,
    body_html TEXT NOT NULL,
    body_text TEXT NOT NULL,
    
    sent_at TIMESTAMPTZ NOT NULL,
    opened_at TIMESTAMPTZ,
    clicked_at TIMESTAMPTZ,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_dunning_emails_subscription_id ON dunning_emails(subscription_id);
CREATE INDEX IF NOT EXISTS idx_dunning_emails_invoice_id ON dunning_emails(invoice_id);

-- Trigger to update subscriptions.updated_at
CREATE OR REPLACE FUNCTION update_subscriptions_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS subscriptions_updated_at ON subscriptions;
CREATE TRIGGER subscriptions_updated_at
    BEFORE UPDATE ON subscriptions
    FOR EACH ROW
    EXECUTE FUNCTION update_subscriptions_updated_at();

-- Trigger to update subscription_invoices.updated_at
CREATE OR REPLACE FUNCTION update_subscription_invoices_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS subscription_invoices_updated_at ON subscription_invoices;
CREATE TRIGGER subscription_invoices_updated_at
    BEFORE UPDATE ON subscription_invoices
    FOR EACH ROW
    EXECUTE FUNCTION update_subscription_invoices_updated_at();

-- View for active subscription summary
CREATE OR REPLACE VIEW subscription_summary AS
SELECT 
    status,
    COUNT(*) as count,
    SUM(amount) as total_amount,
    currency
FROM subscriptions
GROUP BY status, currency;

-- View for upcoming billings (next 7 days)
CREATE OR REPLACE VIEW upcoming_billings AS
SELECT 
    s.id as subscription_id,
    s.customer_id,
    s.product_id,
    s.next_billing_at,
    s.amount,
    s.currency,
    s.current_cycle,
    c.email as customer_email,
    p.title as product_title
FROM subscriptions s
JOIN customers c ON s.customer_id = c.id
JOIN products p ON s.product_id = p.id
WHERE s.status IN ('active', 'trialing')
AND s.next_billing_at <= NOW() + INTERVAL '7 days'
ORDER BY s.next_billing_at ASC;
