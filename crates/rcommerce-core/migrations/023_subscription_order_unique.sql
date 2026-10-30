DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM subscriptions
        GROUP BY order_id
        HAVING COUNT(*) > 1
        LIMIT 1
    ) THEN
        RAISE EXCEPTION 'Duplicate subscriptions.order_id values detected; reconcile them before migration 023';
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_subscriptions_order_id_unique
    ON subscriptions(order_id);
