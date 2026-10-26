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
use std::collections::HashMap;

use rcommerce_core::common::validation::validate_slug;
use rcommerce_core::models::{
    CreateProductRequest, Product, ProductCategory, UpdateProductRequest,
};
use rcommerce_core::repository::{
    normalize_locale, CatalogI18nRepository, CategoryRepository, CategoryTranslation,
    PostgresCategoryRepository, ProductTranslation,
};
use rcommerce_core::services::product_service::ProductList;
use rcommerce_core::services::{PaginationInfo, PaginationParams};

/// `?lang=` on storefront routes. The language is part of the URL (not
/// Accept-Language) so CDN and browser caches key on it without `Vary`.
#[derive(Debug, Deserialize, Default)]
pub struct LangParams {
    lang: Option<String>,
}

/// The translation locale for `?lang=` ("fr-BE" -> "fr"); None = base text
fn requested_locale(lang: Option<&str>) -> Option<String> {
    lang.and_then(normalize_locale)
}

/// List products, optionally in one category (`?category=`, base or translated
/// slug) and/or matching a full-text search (`?q=`, best match first), with
/// text in `?lang=` where translated
pub async fn list_products(
    State(state): State<AppState>,
    Query(params): Query<ListProductsParams>,
) -> Json<serde_json::Value> {
    let locale = requested_locale(params.lang.as_deref());
    let i18n = CatalogI18nRepository::new(state.db.pool().clone());
    let category_filter = if let Some(category_slug) = params.category.as_deref() {
        match i18n
            .category_id_by_slug(category_slug, locale.as_deref())
            .await
        {
            Ok(Some(id)) => Some(id),
            Ok(None) => return Json(empty_product_list()),
            Err(e) => {
                tracing::error!("Failed to lookup category {}: {}", category_slug, e);
                return Json(empty_product_list());
            }
        }
    } else {
        None
    };

    let page = product_page(&state, &i18n, &params, locale.as_deref(), category_filter);
    let product_list = match page.await {
        Ok(list) => list,
        Err(e) => {
            tracing::error!("Failed to list products: {}", e);
            return Json(empty_product_list());
        }
    };

    let cards = ProductCards::load(&state, &product_list.products, locale.as_deref()).await;
    let products = cards.into_cards(product_list.products);
    Json(product_list_json(products, &product_list.pagination))
}

/// The requested page: search hits in relevance order with `?q=`, otherwise
/// the active products (of the category, if any)
async fn product_page(
    state: &AppState,
    i18n: &CatalogI18nRepository,
    params: &ListProductsParams,
    locale: Option<&str>,
    category_id: Option<Uuid>,
) -> rcommerce_core::Result<ProductList> {
    let pagination = PaginationParams {
        page: params.page.filter(|p| *p >= 1).unwrap_or(1),
        per_page: params.per_page.unwrap_or(20).clamp(1, 100),
    };
    match params.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(query) => search_page(state, i18n, query, locale, category_id, &pagination).await,
        None => {
            let filter = rcommerce_core::models::ProductFilter {
                status: Some(rcommerce_core::models::ProductStatus::Active),
                category_id,
                ..Default::default()
            };
            state
                .product_service
                .list_products(Some(filter), pagination)
                .await
        }
    }
}

/// Response of a product listing
fn product_list_json(
    products: Vec<serde_json::Value>,
    pagination: &PaginationInfo,
) -> serde_json::Value {
    serde_json::json!({
        "products": products,
        "meta": {
            "total": pagination.total,
            "page": pagination.page,
            "per_page": pagination.per_page,
            "total_pages": pagination.total_pages
        }
    })
}

/// Response of a listing that matches nothing (or failed)
fn empty_product_list() -> serde_json::Value {
    let pagination = PaginationInfo {
        page: 1,
        per_page: 20,
        total: 0,
        total_pages: 0,
    };
    product_list_json(Vec::new(), &pagination)
}

/// Card data for one page of products. First images, categories, review
/// aggregates and bundle stock are one query each for the whole page, run
/// concurrently (the images and categories used to cost two queries per
/// product), as are the translations in the requested language. A failing
/// lookup leaves its fields empty, as before.
struct ProductCards {
    images: HashMap<Uuid, String>,
    categories: HashMap<Uuid, Vec<serde_json::Value>>,
    reviews: HashMap<Uuid, (f64, i64)>,
    bundle_stock: HashMap<Uuid, i32>,
    translations: HashMap<Uuid, ProductTranslation>,
}

impl ProductCards {
    async fn load(state: &AppState, products: &[Product], locale: Option<&str>) -> Self {
        let ids: Vec<Uuid> = products.iter().map(|p| p.id).collect();
        let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
        let i18n = CatalogI18nRepository::new(state.db.pool().clone());
        let translations = async {
            match locale {
                Some(locale) => i18n.product_translations(&ids, locale).await,
                None => Ok(HashMap::new()),
            }
        };
        let (images, categories, reviews, bundle_stock, translations) = tokio::join!(
            state.product_service.repository.find_first_images(&ids),
            cat_repo.categories_for_products(&ids, locale),
            review_aggregates(state, &ids),
            bundle_stock(state, products),
            translations,
        );
        Self {
            images: or_empty(images, "product images"),
            categories: category_refs(or_empty(categories, "product categories")),
            reviews: or_empty(reviews, "review aggregates"),
            bundle_stock: or_empty(bundle_stock, "bundle stock"),
            translations: or_empty(translations, "product translations"),
        }
    }

    /// The listing cards of a page of products, in order
    fn into_cards(mut self, products: Vec<Product>) -> Vec<serde_json::Value> {
        products.into_iter().map(|p| self.card(p)).collect()
    }

    /// The listing card of one product, translated when a translation was loaded
    fn card(&mut self, p: Product) -> serde_json::Value {
        let categories = self.categories.remove(&p.id).unwrap_or_default();
        let (average_rating, review_count) = self.reviews.get(&p.id).copied().unwrap_or((0.0, 0));
        let t = self.translations.remove(&p.id);
        serde_json::json!({
            "id": p.id,
            "title": t.as_ref().map_or(&p.title, |t| &t.title),
            "slug": t.as_ref().map_or(&p.slug, |t| &t.slug),
            "sku": p.sku,
            "price": p.price,
            "currency": p.currency,
            "description": t.as_ref().map_or(p.description.as_ref(), |t| t.description.as_ref()),
            "seo_description": t.as_ref().map_or(p.seo_description.as_ref(), |t| t.seo_description.as_ref()),
            "locale": t.as_ref().map(|t| &t.locale),
            "is_active": p.is_active,
            "product_type": format!("{:?}", p.product_type).to_lowercase(),
            "inventory_quantity": self.bundle_stock.get(&p.id).copied().unwrap_or(p.inventory_quantity),
            "image": self.images.get(&p.id),
            "categories": categories,
            "average_rating": average_rating,
            "review_count": review_count,
            "created_at": p.created_at
        })
    }
}

/// Stock of the bundles among the products, derived from their required
/// components
async fn bundle_stock(
    state: &AppState,
    products: &[Product],
) -> rcommerce_core::Result<HashMap<Uuid, i32>> {
    let bundle_ids: Vec<Uuid> = products
        .iter()
        .filter(|p| matches!(p.product_type, rcommerce_core::models::ProductType::Bundle))
        .map(|p| p.id)
        .collect();
    state.bundle_service.available_stock_many(&bundle_ids).await
}

/// Each product's categories as `{id, name, slug}` references
fn category_refs(
    categories: HashMap<Uuid, Vec<ProductCategory>>,
) -> HashMap<Uuid, Vec<serde_json::Value>> {
    categories
        .into_iter()
        .map(|(id, cats)| {
            let cats = cats
                .into_iter()
                .map(|c| serde_json::json!({"id": c.id, "name": c.name, "slug": c.slug}))
                .collect();
            (id, cats)
        })
        .collect()
}

/// Approved-review average and count per product, in one query
async fn review_aggregates(
    state: &AppState,
    product_ids: &[Uuid],
) -> Result<HashMap<Uuid, (f64, i64)>, sqlx::Error> {
    if product_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, (Uuid, Option<f64>, i64)>(
        r#"
        SELECT product_id, AVG(rating)::float8, COUNT(*)::int8
        FROM product_reviews
        WHERE product_id = ANY($1) AND status = 'approved'
        GROUP BY product_id
        "#,
    )
    .bind(product_ids)
    .fetch_all(state.db.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|(product_id, avg, count)| (product_id, (avg.unwrap_or(0.0), count)))
        .collect())
}

/// The result of a card lookup, or nothing (logged) when it failed
fn or_empty<T: Default, E: std::fmt::Display>(result: Result<T, E>, what: &str) -> T {
    result.unwrap_or_else(|e| {
        tracing::error!("Failed to fetch {} for list: {}", what, e);
        T::default()
    })
}

/// One page of full-text search hits, in relevance order
async fn search_page(
    state: &AppState,
    i18n: &CatalogI18nRepository,
    query: &str,
    locale: Option<&str>,
    category_id: Option<Uuid>,
    pagination: &PaginationParams,
) -> rcommerce_core::Result<ProductList> {
    let offset = (pagination.page - 1) * pagination.per_page;
    let limit = pagination.per_page;
    let hits = match &state.catalog_search {
        Some(search) => match search
            .search(query, locale, category_id, limit, offset)
            .await
        {
            Ok(hits) => hits,
            Err(e) => {
                tracing::warn!("Elasticsearch search failed, using PostgreSQL: {}", e);
                i18n.search_products(query, locale, category_id, limit, offset)
                    .await?
            }
        },
        None => {
            i18n.search_products(query, locale, category_id, limit, offset)
                .await?
        }
    };
    let mut products = state
        .product_service
        .repository
        .find_by_ids(&hits.ids)
        .await?;
    let rank: HashMap<Uuid, usize> = hits
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();
    products.sort_by_key(|p| rank.get(&p.id).copied().unwrap_or(usize::MAX));
    Ok(ProductList {
        products,
        pagination: PaginationInfo {
            page: pagination.page,
            per_page: pagination.per_page,
            total: hits.total,
            total_pages: (hits.total + pagination.per_page - 1) / pagination.per_page,
        },
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct ListProductsParams {
    /// Category slug (base or translated)
    category: Option<String>,
    /// Full-text search: every word must match, as the prefix of a title word or as
    /// a whole word in the title or description
    q: Option<String>,
    /// 1-based page number (default 1)
    page: Option<i64>,
    /// Page size, clamped to 1..=100 (default 20)
    per_page: Option<i64>,
    /// Language of the returned text, e.g. "fr" (falls back to the base text)
    lang: Option<String>,
}

/// Whether a product can be shown publicly: inactive products are drafts,
/// visible to admins only
fn product_visible(is_active: bool, auth: Option<&JwtAuth>) -> bool {
    is_active || auth.is_some_and(|a| a.is_admin())
}

/// Get a product by ID or slug (base or translated; 404 when missing or a
/// draft). `?lang=` picks the text; `alternates` lists the slug in every
/// translated language (hreflang links).
pub async fn get_product(
    State(state): State<AppState>,
    // Set by optional_auth_middleware; absent (anonymous) when a router mounts
    // this route without it
    auth: Option<Extension<Option<JwtAuth>>>,
    Path(id): Path<String>,
    Query(params): Query<LangParams>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let auth = auth.and_then(|Extension(auth)| auth);
    let locale = requested_locale(params.lang.as_deref());
    let i18n = CatalogI18nRepository::new(state.db.pool().clone());

    // The product row and its translations in one query (by UUID, else by slug)
    let not_found = || rcommerce_core::Error::not_found("Product not found");
    let found = i18n
        .product_with_translations(Uuid::parse_str(&id).ok(), &id, locale.as_deref())
        .await;
    let (p, translations) = match found {
        Ok(Some(found)) => found,
        Ok(None) => return Err(not_found()),
        Err(e) => {
            tracing::error!("Failed to get product {}: {}", id, e);
            return Err(e);
        }
    };
    if !product_visible(p.is_active, auth.as_ref()) {
        return Err(not_found());
    }

    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
    let product_ids = [p.id];
    let (related, categories) = tokio::join!(
        state.product_service.load_related(p.id),
        cat_repo.categories_for_products(&product_ids, locale.as_deref()),
    );
    let (variants, images, options) = related?;

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
    let categories: Vec<serde_json::Value> = categories
        .unwrap_or_default()
        .remove(&p.id)
        .unwrap_or_default()
        .into_iter()
        .map(|c| serde_json::json!({"id": c.id, "name": c.name, "slug": c.slug}))
        .collect();
    let t = locale
        .as_deref()
        .and_then(|locale| translations.iter().find(|t| t.locale == locale));
    let alternates: Vec<serde_json::Value> = translations
        .iter()
        .map(|t| serde_json::json!({"locale": t.locale, "slug": t.slug}))
        .collect();

    Ok(Json(serde_json::json!({
    "product": {
        "id": p.id,
        "title": t.map_or(&p.title, |t| &t.title),
        "slug": t.map_or(&p.slug, |t| &t.slug),
        "description": t.map_or(p.description.as_ref(), |t| t.description.as_ref()),
        "locale": t.map(|t| &t.locale),
        "alternates": alternates,
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
                "seo_title": t.map_or(p.seo_title.as_ref(), |t| t.seo_title.as_ref()),
                "seo_description": t.map_or(p.seo_description.as_ref(), |t| t.seo_description.as_ref()),
                "created_at": p.created_at,
                "updated_at": p.updated_at,
                "published_at": p.published_at,
                "categories": categories,
                "variants": variants.into_iter().map(|v| serde_json::json!({
                    "id": v.id,
                    "title": v.title,
                    "sku": v.sku,
                    "gtin": v.gtin,
                    "price": v.price,
                    "inventory_quantity": v.inventory_quantity
                })).collect::<Vec<_>>(),
                "images": images.into_iter().map(|i| serde_json::json!({
                    "id": i.id,
                    "src": i.src,
                    "alt_text": i.alt_text
                })).collect::<Vec<_>>(),
                "options": options.into_iter().map(|(opt, values)| serde_json::json!({
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
        })))
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
    let pack_stock: HashMap<Uuid, i32> = if pack_ids.is_empty() {
        HashMap::new()
    } else {
        state
            .bundle_service
            .available_stock_many(&pack_ids)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to derive pack stock: {}", e);
                HashMap::new()
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
    reindex_in_background(&state, vec![product.id]);

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
    reindex_in_background(&state, vec![id]);

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
    reindex_in_background(&state, vec![id]);

    Ok(StatusCode::NO_CONTENT)
}

/// Re-index products in the search engine after an admin change (no-op
/// without Elasticsearch). Runs in the background: a slow or unavailable search
/// engine must not fail the admin request.
fn reindex_in_background(state: &AppState, ids: Vec<Uuid>) {
    if let Some(search) = state.catalog_search.clone() {
        tokio::spawn(async move {
            if let Err(e) = search.index_products(&ids).await {
                tracing::error!("Search index update failed for {:?}: {}", ids, e);
            }
        });
    }
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

/// List all active product categories (`?lang=` for translated names and slugs)
pub async fn list_categories(
    State(state): State<AppState>,
    Query(params): Query<LangParams>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let locale = requested_locale(params.lang.as_deref());
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
    let categories = cat_repo.list(1000, 0).await?;
    let mut translations = match locale.as_deref() {
        Some(locale) => {
            let ids: Vec<Uuid> = categories.iter().map(|c| c.id).collect();
            CatalogI18nRepository::new(state.db.pool().clone())
                .category_translations(&ids, locale)
                .await?
        }
        None => HashMap::new(),
    };
    Ok(Json(serde_json::json!({
        "categories": categories.into_iter().map(|c| {
            let t = translations.remove(&c.id);
            serde_json::json!({
                "id": c.id,
                "name": t.as_ref().map_or(&c.name, |t| &t.name),
                "slug": t.as_ref().map_or(&c.slug, |t| &t.slug),
                "description": t.as_ref().map_or(c.description.as_ref(), |t| t.description.as_ref()),
                "locale": t.as_ref().map(|t| &t.locale),
            })
        }).collect::<Vec<_>>()
    })))
}

/// Get a single category by slug (base or translated) and include its products
pub async fn get_category(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<LangParams>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let locale = requested_locale(params.lang.as_deref());
    let i18n = CatalogI18nRepository::new(state.db.pool().clone());
    let cat_repo = PostgresCategoryRepository::new(state.db.pool().clone());
    let not_found = || rcommerce_core::Error::not_found("Category not found");

    let category_id = i18n
        .category_id_by_slug(&slug, locale.as_deref())
        .await?
        .ok_or_else(not_found)?;
    let category = cat_repo
        .get_by_id(category_id)
        .await?
        .ok_or_else(not_found)?;
    let products = cat_repo.get_products(category.id, 100, 0).await?;

    let ids: Vec<Uuid> = products.iter().map(|p| p.id).collect();
    let category_ids = [category.id];
    let (mut category_t, product_t) = match locale.as_deref() {
        Some(locale) => tokio::try_join!(
            i18n.category_translations(&category_ids, locale),
            i18n.product_translations(&ids, locale),
        )?,
        None => (HashMap::new(), HashMap::new()),
    };
    let t = category_t.remove(&category.id);
    let products = category_products_json(products, product_t);

    Ok(Json(serde_json::json!({
        "category": {
            "id": category.id,
            "name": t.as_ref().map_or(&category.name, |t| &t.name),
            "slug": t.as_ref().map_or(&category.slug, |t| &t.slug),
            "description": t.as_ref().map_or(category.description.as_ref(), |t| t.description.as_ref()),
            "locale": t.as_ref().map(|t| &t.locale),
        },
        "products": products,
    })))
}

/// The products of a category page, translated where a translation was loaded
fn category_products_json(
    products: Vec<Product>,
    mut translations: HashMap<Uuid, ProductTranslation>,
) -> Vec<serde_json::Value> {
    products
        .into_iter()
        .map(|p| {
            let pt = translations.remove(&p.id);
            serde_json::json!({
                "id": p.id,
                "title": pt.as_ref().map_or(&p.title, |t| &t.title),
                "slug": pt.as_ref().map_or(&p.slug, |t| &t.slug),
                "price": p.price,
                "currency": p.currency,
                "is_active": p.is_active,
            })
        })
        .collect()
}

/// Body of PUT .../translations/:locale (product or category)
#[derive(Debug, Deserialize)]
pub struct TranslationInput {
    /// Product title or category name
    #[serde(alias = "name")]
    pub title: String,
    pub slug: String,
    pub description: Option<String>,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
}

impl TranslationInput {
    fn validate(&self) -> Result<(), rcommerce_core::Error> {
        let title = self.title.trim();
        if title.is_empty() || title.chars().count() > 255 {
            return Err(rcommerce_core::Error::validation(
                "Title must be 1-255 characters",
            ));
        }
        if !validate_slug(&self.slug) {
            return Err(rcommerce_core::Error::validation(
                "Slug must be lowercase letters, digits and single hyphens (max 255)",
            ));
        }
        if self
            .seo_title
            .as_deref()
            .is_some_and(|s| s.chars().count() > 255)
        {
            return Err(rcommerce_core::Error::validation(
                "SEO title must be at most 255 characters",
            ));
        }
        Ok(())
    }
}

fn path_locale(locale: &str) -> Result<String, rcommerce_core::Error> {
    normalize_locale(locale).ok_or_else(|| {
        rcommerce_core::Error::validation("Locale must be a 2-3 letter language code")
    })
}

fn require_admin(auth: &JwtAuth) -> Result<(), rcommerce_core::Error> {
    if auth.is_admin() {
        Ok(())
    } else {
        Err(rcommerce_core::Error::unauthorized("Admin access required"))
    }
}

/// List a product's translations (admin)
pub async fn list_product_translations(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    require_admin(&auth)?;
    let translations = CatalogI18nRepository::new(state.db.pool().clone())
        .all_product_translations(id)
        .await?;
    Ok(Json(serde_json::json!({ "translations": translations })))
}

/// Create or replace a product translation (admin)
pub async fn put_product_translation(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path((id, locale)): Path<(Uuid, String)>,
    Json(input): Json<TranslationInput>,
) -> Result<Json<ProductTranslation>, rcommerce_core::Error> {
    require_admin(&auth)?;
    input.validate()?;
    let translation = ProductTranslation {
        product_id: id,
        locale: path_locale(&locale)?,
        title: input.title.trim().to_string(),
        slug: input.slug,
        description: input.description,
        seo_title: input.seo_title,
        seo_description: input.seo_description,
    };
    let saved = CatalogI18nRepository::new(state.db.pool().clone())
        .upsert_product_translation(&translation)
        .await?;
    reindex_in_background(&state, vec![id]);
    Ok(Json(saved))
}

/// Delete a product translation (admin)
pub async fn delete_product_translation(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path((id, locale)): Path<(Uuid, String)>,
) -> Result<StatusCode, rcommerce_core::Error> {
    require_admin(&auth)?;
    let deleted = CatalogI18nRepository::new(state.db.pool().clone())
        .delete_product_translation(id, &path_locale(&locale)?)
        .await?;
    reindex_in_background(&state, vec![id]);
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(rcommerce_core::Error::not_found("Translation not found"))
    }
}

/// Create or replace a category translation (admin)
pub async fn put_category_translation(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path((id, locale)): Path<(Uuid, String)>,
    Json(input): Json<TranslationInput>,
) -> Result<Json<CategoryTranslation>, rcommerce_core::Error> {
    require_admin(&auth)?;
    input.validate()?;
    let translation = CategoryTranslation {
        category_id: id,
        locale: path_locale(&locale)?,
        name: input.title.trim().to_string(),
        slug: input.slug,
        description: input.description,
        seo_title: input.seo_title,
        seo_description: input.seo_description,
    };
    let saved = CatalogI18nRepository::new(state.db.pool().clone())
        .upsert_category_translation(&translation)
        .await?;
    Ok(Json(saved))
}

/// Delete a category translation (admin)
pub async fn delete_category_translation(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path((id, locale)): Path<(Uuid, String)>,
) -> Result<StatusCode, rcommerce_core::Error> {
    require_admin(&auth)?;
    let deleted = CatalogI18nRepository::new(state.db.pool().clone())
        .delete_category_translation(id, &path_locale(&locale)?)
        .await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(rcommerce_core::Error::not_found("Translation not found"))
    }
}

/// Router for admin product routes (requires admin middleware to be applied by caller)
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/products", post(create_product))
        .route("/products/:id", put(update_product).delete(delete_product))
        .route("/products/:id/translations", get(list_product_translations))
        .route(
            "/products/:id/translations/:locale",
            put(put_product_translation).delete(delete_product_translation),
        )
        .route(
            "/categories/:id/translations/:locale",
            put(put_category_translation).delete(delete_category_translation),
        )
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
    fn drafts_are_visible_to_admins_only() {
        let customer = JwtAuth {
            customer_id: uuid::Uuid::new_v4(),
            email: "c@example.com".to_string(),
            permissions: vec!["read".to_string()],
        };
        let admin = JwtAuth {
            permissions: vec!["read".to_string(), "write".to_string(), "admin".to_string()],
            ..customer.clone()
        };
        assert!(product_visible(true, None));
        assert!(product_visible(true, Some(&customer)));
        assert!(!product_visible(false, None));
        assert!(!product_visible(false, Some(&customer)));
        assert!(product_visible(false, Some(&admin)));
    }

    #[test]
    fn requested_locale_uses_the_language_subtag() {
        assert_eq!(requested_locale(Some("fr-BE")).as_deref(), Some("fr"));
        assert_eq!(requested_locale(Some("not a locale")), None);
        assert_eq!(requested_locale(None), None);
    }

    #[test]
    fn pack_option_label_fallback() {
        assert_eq!(pack_option_label("Big Mitty", "Big Mitty"), "Pack");
        assert_eq!(pack_option_label("McFly -", "McFly"), "Pack");
    }
}
