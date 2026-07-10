-- API Keys table for service-to-service authentication
-- Supports both customer API keys and admin/system API keys

-- Create table if not exists
CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    -- Owner of the API key (nullable for system keys)
    customer_id UUID REFERENCES customers(id) ON DELETE CASCADE,
    -- Key identifier (prefix shown to user)
    key_prefix VARCHAR(16) NOT NULL UNIQUE,
    -- SHA256 hash of the full key (only this is stored, never the plaintext key)
    key_hash VARCHAR(64) NOT NULL,
    -- Key name/description for user's reference
    name VARCHAR(100) NOT NULL DEFAULT 'API Key',
    -- Permissions/scopes for this key
    scopes TEXT[] NOT NULL DEFAULT ARRAY['read'],
    -- Optional expiration
    expires_at TIMESTAMPTZ,
    -- Last used timestamp
    last_used_at TIMESTAMPTZ,
    -- IP address that last used this key
    last_used_ip VARCHAR(45),
    -- Rate limit override (null = use default)
    rate_limit_per_minute INTEGER,
    -- Key status
    is_active BOOLEAN NOT NULL DEFAULT true,
    -- Audit timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Soft delete (revoked keys kept for audit)
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT
);

-- Add columns if they don't exist (for idempotent migrations)
DO $$
BEGIN
    -- Add customer_id if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'customer_id') THEN
        ALTER TABLE api_keys ADD COLUMN customer_id UUID REFERENCES customers(id) ON DELETE CASCADE;
    END IF;
    
    -- Add key_prefix if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'key_prefix') THEN
        ALTER TABLE api_keys ADD COLUMN key_prefix VARCHAR(16) NOT NULL DEFAULT 'temp';
        ALTER TABLE api_keys ADD CONSTRAINT api_keys_key_prefix_unique UNIQUE (key_prefix);
    END IF;
    
    -- Add key_hash if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'key_hash') THEN
        ALTER TABLE api_keys ADD COLUMN key_hash VARCHAR(64) NOT NULL DEFAULT '';
    END IF;
    
    -- Add name if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'name') THEN
        ALTER TABLE api_keys ADD COLUMN name VARCHAR(100) NOT NULL DEFAULT 'API Key';
    END IF;
    
    -- Add scopes if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'scopes') THEN
        ALTER TABLE api_keys ADD COLUMN scopes TEXT[] NOT NULL DEFAULT ARRAY['read'];
    END IF;
    
    -- Add expires_at if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'expires_at') THEN
        ALTER TABLE api_keys ADD COLUMN expires_at TIMESTAMPTZ;
    END IF;
    
    -- Add last_used_at if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'last_used_at') THEN
        ALTER TABLE api_keys ADD COLUMN last_used_at TIMESTAMPTZ;
    END IF;
    
    -- Add last_used_ip if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'last_used_ip') THEN
        ALTER TABLE api_keys ADD COLUMN last_used_ip VARCHAR(45);
    END IF;
    
    -- Add rate_limit_per_minute if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'rate_limit_per_minute') THEN
        ALTER TABLE api_keys ADD COLUMN rate_limit_per_minute INTEGER;
    END IF;
    
    -- Add is_active if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'is_active') THEN
        ALTER TABLE api_keys ADD COLUMN is_active BOOLEAN NOT NULL DEFAULT true;
    END IF;
    
    -- Add created_at if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'created_at') THEN
        ALTER TABLE api_keys ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
    END IF;
    
    -- Add updated_at if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'updated_at') THEN
        ALTER TABLE api_keys ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
    END IF;
    
    -- Add revoked_at if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'revoked_at') THEN
        ALTER TABLE api_keys ADD COLUMN revoked_at TIMESTAMPTZ;
    END IF;
    
    -- Add revoked_reason if missing
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'api_keys' AND column_name = 'revoked_reason') THEN
        ALTER TABLE api_keys ADD COLUMN revoked_reason TEXT;
    END IF;
END $$;

-- Indexes for common queries (idempotent)
CREATE INDEX IF NOT EXISTS idx_api_keys_customer_id ON api_keys(customer_id) WHERE customer_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_api_keys_key_prefix ON api_keys(key_prefix);
CREATE INDEX IF NOT EXISTS idx_api_keys_is_active ON api_keys(is_active) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_api_keys_expires_at ON api_keys(expires_at) WHERE expires_at IS NOT NULL;

-- Trigger to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_api_keys_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS api_keys_updated_at ON api_keys;
CREATE TRIGGER api_keys_updated_at
    BEFORE UPDATE ON api_keys
    FOR EACH ROW
    EXECUTE FUNCTION update_api_keys_updated_at();
