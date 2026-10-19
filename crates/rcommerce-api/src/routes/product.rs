use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Extension, Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::middleware::JwtAuth;
use crate::state::AppState;
use rcommerce_core::models::{CreateProductRequest, UpdateProductRequest};
use rcommerce_core::repository::{CategoryRepository, PostgresCategoryRepository};
use rcommerce_core::services::product_service::ProductList;
use rcommerce_core::services::PaginationParams;

/// List products from database
pub async fn list_products(
    State(state): State<AppState>,
    Query(params): Query<ListProductsParams>,
) -> Json<serde_json::Value> {
    let category_filter = if let Some(category_slug) = params.category.as_deref() {
        let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
        match cat_repo.get_by_slug(category_slug).await {
            Ok(Some(cat)) => Some(cat.id),
            Ok(None) => {
                return Json(serde_json::json!({
                    "products": [],
                    "meta": {
                        "total": 0,
                        "page": 1,
                        "per_page": 20,
                        "total_pages": 0
                    }
                }));
            }
            Err(e) => {
                tracing::error!("Failed to lookup category {}: {}", category_slug, e);
                return Json(serde_json::json!({
                    "products": [],
                    "meta": {
                        "total": 0,
                        "page": 1,
                        "per_page": 20,
                        "total_pages": 0
                    }
                }));
            }
        }
    } else {
        None
    };

    let pagination = PaginationParams {
        page: params.page.filter(|p| *p >= 1).unwrap_or(1),
        per_page: params.per_page.unwrap_or(20).clamp(1, 100),
    };
    let filter = rcommerce_core::models::ProductFilter {
        status: Some(rcommerce_core::models::ProductStatus::Active),
        category_id: category_filter,
        query: params
            .q
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(|q| q.to_string()),
        ..Default::default()
    };
    let product_list = match state
        .product_service
        .list_products(Some(filter), pagination)
        .await
    {
        Ok(list) => list,
        Err(e) => {
            tracing::error!("Failed to list products: {}", e);
            return Json(serde_json::json!({
                "products": [],
                "meta": {
                    "total": 0,
                    "page": 1,
                    "per_page": 20,
                    "total_pages": 0
                }
            }));
        }
    };

    // Fetch first image and categories for each product (small catalog, simple N+1 is fine)
    let mut images_by_product: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();
    let mut categories_by_product: std::collections::HashMap<Uuid, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
    for p in &product_list.products {
        if let Ok(images) = state.product_service.repository.find_images(p.id).await {
            if let Some(img) = images.first().cloned() {
                images_by_product.insert(p.id, img.src.clone());
            }
        }
        if let Ok(cats) = cat_repo.get_product_categories(p.id).await {
            categories_by_product.insert(
                p.id,
                cats.into_iter()
                    .map(|c| serde_json::json!({"id": c.id, "name": c.name, "slug": c.slug}))
                    .collect(),
            );
        }
    }

    // Review aggregates for the listed products (single batched query)
    let mut reviews_by_product: std::collections::HashMap<Uuid, (f64, i64)> =
        std::collections::HashMap::new();
    let product_ids: Vec<Uuid> = product_list.products.iter().map(|p| p.id).collect();
    if !product_ids.is_empty() {
        let agg = sqlx::query_as::<_, (Uuid, Option<f64>, i64)>(
            r#"
            SELECT product_id, AVG(rating)::float8, COUNT(*)::int8
            FROM product_reviews
            WHERE product_id = ANY($1) AND status = 'approved'
            GROUP BY product_id
            "#,
        )
        .bind(&product_ids)
        .fetch_all(state.db.pool())
        .await;
        if let Ok(rows) = agg {
            for (product_id, avg, count) in rows {
                reviews_by_product.insert(product_id, (avg.unwrap_or(0.0), count));
            }
        }
    }

    // Derived stock for bundle products (sellable = min over required components)
    let bundle_ids: Vec<Uuid> = product_list
        .products
        .iter()
        .filter(|p| matches!(p.product_type, rcommerce_core::models::ProductType::Bundle))
        .map(|p| p.id)
        .collect();
    let bundle_stock: std::collections::HashMap<Uuid, i32> = if bundle_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        state
            .bundle_service
            .available_stock_many(&bundle_ids)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to derive bundle stock for list: {}", e);
                std::collections::HashMap::new()
            })
    };

    let products: Vec<serde_json::Value> = product_list
        .products
        .into_iter()
        .map(|p| {
            let image = images_by_product.get(&p.id).cloned();
            let categories = categories_by_product.remove(&p.id).unwrap_or_default();
            let (average_rating, review_count) =
                reviews_by_product.get(&p.id).copied().unwrap_or((0.0, 0));
            serde_json::json!({
                "id": p.id,
                "title": p.title,
                "slug": p.slug,
                "sku": p.sku,
                "price": p.price,
                "currency": p.currency,
                "description": p.description,
                "seo_description": p.seo_description,
                "is_active": p.is_active,
                "product_type": format!("{:?}", p.product_type).to_lowercase(),
                "inventory_quantity": bundle_stock.get(&p.id).copied().unwrap_or(p.inventory_quantity),
                "image": image,
                "categories": categories,
                "average_rating": average_rating,
                "review_count": review_count,
                "created_at": p.created_at
            })
        })
        .collect();

    Json(serde_json::json!({
        "products": products,
        "meta": {
            "total": product_list.pagination.total,
            "page": product_list.pagination.page,
            "per_page": product_list.pagination.per_page,
            "total_pages": product_list.pagination.total_pages
        }
    }))
}

#[derive(Debug, Deserialize, Default)]
pub struct ListProductsParams {
    category: Option<String>,
    /// Case-insensitive search across product title and description
    q: Option<String>,
    /// 1-based page number (default 1)
    page: Option<i64>,
    /// Page size, clamped to 1..=100 (default 20)
    per_page: Option<i64>,
}

/// Get product by ID or slug from database
pub async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    // Try UUID first, fall back to slug lookup
    let product_detail = if let Ok(uuid) = Uuid::parse_str(&id) {
        state.product_service.get_product(uuid).await
    } else {
        state.product_service.get_product_by_slug(&id).await
    };

    let product_detail = match product_detail {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            return Json(serde_json::json!({
                "error": "Product not found"
            }));
        }
        Err(e) => {
            tracing::error!("Failed to get product {}: {}", id, e);
            return Json(serde_json::json!({
                "error": "Failed to retrieve product"
            }));
        }
    };

    let p = product_detail.product;
    // Bundles report stock derived from their required components
    let inventory_quantity =
        if matches!(p.product_type, rcommerce_core::models::ProductType::Bundle) {
            match state.bundle_service.available_stock(p.id).await {
                Ok(stock) => stock,
                Err(e) => {
                    tracing::error!("Failed to derive bundle stock for product {}: {}", p.id, e);
                    p.inventory_quantity
                }
            }
        } else {
            p.inventory_quantity
        };
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
    let categories: Vec<serde_json::Value> = cat_repo
        .get_product_categories(p.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|c| serde_json::json!({"id": c.id, "name": c.name, "slug": c.slug}))
        .collect();

    Json(serde_json::json!({
    "product": {
        "id": p.id,
        "title": p.title,
        "slug": p.slug,
        "description": p.description,
        "sku": p.sku,
        "gtin": p.gtin,
        "price": p.price,
        "compare_at_price": p.compare_at_price,
                "cost_price": p.cost_price,
                "currency": p.currency,
                "product_type": format!("{:?}", p.product_type).to_lowercase(),
                "inventory_quantity": inventory_quantity,
                "inventory_policy": p.inventory_policy,
                "inventory_management": p.inventory_management,
                "weight": p.weight,
                "weight_unit": p.weight_unit,
                "requires_shipping": p.requires_shipping,
                "is_active": p.is_active,
                "is_featured": p.is_featured,
                "seo_title": p.seo_title,
                "seo_description": p.seo_description,
                "created_at": p.created_at,
                "updated_at": p.updated_at,
                "published_at": p.published_at,
                "categories": categories,
                "variants": product_detail.variants.into_iter().map(|v| serde_json::json!({
                    "id": v.id,
                    "title": v.title,
                    "sku": v.sku,
                    "gtin": v.gtin,
                    "price": v.price,
                    "inventory_quantity": v.inventory_quantity
                })).collect::<Vec<_>>(),
                "images": product_detail.images.into_iter().map(|i| serde_json::json!({
                    "id": i.id,
                    "src": i.src,
                    "alt_text": i.alt_text
                })).collect::<Vec<_>>(),
                "options": product_detail.options.into_iter().map(|(opt, values)| serde_json::json!({
                    "id": opt.id,
                    "name": opt.name,
                    "position": opt.position,
                    "values": values.into_iter().map(|v| serde_json::json!({
                        "id": v.id,
                        "variant_id": v.variant_id,
                        "value": v.value
                    })).collect::<Vec<_>>()
                })).collect::<Vec<_>>()
            }
        }))
}

/// Derive a short purchase-option label from a pack product's title.
/// - contains "race pack" (case-insensitive) → "Race Pack"
/// - contains "party pack" or "大礼包" → "Party Pack"
/// - otherwise strip the base product's title prefix; fall back to "Pack"
fn pack_option_label(pack_title: &str, base_title: &str) -> String {
    let lower = pack_title.to_lowercase();
    if lower.contains("race pack") {
        return "Race Pack".to_string();
    }
    if lower.contains("party pack") || pack_title.contains("大礼包") {
        return "Party Pack".to_string();
    }
    let remainder = pack_title
        .trim_start_matches(base_title)
        .trim()
        .trim_start_matches(|c: char| c == '-' || c == '–' || c == '—' || c == ':')
        .trim();
    if remainder.is_empty() {
        "Pack".to_string()
    } else {
        remainder.to_string()
    }
}

/// List pack purchase options for a base product (public).
/// Returns one option per distinct active bundle product that has this
/// product as a bundle component, sorted by price ascending.
pub async fn get_pack_options(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let detail = state
        .product_service
        .get_product(id)
        .await?
        .ok_or_else(|| rcommerce_core::Error::not_found("Product not found"))?;
    let base_title = detail.product.title;

    let mut packs = sqlx::query_as::<_, rcommerce_core::models::Product>(
        r#"
        SELECT DISTINCT ON (p.id) p.*
        FROM bundle_components bc
        JOIN products p ON p.id = bc.bundle_product_id
        WHERE bc.component_product_id = $1
          AND p.id <> $1
          AND p.is_active = true
        ORDER BY p.id, p.price ASC
        "#,
    )
    .bind(id)
    .fetch_all(state.db.pool())
    .await?;
    packs.sort_by(|a, b| a.price.cmp(&b.price));

    // Packs are bundles: report stock derived from their components
    let pack_ids: Vec<Uuid> = packs
        .iter()
        .filter(|p| matches!(p.product_type, rcommerce_core::models::ProductType::Bundle))
        .map(|p| p.id)
        .collect();
    let pack_stock: std::collections::HashMap<Uuid, i32> = if pack_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        state
            .bundle_service
            .available_stock_many(&pack_ids)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to derive pack stock: {}", e);
                std::collections::HashMap::new()
            })
    };

    let options: Vec<serde_json::Value> = packs
        .into_iter()
        .map(|p| {
            serde_json::json!({
                "product_id": p.id,
                "title": p.title,
                "slug": p.slug,
                "price": p.price,
                "currency": p.currency,
                "inventory_quantity": pack_stock.get(&p.id).copied().unwrap_or(p.inventory_quantity),
                "label": pack_option_label(&p.title, &base_title),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "options": options })))
}

/// Create a new product (admin only)
pub async fn create_product(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(request): Json<CreateProductRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), rcommerce_core::Error> {
    if !auth.is_admin() {
        return Err(rcommerce_core::Error::unauthorized("Admin access required"));
    }

    let product = state.product_service.create_product(request).await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": product.id,
            "title": product.title,
            "slug": product.slug,
            "price": product.price,
            "currency": product.currency,
            "is_active": product.is_active,
            "created_at": product.created_at,
        })),
    ))
}

/// Update a product (admin only)
pub async fn update_product(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateProductRequest>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    if !auth.is_admin() {
        return Err(rcommerce_core::Error::unauthorized("Admin access required"));
    }

    let product = state.product_service.update_product(id, request).await?;

    Ok(Json(serde_json::json!({
        "id": product.id,
        "title": product.title,
        "slug": product.slug,
        "price": product.price,
        "currency": product.currency,
        "is_active": product.is_active,
        "updated_at": product.updated_at,
    })))
}

/// Delete a product (admin only)
pub async fn delete_product(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, rcommerce_core::Error> {
    if !auth.is_admin() {
        return Err(rcommerce_core::Error::unauthorized("Admin access required"));
    }

    state.product_service.delete_product(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Router for public product routes
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/products", get(list_products))
        .route("/products/:id", get(get_product))
        .route("/products/:id/pack-options", get(get_pack_options))
        .route("/categories", get(list_categories))
        .route("/categories/:slug", get(get_category))
}

/// List all active product categories
pub async fn list_categories(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
    match cat_repo.list(1000, 0).await {
        Ok(categories) => Json(serde_json::json!({
            "categories": categories.into_iter().map(|c| serde_json::json!({
                "id": c.id,
                "name": c.name,
                "slug": c.slug,
                "description": c.description,
            })).collect::<Vec<_>>()
        })),
        Err(e) => {
            tracing::error!("Failed to list categories: {}", e);
            Json(serde_json::json!({ "categories": [] }))
        }
    }
}

/// Get a single category by slug and include its products
pub async fn get_category(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Json<serde_json::Value> {
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
    match cat_repo.get_by_slug(&slug).await {
        Ok(Some(category)) => {
            let products = match cat_repo.get_products(category.id, 100, 0).await {
                Ok(products) => products
                    .into_iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id,
                            "title": p.title,
                            "slug": p.slug,
                            "price": p.price,
                            "currency": p.currency,
                            "is_active": p.is_active,
                        })
                    })
                    .collect::<Vec<_>>(),
                Err(e) => {
                    tracing::error!("Failed to fetch category products: {}", e);
                    vec![]
                }
            };
            Json(serde_json::json!({
                "category": {
                    "id": category.id,
                    "name": category.name,
                    "slug": category.slug,
                    "description": category.description,
                },
                "products": products,
            }))
        }
        Ok(None) => Json(serde_json::json!({ "error": "Category not found" })),
        Err(e) => {
            tracing::error!("Failed to get category: {}", e);
            Json(serde_json::json!({ "error": "Failed to retrieve category" }))
        }
    }
}

/// Router for admin product routes (requires admin middleware to be applied by caller)
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/products", post(create_product))
        .route("/products/:id", put(update_product).delete(delete_product))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_option_label_race_pack() {
        assert_eq!(
            pack_option_label("Big Mitty Race Pack (16 sets)", "Big Mitty"),
            "Race Pack"
        );
        assert_eq!(pack_option_label("McFLY RACE PACK", "McFly"), "Race Pack");
    }

    #[test]
    fn pack_option_label_party_pack() {
        assert_eq!(
            pack_option_label("Big Mitty Party Pack", "Big Mitty"),
            "Party Pack"
        );
        assert_eq!(pack_option_label("McFly 大礼包", "McFly"), "Party Pack");
        assert_eq!(
            pack_option_label("Party Pack - McFly", "McFly"),
            "Party Pack"
        );
    }

    #[test]
    fn pack_option_label_strips_base_title() {
        assert_eq!(
            pack_option_label("Big Mitty - 4 Pack", "Big Mitty"),
            "4 Pack"
        );
        assert_eq!(pack_option_label("McFly Value Pack", "McFly"), "Value Pack");
    }

    #[test]
    fn pack_option_label_fallback() {
        assert_eq!(pack_option_label("Big Mitty", "Big Mitty"), "Pack");
        assert_eq!(pack_option_label("McFly -", "McFly"), "Pack");
    }
}
