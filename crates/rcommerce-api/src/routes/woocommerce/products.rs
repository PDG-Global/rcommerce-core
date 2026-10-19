//! WooCommerce-compatible product endpoints.
//!
//! GET /wp-json/wc/v3/products
//! GET /wp-json/wc/v3/products/:id
//! GET /wp-json/wc/v3/products/categories

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;

use crate::middleware::api_key_auth::ApiKeyAuth;
use crate::state::AppState;
use rcommerce_core::repository::{CategoryRepository, PostgresCategoryRepository};
use rcommerce_core::services::PaginationParams;

use super::pagination;
use super::types::*;

#[derive(Debug, Deserialize, Default)]
pub struct ProductListParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub search: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
}

/// GET /wp-json/wc/v3/products
pub async fn list_products(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
    Query(params): Query<ProductListParams>,
) -> Response {
    let (page, per_page) = pagination::parse_pagination(params.page, params.per_page);

    let product_list = match state
        .product_service
        .list_products(None, PaginationParams { page, per_page })
        .await
    {
        Ok(list) => list,
        Err(e) => {
            tracing::error!("WC products: {}", e);
            return super::super::woocommerce_error(
                500,
                "internal_server_error",
                "Internal server error.",
            );
        }
    };

    let total = product_list.pagination.total;
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());

    let mut wc_products = Vec::new();
    for p in product_list.products {
        // Get categories
        let categories = cat_repo
            .get_product_categories(p.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|c| WcCategoryRef {
                id: c.id.to_string(),
                name: c.name,
                slug: c.slug,
            })
            .collect();

        // Get first image
        let images = state
            .product_service
            .repository
            .find_images(p.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, img)| WcImage {
                id: i.to_string(),
                src: img.src,
                alt: img.alt_text.unwrap_or_default(),
            })
            .collect();

        wc_products.push(WcProduct {
            id: p.id.to_string(),
            name: p.title,
            slug: p.slug,
            status: if p.is_active { "publish" } else { "draft" }.to_string(),
            product_type: "simple".to_string(),
            description: p.description.unwrap_or_default(),
            short_description: p.seo_description.unwrap_or_default(),
            sku: p.sku.unwrap_or_default(),
            price: format_price(&p.price),
            regular_price: format_price(&p.price),
            sale_price: p
                .compare_at_price
                .map(|c| format_price(&c))
                .unwrap_or_default(),
            stock_quantity: p.inventory_quantity,
            stock_status: if p.inventory_quantity > 0 {
                "instock"
            } else {
                "outofstock"
            }
            .to_string(),
            categories,
            images,
            date_created: p.created_at.to_rfc3339(),
            date_modified: p.updated_at.to_rfc3339(),
        });
    }

    let mut response = Json(serde_json::to_value(&wc_products).unwrap_or_default()).into_response();
    let headers = pagination::pagination_headers(
        total,
        page,
        per_page,
        "https://example.com/wp-json/wc/v3/products",
    );
    response.headers_mut().extend(headers);
    response
}

/// Resolve a product identifier from the WC API path: a raw UUID, the
/// WC-compatible integer id (`i64::from_be_bytes` of the UUID's first 8
/// bytes, `unsigned_abs`) that order line items expose, or (handled by the
/// caller) a slug.
async fn resolve_product_id(
    state: &AppState,
    id: &str,
) -> Result<uuid::Uuid, axum::response::Response> {
    if let Ok(uuid) = uuid::Uuid::parse_str(id) {
        return Ok(uuid);
    }
    if let Ok(int_id) = id.parse::<i64>() {
        // map_product exposes unsigned_abs(i64); abs() because high-bit UUIDs
        // produce negative bit(64)::bigint values
        let lookup: Option<(uuid::Uuid,)> = sqlx::query_as(
            r#"SELECT id FROM products
               WHERE abs(('x' || encode(substring(uuid_send(id) from 1 for 8), 'hex'))::bit(64)::bigint) = $1"#,
        )
        .bind(int_id)
        .fetch_optional(state.db.pool())
        .await
        .unwrap_or(None);
        if let Some((uuid,)) = lookup {
            return Ok(uuid);
        }
    }
    Err(super::super::woocommerce_error(
        400,
        "woocommerce_rest_invalid_id",
        "Invalid ID.",
    ))
}

pub async fn get_product(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
    Path(id): Path<String>,
) -> Response {
    let detail = if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
        state.product_service.get_product(uuid).await
    } else if id.parse::<i64>().is_ok() {
        // Dianxiaomi only knows the WC integer id from order line items
        match resolve_product_id(&state, &id).await {
            Ok(uuid) => state.product_service.get_product(uuid).await,
            Err(response) => return response,
        }
    } else {
        state.product_service.get_product_by_slug(&id).await
    };

    let detail = match detail {
        Ok(Some(d)) => d,
        Ok(None) => {
            return super::super::woocommerce_error(
                404,
                "woocommerce_rest_invalid_id",
                "Invalid ID.",
            );
        }
        Err(e) => {
            tracing::error!("WC product get: {}", e);
            return super::super::woocommerce_error(
                500,
                "internal_server_error",
                "Internal server error.",
            );
        }
    };

    let p = detail.product;
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());

    let categories = cat_repo
        .get_product_categories(p.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|c| WcCategoryRef {
            id: c.id.to_string(),
            name: c.name,
            slug: c.slug,
        })
        .collect();

    let images = detail
        .images
        .into_iter()
        .enumerate()
        .map(|(i, img)| WcImage {
            id: i.to_string(),
            src: img.src,
            alt: img.alt_text.unwrap_or_default(),
        })
        .collect();

    let wc = WcProduct {
        id: p.id.to_string(),
        name: p.title,
        slug: p.slug,
        status: if p.is_active { "publish" } else { "draft" }.to_string(),
        product_type: "simple".to_string(),
        description: p.description.unwrap_or_default(),
        short_description: p.seo_description.unwrap_or_default(),
        sku: p.sku.unwrap_or_default(),
        price: format_price(&p.price),
        regular_price: format_price(&p.price),
        sale_price: p
            .compare_at_price
            .map(|c| format_price(&c))
            .unwrap_or_default(),
        stock_quantity: p.inventory_quantity,
        stock_status: if p.inventory_quantity > 0 {
            "instock"
        } else {
            "outofstock"
        }
        .to_string(),
        categories,
        images,
        date_created: p.created_at.to_rfc3339(),
        date_modified: p.updated_at.to_rfc3339(),
    };

    Json(serde_json::to_value(&wc).unwrap_or_default()).into_response()
}

/// GET /wp-json/wc/v3/products/categories
pub async fn list_categories(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
) -> Response {
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
    let categories = match cat_repo.list(100, 0).await {
        Ok(cats) => cats,
        Err(e) => {
            tracing::error!("WC categories: {}", e);
            return super::super::woocommerce_error(
                500,
                "internal_server_error",
                "Internal server error.",
            );
        }
    };

    let wc_categories: Vec<WcProductCategory> = categories
        .into_iter()
        .map(|c| WcProductCategory {
            id: c.id.to_string(),
            name: c.name,
            slug: c.slug,
            description: c.description.unwrap_or_default(),
            count: 0, // Would need a join query for accurate count
        })
        .collect();

    Json(serde_json::to_value(&wc_categories).unwrap_or_default()).into_response()
}
