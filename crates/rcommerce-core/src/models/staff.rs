//! Staff roles: named permission sets that open parts of the admin API to
//! accounts that are not global admins

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

/// A named set of `<resource>:read` / `<resource>:write` permissions
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StaffRole {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create a staff role
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateStaffRoleRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Update a staff role; omitted fields keep their value
#[derive(Debug, Clone, Default, Deserialize, Validate)]
pub struct UpdateStaffRoleRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
}

/// An account holding at least one staff role
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct StaffMember {
    pub customer_id: Uuid,
    pub email: String,
    pub role_ids: Vec<Uuid>,
}
