-- Staff roles: named sets of `<resource>:read` / `<resource>:write` permissions that
-- open parts of the admin API to accounts that are not global admins.
-- Global admins (customers.role = 'admin') keep full access, and an account without
-- an assigned role keeps no admin access, so existing installations are unchanged.

CREATE TABLE IF NOT EXISTS staff_roles (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    permissions TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS staff_role_assignments (
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES staff_roles(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (customer_id, role_id)
);

CREATE INDEX IF NOT EXISTS idx_staff_role_assignments_role
    ON staff_role_assignments (role_id);
