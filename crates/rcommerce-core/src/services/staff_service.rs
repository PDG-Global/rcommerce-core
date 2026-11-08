//! Staff roles and their assignment to accounts
//!
//! A staff role lists `<resource>:read` and `<resource>:write` permissions
//! (see [`crate::services::api_key_scopes`]). An account holding roles gets
//! the union of their permissions on the admin API; global admins do not need
//! roles. Roles never grant `admin` or wildcard scopes, so they cannot reach
//! admin-only routes or resources added later.

use uuid::Uuid;
use validator::Validate;

use crate::{
    models::{CreateStaffRoleRequest, StaffMember, StaffRole, UpdateStaffRoleRequest},
    repository::Database,
    services::api_key_scopes::{Action, Scope},
    Error, Result,
};

/// Staff role management and permission lookup
pub struct StaffService {
    db: Database,
}

impl StaffService {
    /// Create a new staff service
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// All staff roles, by name
    pub async fn list_roles(&self) -> Result<Vec<StaffRole>> {
        let roles = sqlx::query_as::<_, StaffRole>("SELECT * FROM staff_roles ORDER BY name")
            .fetch_all(self.db.pool())
            .await?;
        Ok(roles)
    }

    /// Create a role; its name must be unique
    pub async fn create_role(&self, request: CreateStaffRoleRequest) -> Result<StaffRole> {
        request
            .validate()
            .map_err(|e| Error::validation(format!("Invalid staff role: {}", e)))?;
        let permissions = validate_permissions(&request.permissions)?;

        sqlx::query_as::<_, StaffRole>(
            "INSERT INTO staff_roles (name, description, permissions) \
             VALUES ($1, $2, $3) RETURNING *",
        )
        .bind(&request.name)
        .bind(&request.description)
        .bind(&permissions)
        .fetch_one(self.db.pool())
        .await
        .map_err(duplicate_name)
    }

    /// Update a role; omitted fields keep their value
    pub async fn update_role(
        &self,
        id: Uuid,
        request: UpdateStaffRoleRequest,
    ) -> Result<StaffRole> {
        request
            .validate()
            .map_err(|e| Error::validation(format!("Invalid staff role: {}", e)))?;
        let permissions = match &request.permissions {
            Some(permissions) => Some(validate_permissions(permissions)?),
            None => None,
        };

        sqlx::query_as::<_, StaffRole>(
            "UPDATE staff_roles SET name = COALESCE($2, name), \
             description = COALESCE($3, description), \
             permissions = COALESCE($4, permissions), updated_at = NOW() \
             WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&permissions)
        .fetch_optional(self.db.pool())
        .await
        .map_err(duplicate_name)?
        .ok_or_else(|| Error::not_found("Staff role not found"))
    }

    /// Delete a role and its assignments
    pub async fn delete_role(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM staff_roles WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::not_found("Staff role not found"));
        }
        Ok(())
    }

    /// Accounts holding at least one role, by e-mail
    pub async fn list_members(&self) -> Result<Vec<StaffMember>> {
        let members = sqlx::query_as::<_, StaffMember>(
            "SELECT c.id AS customer_id, c.email, \
             array_agg(a.role_id ORDER BY a.role_id) AS role_ids \
             FROM staff_role_assignments a JOIN customers c ON c.id = a.customer_id \
             GROUP BY c.id, c.email ORDER BY c.email",
        )
        .fetch_all(self.db.pool())
        .await?;
        Ok(members)
    }

    /// Replace an account's roles (an empty list removes its staff access)
    pub async fn set_member_roles(
        &self,
        customer_id: Uuid,
        role_ids: &[Uuid],
    ) -> Result<Vec<Uuid>> {
        let mut role_ids = role_ids.to_vec();
        role_ids.sort();
        role_ids.dedup();

        let mut tx = self.db.pool().begin().await?;
        let customer: Option<Uuid> = sqlx::query_scalar("SELECT id FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_optional(&mut *tx)
            .await?;
        customer.ok_or_else(|| Error::not_found("Customer not found"))?;

        let known: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM staff_roles WHERE id = ANY($1)")
            .bind(&role_ids)
            .fetch_one(&mut *tx)
            .await?;
        if known != role_ids.len() as i64 {
            return Err(Error::not_found("Staff role not found"));
        }

        sqlx::query("DELETE FROM staff_role_assignments WHERE customer_id = $1")
            .bind(customer_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO staff_role_assignments (customer_id, role_id) \
             SELECT $1, unnest($2::uuid[])",
        )
        .bind(customer_id)
        .bind(&role_ids)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(role_ids)
    }

    /// The union of the permissions of an account's roles (empty without roles)
    pub async fn permissions_for(&self, customer_id: Uuid) -> Result<Vec<String>> {
        let permissions = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT p FROM staff_role_assignments a \
             JOIN staff_roles r ON r.id = a.role_id, unnest(r.permissions) AS p \
             WHERE a.customer_id = $1 ORDER BY p",
        )
        .bind(customer_id)
        .fetch_all(self.db.pool())
        .await?;
        Ok(permissions)
    }
}

/// Check and normalise a role's permissions: each must be `<resource>:read` or
/// `<resource>:write`. Wildcards (`read`, `write`, `admin`) and `<resource>:admin`
/// are refused, so a role can never grant admin rights.
pub fn validate_permissions(permissions: &[String]) -> Result<Vec<String>> {
    let mut normalized: Vec<String> = Vec::new();
    for permission in permissions {
        let scope = Scope::parse(permission).map_err(|e| {
            Error::validation(format!("Invalid permission '{}': {}", permission, e))
        })?;
        if scope.resource.is_none() || scope.action == Action::Admin {
            return Err(Error::validation(format!(
                "Invalid permission '{}': use <resource>:read or <resource>:write",
                permission
            )));
        }
        let scope = scope.as_scope_string();
        if !normalized.contains(&scope) {
            normalized.push(scope);
        }
    }
    Ok(normalized)
}

/// A unique violation on the role name is a client error
fn duplicate_name(error: sqlx::Error) -> Error {
    match &error {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            Error::validation("A staff role with this name already exists")
        }
        _ => error.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_permissions;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn resource_read_and_write_are_accepted_once() {
        let permissions = strings(&["products:write", "reports:read", "products:write"]);
        assert_eq!(
            validate_permissions(&permissions).unwrap(),
            strings(&["products:write", "reports:read"])
        );
        assert!(validate_permissions(&[]).unwrap().is_empty());
    }

    #[test]
    fn admin_wildcard_and_unknown_permissions_are_refused() {
        for bad in [
            "admin",
            "read",
            "write",
            "products:admin",
            "nope:read",
            "products:delete",
            "",
        ] {
            let error = validate_permissions(&strings(&[bad])).unwrap_err();
            assert_eq!(error.status_code(), 400, "{bad}");
        }
    }
}
