-- Gateway-specific customer mappings and local ownership records for saved payment methods.
-- Raw provider tokens are never stored; only SHA-256 hashes are retained for ownership checks.

CREATE TABLE IF NOT EXISTS payment_gateway_customers (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    gateway VARCHAR(50) NOT NULL,
    gateway_customer_id VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (customer_id, gateway),
    UNIQUE (gateway, gateway_customer_id)
);

CREATE TABLE IF NOT EXISTS saved_payment_methods (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    gateway VARCHAR(50) NOT NULL,
    token_hash CHAR(64) NOT NULL,
    method_type VARCHAR(50) NOT NULL,
    card_last_four VARCHAR(4),
    card_brand VARCHAR(50),
    exp_month VARCHAR(2),
    exp_year VARCHAR(4),
    cardholder_name VARCHAR(255),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (token_hash)
);

CREATE INDEX IF NOT EXISTS idx_saved_payment_methods_customer_gateway
    ON saved_payment_methods(customer_id, gateway);

CREATE TABLE IF NOT EXISTS payment_initiation_attempts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL UNIQUE REFERENCES orders(id) ON DELETE CASCADE,
    gateway VARCHAR(50) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS payment_identifier_conflicts (
    payment_id UUID PRIMARY KEY REFERENCES payments(id) ON DELETE CASCADE,
    gateway VARCHAR(50) NOT NULL,
    gateway_payment_id VARCHAR(255) NOT NULL,
    quarantined_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

DO $$
DECLARE
    duplicate_count BIGINT;
BEGIN
    SELECT COUNT(*) INTO duplicate_count
    FROM (
        SELECT airwallex_customer_id
        FROM customers
        WHERE airwallex_customer_id IS NOT NULL
          AND BTRIM(airwallex_customer_id) <> ''
        GROUP BY airwallex_customer_id
        HAVING COUNT(*) > 1
    ) duplicates;

    IF duplicate_count > 0 THEN
        RAISE WARNING 'Skipped % duplicate legacy Airwallex customer IDs; map these customers explicitly', duplicate_count;
    END IF;
END $$;

INSERT INTO payment_gateway_customers (customer_id, gateway, gateway_customer_id)
SELECT customer.id, 'airwallex', customer.airwallex_customer_id
FROM customers customer
WHERE customer.airwallex_customer_id IS NOT NULL
  AND BTRIM(customer.airwallex_customer_id) <> ''
  AND NOT EXISTS (
      SELECT 1 FROM customers duplicate
      WHERE duplicate.airwallex_customer_id = customer.airwallex_customer_id
        AND duplicate.id <> customer.id
  )
  AND NOT EXISTS (
      SELECT 1 FROM payment_gateway_customers existing
      WHERE existing.gateway = 'airwallex'
        AND existing.gateway_customer_id = customer.airwallex_customer_id
        AND existing.customer_id <> customer.id
  )
ON CONFLICT DO NOTHING;

DO $$
DECLARE
    conflict_count BIGINT;
BEGIN
    CREATE TEMP TABLE IF NOT EXISTS payment_identifier_conflicts_to_quarantine (
        payment_id UUID PRIMARY KEY,
        gateway VARCHAR(50) NOT NULL,
        gateway_payment_id VARCHAR(255) NOT NULL
    ) ON COMMIT DROP;
    TRUNCATE payment_identifier_conflicts_to_quarantine;

    INSERT INTO payment_identifier_conflicts_to_quarantine (payment_id, gateway, gateway_payment_id)
    SELECT payment.id, payment.gateway, payment.gateway_payment_id
    FROM payments payment
    JOIN (
        SELECT gateway, gateway_payment_id
        FROM payments
        WHERE gateway_payment_id IS NOT NULL
        GROUP BY gateway, gateway_payment_id
        HAVING COUNT(*) > 1
    ) duplicate
      ON duplicate.gateway = payment.gateway
     AND duplicate.gateway_payment_id = payment.gateway_payment_id;

    SELECT COUNT(*) INTO conflict_count FROM payment_identifier_conflicts_to_quarantine;
    INSERT INTO payment_identifier_conflicts (payment_id, gateway, gateway_payment_id)
    SELECT payment_id, gateway, gateway_payment_id
    FROM payment_identifier_conflicts_to_quarantine
    ON CONFLICT (payment_id) DO NOTHING;

    UPDATE payments payment
    SET gateway_payment_id = NULL, updated_at = NOW()
    FROM payment_identifier_conflicts_to_quarantine conflict
    WHERE payment.id = conflict.payment_id;

    IF conflict_count > 0 THEN
        RAISE WARNING 'Quarantined % ambiguous gateway payment identifiers in payment_identifier_conflicts', conflict_count;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_payments_gateway_payment_id_unique
    ON payments(gateway, gateway_payment_id)
    WHERE gateway_payment_id IS NOT NULL;
