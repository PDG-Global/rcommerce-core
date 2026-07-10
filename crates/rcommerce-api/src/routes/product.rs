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
use rcommerce_core::services::PaginationParams;
use rcommerce_core::services::product_service::ProductList;

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

    let pagination = PaginationParams::default();
    let product_list = match category_filter {
        Some(category_id) => {
            let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
            match cat_repo.get_products(category_id, pagination.limit(), pagination.offset()).await {
                Ok(products) => {
                    let total = products.len() as i64;
                    ProductList {
                        products,
                        pagination: rcommerce_core::services::PaginationInfo {
                            total,
                            page: pagination.page,
                            per_page: pagination.limit(),
                            total_pages: 1,
                        },
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to fetch products by category: {}", e);
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
        }
        None => match state
            .product_service
            .list_products(None, pagination)
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
        },
    };

    // Fetch first image for each product (small catalog, simple N+1 is fine)
    let mut images_by_product: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();
    for p in &product_list.products {
        if let Ok(images) = state.product_service.repository.find_images(p.id).await {
            if let Some(img) = images.first().cloned() {
                images_by_product.insert(p.id, img.src.clone());
            }
        }
    }

    let products: Vec<serde_json::Value> = product_list
        .products
        .into_iter()
        .map(|p| {
            let image = images_by_product.get(&p.id).cloned();
            serde_json::json!({
                "id": p.id,
                "title": p.title,
                "slug": p.slug,
                "price": p.price,
                "currency": p.currency,
                "description": p.description,
                "seo_description": p.seo_description,
                "is_active": p.is_active,
                "inventory_quantity": p.inventory_quantity,
                "image": image,
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
}

/// Get product by ID from database
pub async fn get_product(
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

    match state.product_service.get_product(product_id).await {
        Ok(Some(product_detail)) => {
            let p = product_detail.product;
            Json(serde_json::json!({
                "product": {
                    "id": p.id,
                    "title": p.title,
                    "slug": p.slug,
                    "description": p.description,
                    "sku": p.sku,
                    "price": p.price,
                    "compare_at_price": p.compare_at_price,
                    "cost_price": p.cost_price,
                    "currency": p.currency,
                    "product_type": format!("{:?}", p.product_type).to_lowercase(),
                    "inventory_quantity": p.inventory_quantity,
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
                    "variants": product_detail.variants.into_iter().map(|v| serde_json::json!({
                        "id": v.id,
                        "title": v.title,
                        "sku": v.sku,
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
        Ok(None) => Json(serde_json::json!({
            "error": "Product not found"
        })),
        Err(e) => {
            tracing::error!("Failed to get product: {}", e);
            Json(serde_json::json!({
                "error": "Failed to retrieve product"
            }))
        }
    }
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
                Ok(products) => products.into_iter().map(|p| serde_json::json!({
                    "id": p.id,
                    "title": p.title,
                    "slug": p.slug,
                    "price": p.price,
                    "currency": p.currency,
                    "is_active": p.is_active,
                })).collect::<Vec<_>>(),
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
