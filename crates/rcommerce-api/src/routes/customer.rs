use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Extension, Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::middleware::JwtAuth;
use crate::state::AppState;
use rcommerce_core::{
    models::{CreateAddressRequest, Customer, UpdateCustomerRequest},
    services::{customer_service::CustomerDetail, PaginationParams},
    Error,
};

/// List customers (admin only)
pub async fn list_customers(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
) -> Result<Json<serde_json::Value>, Error> {
    // Check admin permission
    if !auth.is_admin() {
        return Err(Error::unauthorized("Admin access required"));
    }

    let customer_list = state
        .customer_service
        .list_customers(PaginationParams::default())
        .await?;

    let customers: Vec<serde_json::Value> = customer_list
        .customers
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "email": c.email,
                "first_name": c.first_name,
                "last_name": c.last_name,
                "phone": c.phone,
                "accepts_marketing": c.accepts_marketing,
                "tax_exempt": c.tax_exempt,
                "currency": c.currency.to_string(),
                "created_at": c.created_at,
                "updated_at": c.updated_at,
                "confirmed_at": c.confirmed_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "customers": customers,
        "meta": {
            "total": customer_list.pagination.total,
            "page": customer_list.pagination.page,
            "per_page": customer_list.pagination.per_page,
            "total_pages": customer_list.pagination.total_pages,
        }
    })))
}

/// Profile fields of a customer, as returned by the customer endpoints
fn customer_json(c: &Customer) -> serde_json::Value {
    serde_json::json!({
        "id": c.id,
        "email": c.email,
        "first_name": c.first_name,
        "last_name": c.last_name,
        "phone": c.phone,
        "accepts_marketing": c.accepts_marketing,
        "tax_exempt": c.tax_exempt,
        "currency": c.currency.to_string(),
        "created_at": c.created_at,
        "updated_at": c.updated_at,
        "confirmed_at": c.confirmed_at,
    })
}

/// A customer with their addresses
fn detail_json(detail: CustomerDetail) -> serde_json::Value {
    let addresses: Vec<serde_json::Value> = detail
        .addresses
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "first_name": a.first_name,
                "last_name": a.last_name,
                "company": a.company,
                "phone": a.phone,
                "address1": a.address1,
                "address2": a.address2,
                "city": a.city,
                "state": a.state,
                "country": a.country,
                "zip": a.zip,
                "is_default_shipping": a.is_default_shipping,
                "is_default_billing": a.is_default_billing,
                "created_at": a.created_at,
                "updated_at": a.updated_at,
            })
        })
        .collect();
    serde_json::json!({ "customer": customer_json(&detail.customer), "addresses": addresses })
}

/// Fields a customer may change on their own profile. E-mail (needs verification) and
/// tax exemption (merchant decision) are not among them; unknown fields are refused.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 1, max = 100))]
    pub first_name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub last_name: Option<String>,
    /// An empty string removes the phone number
    #[validate(length(max = 50))]
    pub phone: Option<String>,
    pub accepts_marketing: Option<bool>,
}

/// Update the signed-in customer's own profile
pub async fn update_current_customer(
    Extension(auth): Extension<JwtAuth>,
    State(state): State<AppState>,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<serde_json::Value>, Error> {
    request
        .validate()
        .map_err(|e| Error::validation(e.to_string()))?;
    let update = UpdateCustomerRequest {
        email: None,
        first_name: request.first_name.map(|s| s.trim().to_string()),
        last_name: request.last_name.map(|s| s.trim().to_string()),
        phone: request.phone.map(|s| s.trim().to_string()),
        accepts_marketing: request.accepts_marketing,
        tax_exempt: None,
    };
    let customer = state
        .customer_service
        .update_customer(auth.customer_id, update)
        .await?;
    Ok(Json(
        serde_json::json!({ "customer": customer_json(&customer) }),
    ))
}

/// Get customer by ID
pub async fn get_customer(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(auth): Extension<JwtAuth>,
) -> Result<Json<serde_json::Value>, Error> {
    // Users can only access their own profile unless admin
    if auth.customer_id != id && !auth.is_admin() {
        return Err(Error::unauthorized("Access denied"));
    }

    let customer_data = state.customer_service.get_customer(id).await?;

    match customer_data {
        Some(detail) => Ok(Json(detail_json(detail))),
        None => Err(Error::not_found("Customer not found")),
    }
}

/// Get current customer profile (requires auth)
pub async fn get_current_customer(
    Extension(auth): Extension<JwtAuth>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, Error> {
    let customer_data = state
        .customer_service
        .get_customer(auth.customer_id)
        .await?;

    match customer_data {
        Some(detail) => Ok(Json(detail_json(detail))),
        None => Err(Error::not_found("Customer not found")),
    }
}

/// List addresses for the current customer
pub async fn list_addresses(
    Extension(auth): Extension<JwtAuth>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, Error> {
    let addresses = state
        .customer_service
        .get_customer(auth.customer_id)
        .await?
        .map(|detail| detail.addresses)
        .unwrap_or_default();

    Ok(Json(serde_json::json!({ "addresses": addresses })))
}

/// Create a new address for the current customer
pub async fn create_address(
    Extension(auth): Extension<JwtAuth>,
    State(state): State<AppState>,
    Json(request): Json<CreateAddressRequest>,
) -> Result<(StatusCode, Json<Address>), Error> {
    super::require_address_phone(&request.phone)?;
    let address = state
        .customer_service
        .add_address(auth.customer_id, request)
        .await?;

    Ok((StatusCode::CREATED, Json(address)))
}

/// Update an existing address
pub async fn update_address(
    Extension(auth): Extension<JwtAuth>,
    State(state): State<AppState>,
    Path(address_id): Path<Uuid>,
    Json(request): Json<CreateAddressRequest>,
) -> Result<Json<Address>, Error> {
    super::require_address_phone(&request.phone)?;
    let address = state
        .customer_service
        .update_address(auth.customer_id, address_id, request)
        .await?;

    Ok(Json(address))
}

/// Delete an address
pub async fn delete_address(
    Extension(auth): Extension<JwtAuth>,
    State(state): State<AppState>,
    Path(address_id): Path<Uuid>,
) -> Result<StatusCode, Error> {
    state
        .customer_service
        .delete_address(auth.customer_id, address_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

use rcommerce_core::models::Address;

/// Router for customer routes
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/customers", get(list_customers))
        .route(
            "/customers/me",
            get(get_current_customer).put(update_current_customer),
        )
        .route("/customers/:id", get(get_customer))
        .route(
            "/customers/me/addresses",
            get(list_addresses).post(create_address),
        )
        .route(
            "/customers/me/addresses/:address_id",
            put(update_address).delete(delete_address),
        )
}
