-- Migration 008: Product reviews and newsletter subscribers
--
-- Creates tables for Phase 2 storefront feature parity with a migrated
-- WooCommerce store:
--   1. product_reviews        - customer reviews on products; imported
--                               WooCommerce reviews land here too.
--   2. newsletter_subscribers - email list for the storefront newsletter
--                               signup form.
--
-- All statements are idempotent (IF NOT EXISTS) so re-running is safe.

-- ============================================================================
-- PRODUCT REVIEWS
-- ============================================================================

CREATE TABLE IF NOT EXISTS product_reviews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    -- Nullable: imported reviews may not map to a local customer account.
    customer_id UUID REFERENCES customers(id) ON DELETE SET NULL,
    -- Denormalized display name (first + last name at submission time).
    author_name VARCHAR(255) NOT NULL,
    rating INT NOT NULL CHECK (rating >= 1 AND rating <= 5),
    title VARCHAR(500),
    content TEXT NOT NULL,
    -- 'pending' | 'approved' | 'rejected'. New storefront submissions default
    -- to 'approved' to match the current live site; moderation comes later.
    status VARCHAR(20) NOT NULL DEFAULT 'approved',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_product_reviews_product_status
    ON product_reviews (product_id, status);

-- ============================================================================
-- NEWSLETTER SUBSCRIBERS
-- ============================================================================

CREATE TABLE IF NOT EXISTS newsletter_subscribers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL,
    -- 'subscribed' | 'unsubscribed'
    status VARCHAR(20) NOT NULL DEFAULT 'subscribed',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Case-insensitive uniqueness on email (CITEXT is not assumed available).
CREATE UNIQUE INDEX IF NOT EXISTS idx_newsletter_subscribers_email_lower
    ON newsletter_subscribers (lower(email));
