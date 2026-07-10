-- Statistics Views Migration
-- Creates materialized views and indexes for efficient statistics queries

-- ====================
-- MATERIALIZED VIEWS
-- ====================

-- Daily sales summary materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_daily_sales_summary AS
SELECT 
    DATE_TRUNC('day', created_at) as sales_date,
    COUNT(*) as order_count,
    COALESCE(SUM(total), 0) as total_revenue,
    COALESCE(SUM(subtotal), 0) as subtotal_revenue,
    COALESCE(SUM(tax_total), 0) as tax_total,
    COALESCE(SUM(shipping_total), 0) as shipping_total,
    COALESCE(SUM(discount_total), 0) as discount_total,
    COALESCE(AVG(total), 0) as average_order_value,
    COUNT(DISTINCT customer_id) as unique_customers
FROM orders
WHERE status NOT IN ('cancelled', 'refunded')
GROUP BY DATE_TRUNC('day', created_at)
ORDER BY sales_date DESC;

-- Create unique index on daily sales summary
CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_daily_sales_date ON mv_daily_sales_summary(sales_date);
CREATE INDEX IF NOT EXISTS idx_mv_daily_sales_revenue ON mv_daily_sales_summary(total_revenue);

-- Monthly sales summary materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_monthly_sales_summary AS
SELECT 
    DATE_TRUNC('month', created_at) as sales_month,
    COUNT(*) as order_count,
    COALESCE(SUM(total), 0) as total_revenue,
    COALESCE(AVG(total), 0) as average_order_value,
    COUNT(DISTINCT customer_id) as unique_customers
FROM orders
WHERE status NOT IN ('cancelled', 'refunded')
GROUP BY DATE_TRUNC('month', created_at)
ORDER BY sales_month DESC;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_monthly_sales_month ON mv_monthly_sales_summary(sales_month);

-- Product performance materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_product_performance AS
SELECT 
    p.id as product_id,
    p.title as product_name,
    p.sku,
    COALESCE(SUM(oi.quantity), 0) as total_units_sold,
    COALESCE(SUM(oi.total), 0) as total_revenue,
    COUNT(DISTINCT oi.order_id) as orders_count,
    MAX(o.created_at) as last_order_date
FROM products p
LEFT JOIN order_items oi ON p.id = oi.product_id
LEFT JOIN orders o ON oi.order_id = o.id AND o.status NOT IN ('cancelled', 'refunded')
WHERE p.is_active = true
GROUP BY p.id, p.title, p.sku
HAVING COALESCE(SUM(oi.quantity), 0) > 0;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_product_perf_id ON mv_product_performance(product_id);
CREATE INDEX IF NOT EXISTS idx_mv_product_perf_revenue ON mv_product_performance(total_revenue DESC);

-- Customer statistics materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_customer_statistics AS
SELECT 
    c.id as customer_id,
    c.email,
    c.created_at as customer_since,
    COUNT(o.id) as total_orders,
    COALESCE(SUM(o.total), 0) as total_spent,
    COALESCE(AVG(o.total), 0) as average_order_value,
    MAX(o.created_at) as last_order_date,
    MIN(o.created_at) as first_order_date
FROM customers c
LEFT JOIN orders o ON c.id = o.customer_id AND o.status NOT IN ('cancelled', 'refunded')
GROUP BY c.id, c.email, c.created_at;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_customer_stats_id ON mv_customer_statistics(customer_id);
CREATE INDEX IF NOT EXISTS idx_mv_customer_stats_spent ON mv_customer_statistics(total_spent DESC);

-- Order status breakdown materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_order_status_breakdown AS
SELECT 
    DATE_TRUNC('day', created_at) as date,
    status::text as status,
    payment_status::text as payment_status,
    fulfillment_status::text as fulfillment_status,
    COUNT(*) as count,
    COALESCE(SUM(total), 0) as revenue
FROM orders
GROUP BY DATE_TRUNC('day', created_at), status, payment_status, fulfillment_status;

CREATE INDEX IF NOT EXISTS idx_mv_status_breakdown_date ON mv_order_status_breakdown(date);

-- ====================
-- HELPER FUNCTIONS
-- ====================

-- Function to refresh all statistics materialized views
CREATE OR REPLACE FUNCTION refresh_statistics_views()
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_daily_sales_summary;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_monthly_sales_summary;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_product_performance;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_customer_statistics;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_order_status_breakdown;
END;
$$;

-- Function to get sales summary for date range
CREATE OR REPLACE FUNCTION get_sales_summary(
    p_from TIMESTAMP WITH TIME ZONE,
    p_to TIMESTAMP WITH TIME ZONE,
    p_period TEXT DEFAULT 'day'
)
RETURNS TABLE (
    period_start TIMESTAMP WITH TIME ZONE,
    period_end TIMESTAMP WITH TIME ZONE,
    total_revenue DECIMAL,
    total_orders BIGINT,
    total_items_sold BIGINT,
    average_order_value DECIMAL
)
LANGUAGE plpgsql
AS $$
BEGIN
    RETURN QUERY
    SELECT 
        DATE_TRUNC(p_period, o.created_at) as period_start,
        DATE_TRUNC(p_period, o.created_at) + 
            CASE p_period
                WHEN 'day' THEN INTERVAL '1 day'
                WHEN 'week' THEN INTERVAL '1 week'
                WHEN 'month' THEN INTERVAL '1 month'
                WHEN 'year' THEN INTERVAL '1 year'
            END as period_end,
        COALESCE(SUM(o.total), 0) as total_revenue,
        COUNT(*) as total_orders,
        COALESCE(SUM((SELECT SUM(quantity) FROM order_items WHERE order_id = o.id)), 0) as total_items_sold,
        COALESCE(AVG(o.total), 0) as average_order_value
    FROM orders o
    WHERE o.created_at >= p_from 
    AND o.created_at <= p_to
    AND o.status NOT IN ('cancelled', 'refunded')
    GROUP BY DATE_TRUNC(p_period, o.created_at)
    ORDER BY period_start;
END;
$$;

-- Function to get top products
CREATE OR REPLACE FUNCTION get_top_products(
    p_limit INTEGER DEFAULT 10,
    p_from TIMESTAMP WITH TIME ZONE DEFAULT NULL,
    p_to TIMESTAMP WITH TIME ZONE DEFAULT NULL
)
RETURNS TABLE (
    product_id UUID,
    product_name TEXT,
    sku TEXT,
    units_sold BIGINT,
    revenue DECIMAL,
    orders_count BIGINT
)
LANGUAGE plpgsql
AS $$
BEGIN
    RETURN QUERY
    SELECT 
        p.id,
        p.title,
        p.sku,
        COALESCE(SUM(oi.quantity), 0)::BIGINT as units_sold,
        COALESCE(SUM(oi.total), 0) as revenue,
        COUNT(DISTINCT oi.order_id)::BIGINT as orders_count
    FROM products p
    LEFT JOIN order_items oi ON p.id = oi.product_id
    LEFT JOIN orders o ON oi.order_id = o.id
    WHERE (p_from IS NULL OR o.created_at >= p_from)
    AND (p_to IS NULL OR o.created_at <= p_to)
    AND (o.status IS NULL OR o.status NOT IN ('cancelled', 'refunded'))
    GROUP BY p.id, p.title, p.sku
    HAVING COALESCE(SUM(oi.quantity), 0) > 0
    ORDER BY revenue DESC
    LIMIT p_limit;
END;
$$;

-- Function to get dashboard metrics
CREATE OR REPLACE FUNCTION get_dashboard_metrics()
RETURNS TABLE (
    total_revenue DECIMAL,
    total_orders BIGINT,
    total_customers BIGINT,
    total_products BIGINT,
    pending_orders BIGINT,
    revenue_today DECIMAL,
    orders_today BIGINT,
    revenue_this_month DECIMAL,
    orders_this_month BIGINT
)
LANGUAGE plpgsql
AS $$
BEGIN
    RETURN QUERY
    SELECT 
        -- Total metrics
        (SELECT COALESCE(SUM(total), 0) FROM orders WHERE status NOT IN ('cancelled', 'refunded')),
        (SELECT COUNT(*) FROM orders WHERE status NOT IN ('cancelled', 'refunded')),
        (SELECT COUNT(*) FROM customers),
        (SELECT COUNT(*) FROM products WHERE is_active = true),
        (SELECT COUNT(*) FROM orders WHERE status IN ('pending', 'processing')),
        -- Today's metrics
        (SELECT COALESCE(SUM(total), 0) FROM orders 
         WHERE created_at >= DATE_TRUNC('day', NOW()) 
         AND status NOT IN ('cancelled', 'refunded')),
        (SELECT COUNT(*) FROM orders 
         WHERE created_at >= DATE_TRUNC('day', NOW()) 
         AND status NOT IN ('cancelled', 'refunded')),
        -- This month's metrics
        (SELECT COALESCE(SUM(total), 0) FROM orders 
         WHERE created_at >= DATE_TRUNC('month', NOW()) 
         AND status NOT IN ('cancelled', 'refunded')),
        (SELECT COUNT(*) FROM orders 
         WHERE created_at >= DATE_TRUNC('month', NOW()) 
         AND status NOT IN ('cancelled', 'refunded'));
END;
$$;

-- Function to compare periods
CREATE OR REPLACE FUNCTION compare_periods(
    p_current_from TIMESTAMP WITH TIME ZONE,
    p_current_to TIMESTAMP WITH TIME ZONE
)
RETURNS TABLE (
    metric_name TEXT,
    current_value DECIMAL,
    previous_value DECIMAL,
    change_amount DECIMAL,
    change_percentage DECIMAL
)
LANGUAGE plpgsql
AS $$
DECLARE
    v_duration INTERVAL := p_current_to - p_current_from;
    v_previous_from TIMESTAMP WITH TIME ZONE := p_current_from - v_duration;
    v_previous_to TIMESTAMP WITH TIME ZONE := p_current_from;
BEGIN
    -- Revenue comparison
    RETURN QUERY
    SELECT 
        'revenue'::TEXT,
        COALESCE((SELECT SUM(total) FROM orders 
                  WHERE created_at >= p_current_from 
                  AND created_at <= p_current_to 
                  AND status NOT IN ('cancelled', 'refunded')), 0),
        COALESCE((SELECT SUM(total) FROM orders 
                  WHERE created_at >= v_previous_from 
                  AND created_at <= v_previous_to 
                  AND status NOT IN ('cancelled', 'refunded')), 0),
        0::DECIMAL,
        0::DECIMAL;
    
    -- Orders comparison
    RETURN QUERY
    SELECT 
        'orders'::TEXT,
        (SELECT COUNT(*)::DECIMAL FROM orders 
         WHERE created_at >= p_current_from 
         AND created_at <= p_current_to 
         AND status NOT IN ('cancelled', 'refunded')),
        (SELECT COUNT(*)::DECIMAL FROM orders 
         WHERE created_at >= v_previous_from 
         AND created_at <= v_previous_to 
         AND status NOT IN ('cancelled', 'refunded')),
        0::DECIMAL,
        0::DECIMAL;
    
    -- Customers comparison
    RETURN QUERY
    SELECT 
        'customers'::TEXT,
        (SELECT COUNT(DISTINCT customer_id)::DECIMAL FROM orders 
         WHERE created_at >= p_current_from 
         AND created_at <= p_current_to 
         AND status NOT IN ('cancelled', 'refunded')),
        (SELECT COUNT(DISTINCT customer_id)::DECIMAL FROM orders 
         WHERE created_at >= v_previous_from 
         AND created_at <= v_previous_to 
         AND status NOT IN ('cancelled', 'refunded')),
        0::DECIMAL,
        0::DECIMAL;
    
    -- AOV comparison
    RETURN QUERY
    SELECT 
        'average_order_value'::TEXT,
        COALESCE((SELECT AVG(total) FROM orders 
                  WHERE created_at >= p_current_from 
                  AND created_at <= p_current_to 
                  AND status NOT IN ('cancelled', 'refunded')), 0),
        COALESCE((SELECT AVG(total) FROM orders 
                  WHERE created_at >= v_previous_from 
                  AND created_at <= v_previous_to 
                  AND status NOT IN ('cancelled', 'refunded')), 0),
        0::DECIMAL,
        0::DECIMAL;
END;
$$;

-- ====================
-- INDEXES FOR PERFORMANCE
-- ====================

-- Additional indexes for statistics queries
CREATE INDEX IF NOT EXISTS idx_orders_created_at_status ON orders(created_at, status) 
    WHERE status NOT IN ('cancelled', 'refunded');

CREATE INDEX IF NOT EXISTS idx_orders_customer_status ON orders(customer_id, status) 
    WHERE status NOT IN ('cancelled', 'refunded');

CREATE INDEX IF NOT EXISTS idx_order_items_product_order ON order_items(product_id, order_id);

CREATE INDEX IF NOT EXISTS idx_customers_created_at ON customers(created_at);

-- Partial index for active products
CREATE INDEX IF NOT EXISTS idx_products_active ON products(id) WHERE is_active = true;

-- Comment on migration
COMMENT ON MATERIALIZED VIEW mv_daily_sales_summary IS 'Daily aggregated sales data for fast statistics queries';
COMMENT ON MATERIALIZED VIEW mv_monthly_sales_summary IS 'Monthly aggregated sales data for fast statistics queries';
COMMENT ON MATERIALIZED VIEW mv_product_performance IS 'Product performance metrics updated periodically';
COMMENT ON MATERIALIZED VIEW mv_customer_statistics IS 'Customer lifetime value and order statistics';
