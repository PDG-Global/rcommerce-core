//! Admin product routes for bundle management
//!
//! Provides endpoints for:
//! - Managing bundle components
//! - Uploading digital product files
//! - Managing license keys

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
        BundleComponentWithProduct, CreateBundleComponentRequest, ProductType,
        UpdateBundleComponentRequest,
    },
    BundleService,
};

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

/// List bundle components for a product
///
/// GET /api/v1/admin/products/:id/bundle-components
pub async fn list_bundle_components(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let product_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid product ID format"
            }));
        }
    };

    let bundle_service = BundleService::new(state.db.clone());

    match bundle_service.get_bundle_components(product_id).await {
        Ok(components) => {
            let components_json: Vec<BundleComponentResponse> = components
                .into_iter()
                .map(BundleComponentResponse::from)
                .collect();

            Json(serde_json::json!({
                "components": components_json
            }))
        }
        Err(e) => {
            tracing::error!("Failed to get bundle components: {}", e);
            Json(serde_json::json!({
                "error": "Failed to retrieve bundle components"
            }))
        }
    }
}

/// Add a component to a bundle
///
/// POST /api/v1/admin/products/:id/bundle-components
pub async fn add_bundle_component(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateBundleComponentBody>,
) -> Json<serde_json::Value> {
    let product_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid product ID format"
            }));
        }
    };

    let component_product_id = match Uuid::parse_str(&body.component_product_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid component product ID format"
            }));
        }
    };

    let request = CreateBundleComponentRequest {
        component_product_id,
        quantity: body.quantity,
        is_optional: body.is_optional,
        sort_order: body.sort_order,
    };

    let bundle_service = BundleService::new(state.db.clone());

    match bundle_service.add_component(product_id, request).await {
        Ok(component) => {
            // Get the component with product details
            match bundle_service.get_bundle_components(product_id).await {
                Ok(components) => {
                    let component_with_product = components
                        .into_iter()
                        .find(|c| c.component.id == component.id)
                        .map(BundleComponentResponse::from);

                    Json(serde_json::json!({
                        "success": true,
                        "component": component_with_product
                    }))
                }
                Err(_) => Json(serde_json::json!({
                    "success": true,
                    "component": {
                        "id": component.id,
                        "bundle_product_id": component.bundle_product_id,
                        "component_product_id": component.component_product_id,
                        "quantity": component.quantity,
                        "is_optional": component.is_optional,
                        "sort_order": component.sort_order,
                    }
                })),
            }
        }
        Err(e) => {
            tracing::error!("Failed to add bundle component: {}", e);
            Json(serde_json::json!({
                "error": e.to_string()
            }))
        }
    }
}

/// Update a bundle component
///
/// PUT /api/v1/admin/products/:id/bundle-components/:component_id
pub async fn update_bundle_component(
    State(state): State<AppState>,
    Path((id, component_id)): Path<(String, String)>,
    Json(body): Json<UpdateBundleComponentBody>,
) -> Json<serde_json::Value> {
    let product_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid product ID format"
            }));
        }
    };

    let component_id = match Uuid::parse_str(&component_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid component ID format"
            }));
        }
    };

    let request = UpdateBundleComponentRequest {
        quantity: body.quantity,
        is_optional: body.is_optional,
        sort_order: body.sort_order,
    };

    let bundle_service = BundleService::new(state.db.clone());

    match bundle_service
        .update_component(product_id, component_id, request)
        .await
    {
        Ok(component) => Json(serde_json::json!({
            "success": true,
            "component": {
                "id": component.id,
                "bundle_product_id": component.bundle_product_id,
                "component_product_id": component.component_product_id,
                "quantity": component.quantity,
                "is_optional": component.is_optional,
                "sort_order": component.sort_order,
            }
        })),
        Err(e) => {
            tracing::error!("Failed to update bundle component: {}", e);
            Json(serde_json::json!({
                "error": e.to_string()
            }))
        }
    }
}

/// Remove a component from a bundle
///
/// DELETE /api/v1/admin/products/:id/bundle-components/:component_id
pub async fn remove_bundle_component(
    State(state): State<AppState>,
    Path((id, component_id)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    let product_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid product ID format"
            }));
        }
    };

    let component_id = match Uuid::parse_str(&component_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid component ID format"
            }));
        }
    };

    let bundle_service = BundleService::new(state.db.clone());

    match bundle_service
        .remove_component(product_id, component_id)
        .await
    {
        Ok(true) => Json(serde_json::json!({
            "success": true,
            "message": "Component removed successfully"
        })),
        Ok(false) => Json(serde_json::json!({
            "error": "Component not found"
        })),
        Err(e) => {
            tracing::error!("Failed to remove bundle component: {}", e);
            Json(serde_json::json!({
                "error": e.to_string()
            }))
        }
    }
}

/// Generate license keys for a digital product
///
/// POST /api/v1/admin/products/:id/license-keys
#[derive(Debug, Deserialize)]
pub struct GenerateLicenseKeysBody {
    pub count: i32,
}

pub async fn generate_license_keys(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<GenerateLicenseKeysBody>,
) -> Json<serde_json::Value> {
    let product_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid product ID format"
            }));
        }
    };

    // Verify product is a digital product with license keys enabled
    let product = match sqlx::query_as::<_, rcommerce_core::models::Product>(
        "SELECT * FROM products WHERE id = $1",
    )
    .bind(product_id)
    .fetch_one(state.db.pool())
    .await
    {
        Ok(p) => p,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Product not found"
            }));
        }
    };

    if !matches!(product.product_type, ProductType::Digital) {
        return Json(serde_json::json!({
            "error": "Product is not a digital product"
        }));
    }

    if !product.license_key_enabled.unwrap_or(false) {
        return Json(serde_json::json!({
            "error": "License keys are not enabled for this product"
        }));
    }

    match state
        .digital_product_service
        .generate_license_keys(product_id, body.count)
        .await
    {
        Ok(keys) => {
            let keys_json: Vec<serde_json::Value> = keys
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

            Json(serde_json::json!({
                "success": true,
                "license_keys": keys_json
            }))
        }
        Err(e) => {
            tracing::error!("Failed to generate license keys: {}", e);
            Json(serde_json::json!({
                "error": e.to_string()
            }))
        }
    }
}

/// List license keys for a product
///
/// GET /api/v1/admin/products/:id/license-keys
pub async fn list_license_keys(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let product_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid product ID format"
            }));
        }
    };

    match state
        .digital_product_service
        .get_product_license_keys(product_id)
        .await
    {
        Ok(keys) => {
            let keys_json: Vec<serde_json::Value> = keys
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

            Json(serde_json::json!({
                "license_keys": keys_json
            }))
        }
        Err(e) => {
            tracing::error!("Failed to get license keys: {}", e);
            Json(serde_json::json!({
                "error": e.to_string()
            }))
        }
    }
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
