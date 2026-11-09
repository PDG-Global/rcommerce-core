-- Refresh-token sessions: one row per login. A refresh token names its session and the
-- one token id the session accepts next; each refresh replaces that id, and presenting
-- an older token revokes the session (see services/auth_session.rs).
CREATE TABLE IF NOT EXISTS auth_sessions (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    current_token_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revoked_reason VARCHAR(32)
);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_live_by_customer
    ON auth_sessions(customer_id)
    WHERE revoked_at IS NULL;
