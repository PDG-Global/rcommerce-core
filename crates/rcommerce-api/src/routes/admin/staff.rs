//! Staff role management
//!
//! Only global admins reach these routes: `/admin/staff` is not a staff area
//! (see [`crate::middleware::staff_access`]), so a staff member cannot widen
//! its own access.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;
use rcommerce_core::{
    models::{CreateStaffRoleRequest, UpdateStaffRoleRequest},
    Error, StaffService,
};

/// JSON body on success, the error's status and message otherwise
type JsonResult = Result<Json<serde_json::Value>, Error>;

/// The roles an account should hold
#[derive(Debug, Deserialize)]
pub struct SetMemberRolesBody {
    pub role_ids: Vec<Uuid>,
}

/// List staff roles
///
/// GET /api/v1/admin/staff/roles
pub async fn list_roles(State(state): State<AppState>) -> JsonResult {
    let roles = StaffService::new(state.db.clone()).list_roles().await?;
    Ok(Json(serde_json::json!({ "roles": roles })))
}

/// Create a staff role
///
/// POST /api/v1/admin/staff/roles
pub async fn create_role(
    State(state): State<AppState>,
    Json(body): Json<CreateStaffRoleRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), Error> {
    let role = StaffService::new(state.db.clone())
        .create_role(body)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "role": role })),
    ))
}

/// Update a staff role; the change applies to its holders' next request
///
/// PUT /api/v1/admin/staff/roles/:id
pub async fn update_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateStaffRoleRequest>,
) -> JsonResult {
    let role = StaffService::new(state.db.clone())
        .update_role(id, body)
        .await?;
    Ok(Json(serde_json::json!({ "role": role })))
}

/// Delete a staff role and its assignments
///
/// DELETE /api/v1/admin/staff/roles/:id
pub async fn delete_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Error> {
    StaffService::new(state.db.clone()).delete_role(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// List the accounts holding staff roles
///
/// GET /api/v1/admin/staff/members
pub async fn list_members(State(state): State<AppState>) -> JsonResult {
    let members = StaffService::new(state.db.clone()).list_members().await?;
    Ok(Json(serde_json::json!({ "members": members })))
}

/// Replace an account's staff roles (an empty list removes its staff access)
///
/// PUT /api/v1/admin/staff/members/:customer_id/roles
pub async fn set_member_roles(
    State(state): State<AppState>,
    Path(customer_id): Path<Uuid>,
    Json(body): Json<SetMemberRolesBody>,
) -> JsonResult {
    let role_ids = StaffService::new(state.db.clone())
        .set_member_roles(customer_id, &body.role_ids)
        .await?;
    Ok(Json(serde_json::json!({
        "customer_id": customer_id,
        "role_ids": role_ids
    })))
}

/// Router for staff role management
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/staff/roles", get(list_roles).post(create_role))
        .route(
            "/admin/staff/roles/:id",
            put(update_role).delete(delete_role),
        )
        .route("/admin/staff/members", get(list_members))
        .route(
            "/admin/staff/members/:customer_id/roles",
            put(set_member_roles),
        )
}
