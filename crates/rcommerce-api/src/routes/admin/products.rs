//! Admin product routes for bundle management
//!
//! Provides endpoints for:
//! - Managing bundle components
//! - Uploading digital product files
//! - Managing license keys
//!
//! Failures answer with their status through [`Error`]: 400 for invalid input,
//! 404 for a missing product or component, 5xx with a generic message for
//! server failures.

use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;
use rcommerce_core::{
    models::{
        BundleComponent, BundleComponentWithProduct, CreateBundleComponentRequest, Product,
        ProductType, UpdateBundleComponentRequest,
    },
    BundleService, Error,
};

/// Most license keys one request can generate
const MAX_LICENSE_KEYS_PER_REQUEST: i32 = 1000;

/// JSON body on success, the error's status and message otherwise
type JsonResult = Result<Json<serde_json::Value>, Error>;

/// Create bundle component request
#[derive(Debug, Deserialize)]
pub struct CreateBundleComponentBody {
    pub component_product_id: String,
    pub quantity: i32,
    #[serde(default)]
    pub is_optional: bool,
    #[serde(default)]
    pub sort_order: i32,
}

/// Update bundle component request
#[derive(Debug, Deserialize)]
pub struct UpdateBundleComponentBody {
    pub quantity: Option<i32>,
    pub is_optional: Option<bool>,
    pub sort_order: Option<i32>,
}

/// Bundle component response
#[derive(Debug, Serialize)]
pub struct BundleComponentResponse {
    pub id: String,
    pub bundle_product_id: String,
    pub component_product_id: String,
    pub quantity: i32,
    pub is_optional: bool,
    pub sort_order: i32,
    pub product: Option<serde_json::Value>,
}

impl From<BundleComponentWithProduct> for BundleComponentResponse {
    fn from(bc: BundleComponentWithProduct) -> Self {
        Self {
            id: bc.component.id.to_string(),
            bundle_product_id: bc.component.bundle_product_id.to_string(),
            component_product_id: bc.component.component_product_id.to_string(),
            quantity: bc.component.quantity,
            is_optional: bc.component.is_optional,
            sort_order: bc.component.sort_order,
            product: bc.product.map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "title": p.title,
                    "slug": p.slug,
                    "price": p.price,
                    "sku": p.sku,
                    "is_active": p.is_active,
                })
            }),
        }
    }
}

/// Parse an id from the request; a malformed one is a client error
fn parse_id(id: &str, what: &str) -> Result<Uuid, Error> {
    Uuid::parse_str(id).map_err(|_| Error::validation(format!("Invalid {} ID format", what)))
}

/// A bundle component row, without its product's details
fn component_json(component: &BundleComponent) -> serde_json::Value {
    serde_json::json!({
        "id": component.id,
        "bundle_product_id": component.bundle_product_id,
        "component_product_id": component.component_product_id,
        "quantity": component.quantity,
        "is_optional": component.is_optional,
        "sort_order": component.sort_order,
    })
}

/// List bundle components for a product
///
/// GET /api/v1/admin/products/:id/bundle-components
pub async fn list_bundle_components(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> JsonResult {
    let product_id = parse_id(&id, "product")?;
    let components: Vec<BundleComponentResponse> = BundleService::new(state.db.clone())
        .get_bundle_components(product_id)
        .await?
        .into_iter()
        .map(BundleComponentResponse::from)
        .collect();

    Ok(Json(serde_json::json!({
        "components": components
    })))
}

/// Add a component to a bundle
///
/// POST /api/v1/admin/products/:id/bundle-components
pub async fn add_bundle_component(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateBundleComponentBody>,
) -> JsonResult {
    let product_id = parse_id(&id, "product")?;
    let request = CreateBundleComponentRequest {
        component_product_id: parse_id(&body.component_product_id, "component product")?,
        quantity: body.quantity,
        is_optional: body.is_optional,
        sort_order: body.sort_order,
    };

    let bundle_service = BundleService::new(state.db.clone());
    let component = bundle_service.add_component(product_id, request).await?;

    // The component with its product's details, or the bare row if they
    // cannot be loaded
    let component_json = match bundle_service.get_bundle_components(product_id).await {
        Ok(components) => serde_json::to_value(
            components
                .into_iter()
                .find(|c| c.component.id == component.id)
                .map(BundleComponentResponse::from),
        )?,
        Err(_) => component_json(&component),
    };

    Ok(Json(serde_json::json!({
        "success": true,
        "component": component_json
    })))
}

/// Update a bundle component
///
/// PUT /api/v1/admin/products/:id/bundle-components/:component_id
pub async fn update_bundle_component(
    State(state): State<AppState>,
    Path((id, component_id)): Path<(String, String)>,
    Json(body): Json<UpdateBundleComponentBody>,
) -> JsonResult {
    let product_id = parse_id(&id, "product")?;
    let component_id = parse_id(&component_id, "component")?;
    let request = UpdateBundleComponentRequest {
        quantity: body.quantity,
        is_optional: body.is_optional,
        sort_order: body.sort_order,
    };

    let component = BundleService::new(state.db.clone())
        .update_component(product_id, component_id, request)
        .await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "component": component_json(&component)
    })))
}

/// Remove a component from a bundle
///
/// DELETE /api/v1/admin/products/:id/bundle-components/:component_id
pub async fn remove_bundle_component(
    State(state): State<AppState>,
    Path((id, component_id)): Path<(String, String)>,
) -> JsonResult {
    let product_id = parse_id(&id, "product")?;
    let component_id = parse_id(&component_id, "component")?;

    let removed = BundleService::new(state.db.clone())
        .remove_component(product_id, component_id)
        .await?;
    if !removed {
        return Err(Error::not_found("Component not found"));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Component removed successfully"
    })))
}

/// Generate license keys for a digital product
///
/// POST /api/v1/admin/products/:id/license-keys
#[derive(Debug, Deserialize)]
pub struct GenerateLicenseKeysBody {
    pub count: i32,
}

/// Generate `count` (1 to 1000) license keys for a digital product with
/// license keys enabled
pub async fn generate_license_keys(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<GenerateLicenseKeysBody>,
) -> JsonResult {
    let product_id = parse_id(&id, "product")?;
    if !(1..=MAX_LICENSE_KEYS_PER_REQUEST).contains(&body.count) {
        return Err(Error::validation(format!(
            "count must be between 1 and {}",
            MAX_LICENSE_KEYS_PER_REQUEST
        )));
    }

    let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
        .bind(product_id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or_else(|| Error::not_found("Product not found"))?;
    if !matches!(product.product_type, ProductType::Digital) {
        return Err(Error::validation("Product is not a digital product"));
    }
    if !product.license_key_enabled.unwrap_or(false) {
        return Err(Error::validation(
            "License keys are not enabled for this product",
        ));
    }

    let keys_json: Vec<serde_json::Value> = state
        .digital_product_service
        .generate_license_keys(product_id, body.count)
        .await?
        .into_iter()
        .map(|k| {
            serde_json::json!({
                "id": k.id,
                "license_key": k.license_key,
                "is_used": k.is_used,
                "created_at": k.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "success": true,
        "license_keys": keys_json
    })))
}

/// List license keys for a product
///
/// GET /api/v1/admin/products/:id/license-keys
pub async fn list_license_keys(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> JsonResult {
    let product_id = parse_id(&id, "product")?;
    let keys_json: Vec<serde_json::Value> = state
        .digital_product_service
        .get_product_license_keys(product_id)
        .await?
        .into_iter()
        .map(|k| {
            serde_json::json!({
                "id": k.id,
                "license_key": k.license_key,
                "is_used": k.is_used,
                "used_at": k.used_at,
                "order_item_id": k.order_item_id,
                "customer_id": k.customer_id,
                "created_at": k.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "license_keys": keys_json
    })))
}

/// Router for admin product routes
pub fn router() -> Router<AppState> {
    Router::new()
        // Bundle management
        .route(
            "/admin/products/:id/bundle-components",
            get(list_bundle_components),
        )
        .route(
            "/admin/products/:id/bundle-components",
            post(add_bundle_component),
        )
        .route(
            "/admin/products/:id/bundle-components/:component_id",
            put(update_bundle_component),
        )
        .route(
            "/admin/products/:id/bundle-components/:component_id",
            delete(remove_bundle_component),
        )
        // License key management
        .route("/admin/products/:id/license-keys", get(list_license_keys))
        .route(
            "/admin/products/:id/license-keys",
            post(generate_license_keys),
        )
}

#[cfg(test)]
mod tests {
    use super::parse_id;

    #[test]
    fn malformed_ids_are_client_errors() {
        assert!(parse_id("6f1c7a52-0d1e-4f7a-9b8e-1f2a3b4c5d6e", "product").is_ok());
        let error = parse_id("not-a-uuid", "component").unwrap_err();
        assert_eq!(error.status_code(), 400);
        assert!(error.to_string().contains("Invalid component ID format"));
    }
}
