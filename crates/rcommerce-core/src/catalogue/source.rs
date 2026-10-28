//! Where a store copies its catalogue from: the change log and the catalogue
//! rows. `PgCatalogueSource` reads a catalogue database directly; the trait
//! leaves room for other transports (an HTTP API) later.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;

/// The products a store takes from the catalogue
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assortment {
    /// The whole catalogue
    #[serde(default)]
    pub all: bool,
    /// Categories, with their sub-categories (catalogue ids)
    #[serde(default)]
    pub categories: Vec<Uuid>,
    /// Single products (catalogue ids)
    #[serde(default)]
    pub products: Vec<Uuid>,
}

impl Assortment {
    /// Whether the store takes nothing
    pub fn is_empty(&self) -> bool {
        !self.all && self.categories.is_empty() && self.products.is_empty()
    }
}

/// One entry of the catalogue's change log
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CatalogueChange {
    pub seq: i64,
    /// "product" or "category"
    pub entity: String,
    pub entity_id: Uuid,
    pub deleted: bool,
    pub changed_at: DateTime<Utc>,
}

/// A catalogue category
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SourceCategory {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub image_url: Option<String>,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub sort_order: i32,
    pub is_active: bool,
}

/// A catalogue product row (enums as text)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SourceProduct {
    pub id: Uuid,
    pub title: String,
    pub slug: String,
    pub description: Option<String>,
    pub sku: Option<String>,
    pub product_type: String,
    pub price: Decimal,
    pub compare_at_price: Option<Decimal>,
    pub currency: String,
    pub weight: Option<Decimal>,
    pub weight_unit: Option<String>,
    pub requires_shipping: bool,
    pub is_active: bool,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub gtin: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
}

/// A catalogue variant row
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SourceVariant {
    pub id: Uuid,
    pub product_id: Uuid,
    pub title: String,
    pub sku: Option<String>,
    pub price: Decimal,
    pub compare_at_price: Option<Decimal>,
    pub currency: String,
    pub weight: Option<Decimal>,
    pub weight_unit: Option<String>,
    pub requires_shipping: bool,
    pub is_active: bool,
    pub gtin: Option<String>,
}

/// A catalogue image row
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SourceImage {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub position: i32,
    pub src: String,
    pub alt_text: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// A catalogue product option (e.g. "Size")
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SourceOption {
    pub id: Uuid,
    pub product_id: Uuid,
    pub name: String,
    pub position: i32,
}

/// The value of an option for one variant
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SourceOptionValue {
    pub id: Uuid,
    pub option_id: Uuid,
    pub variant_id: Uuid,
    pub value: String,
}

/// A product with everything copied along with it
#[derive(Debug, Clone)]
pub struct ProductBundle {
    pub product: SourceProduct,
    pub variants: Vec<SourceVariant>,
    pub images: Vec<SourceImage>,
    pub options: Vec<SourceOption>,
    pub option_values: Vec<SourceOptionValue>,
    /// (category id, is primary)
    pub categories: Vec<(Uuid, bool)>,
}

/// Where a store copies its catalogue from
// async_trait marks the boxed futures it generates #[must_use], which they already are
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CatalogueSource: Send + Sync {
    /// Highest sequence in the change log (0 when it is empty)
    async fn last_seq(&self) -> Result<i64>;

    /// Lowest sequence still in the change log (0 when it is empty); a store
    /// behind it missed pruned changes and copies its assortment again
    async fn first_seq(&self) -> Result<i64>;

    /// Changes after `seq`, oldest first, at most `limit`
    async fn changes_since(&self, seq: i64, limit: i64) -> Result<Vec<CatalogueChange>>;

    /// Every category
    async fn categories(&self) -> Result<Vec<SourceCategory>>;

    /// The products in `assortment`; with `among`, only those of these ids
    async fn assortment_ids(
        &self,
        assortment: &Assortment,
        among: Option<&[Uuid]>,
    ) -> Result<Vec<Uuid>>;

    /// These products with their variants, images, options and category links
    async fn products(&self, ids: &[Uuid]) -> Result<Vec<ProductBundle>>;
}

/// A catalogue database read over its own connection pool
pub struct PgCatalogueSource {
    pool: PgPool,
}

impl PgCatalogueSource {
    /// A source reading from this catalogue database
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CatalogueSource for PgCatalogueSource {
    async fn last_seq(&self) -> Result<i64> {
        let seq: Option<i64> = sqlx::query_scalar("SELECT MAX(seq) FROM catalogue_changes")
            .fetch_one(&self.pool)
            .await?;
        Ok(seq.unwrap_or(0))
    }

    async fn first_seq(&self) -> Result<i64> {
        let seq: Option<i64> = sqlx::query_scalar("SELECT MIN(seq) FROM catalogue_changes")
            .fetch_one(&self.pool)
            .await?;
        Ok(seq.unwrap_or(0))
    }

    async fn changes_since(&self, seq: i64, limit: i64) -> Result<Vec<CatalogueChange>> {
        Ok(sqlx::query_as(
            "SELECT seq, entity, entity_id, deleted, changed_at FROM catalogue_changes \
             WHERE seq > $1 ORDER BY seq LIMIT $2",
        )
        .bind(seq)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn categories(&self) -> Result<Vec<SourceCategory>> {
        Ok(sqlx::query_as(
            "SELECT id, name, slug, description, parent_id, image_url, seo_title, \
             seo_description, sort_order, is_active FROM product_categories",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn assortment_ids(
        &self,
        assortment: &Assortment,
        among: Option<&[Uuid]>,
    ) -> Result<Vec<Uuid>> {
        Ok(sqlx::query_scalar(
            r#"
            WITH RECURSIVE cats AS (
                SELECT id FROM product_categories WHERE id = ANY($2)
                UNION
                SELECT c.id FROM product_categories c JOIN cats ON c.parent_id = cats.id
            )
            SELECT p.id FROM products p
            WHERE ($1 OR p.id = ANY($3) OR EXISTS (
                    SELECT 1 FROM product_category_relations r
                    JOIN cats ON cats.id = r.category_id
                    WHERE r.product_id = p.id))
              AND ($4::uuid[] IS NULL OR p.id = ANY($4))
            "#,
        )
        .bind(assortment.all)
        .bind(&assortment.categories)
        .bind(&assortment.products)
        .bind(among)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn products(&self, ids: &[Uuid]) -> Result<Vec<ProductBundle>> {
        let (products, variants, images, options, values, links) = tokio::try_join!(
            self.product_rows(ids),
            self.variant_rows(ids),
            self.image_rows(ids),
            self.option_rows(ids),
            self.option_value_rows(ids),
            self.category_links(ids),
        )?;
        Ok(bundle(products, variants, images, options, values, links))
    }
}

impl PgCatalogueSource {
    async fn product_rows(&self, ids: &[Uuid]) -> Result<Vec<SourceProduct>> {
        Ok(sqlx::query_as(
            "SELECT id, title, slug, description, sku, product_type::text AS product_type, \
             price, compare_at_price, currency::text AS currency, weight, \
             weight_unit::text AS weight_unit, requires_shipping, is_active, seo_title, \
             seo_description, gtin, published_at FROM products WHERE id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn variant_rows(&self, ids: &[Uuid]) -> Result<Vec<SourceVariant>> {
        Ok(sqlx::query_as(
            "SELECT id, product_id, title, sku, price, compare_at_price, \
             currency::text AS currency, weight, weight_unit::text AS weight_unit, \
             requires_shipping, is_active, gtin FROM product_variants WHERE product_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn image_rows(&self, ids: &[Uuid]) -> Result<Vec<SourceImage>> {
        Ok(sqlx::query_as(
            "SELECT id, product_id, variant_id, position, src, alt_text, width, height \
             FROM product_images WHERE product_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn option_rows(&self, ids: &[Uuid]) -> Result<Vec<SourceOption>> {
        Ok(sqlx::query_as(
            "SELECT id, product_id, name, position FROM product_options WHERE product_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn option_value_rows(&self, ids: &[Uuid]) -> Result<Vec<SourceOptionValue>> {
        Ok(sqlx::query_as(
            "SELECT v.id, v.option_id, v.variant_id, v.value FROM product_option_values v \
             JOIN product_options o ON o.id = v.option_id WHERE o.product_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn category_links(&self, ids: &[Uuid]) -> Result<Vec<(Uuid, Uuid, bool)>> {
        Ok(sqlx::query_as(
            "SELECT product_id, category_id, is_primary FROM product_category_relations \
             WHERE product_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?)
    }
}

/// Group the child rows under their product
fn bundle(
    products: Vec<SourceProduct>,
    variants: Vec<SourceVariant>,
    images: Vec<SourceImage>,
    options: Vec<SourceOption>,
    values: Vec<SourceOptionValue>,
    links: Vec<(Uuid, Uuid, bool)>,
) -> Vec<ProductBundle> {
    let option_product: HashMap<Uuid, Uuid> =
        options.iter().map(|o| (o.id, o.product_id)).collect();
    let mut variants = group(variants, |v| Some(v.product_id));
    let mut images = group(images, |i| Some(i.product_id));
    let mut values = group(values, |v| option_product.get(&v.option_id).copied());
    let mut options = group(options, |o| Some(o.product_id));
    let mut links = group(links, |l| Some(l.0));
    products
        .into_iter()
        .map(|product| {
            let id = product.id;
            ProductBundle {
                product,
                variants: variants.remove(&id).unwrap_or_default(),
                images: images.remove(&id).unwrap_or_default(),
                options: options.remove(&id).unwrap_or_default(),
                option_values: values.remove(&id).unwrap_or_default(),
                categories: links
                    .remove(&id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(_, category, primary)| (category, primary))
                    .collect(),
            }
        })
        .collect()
}

/// Rows by the product they belong to (rows without one are dropped)
fn group<T>(rows: Vec<T>, product: impl Fn(&T) -> Option<Uuid>) -> HashMap<Uuid, Vec<T>> {
    let mut by_product: HashMap<Uuid, Vec<T>> = HashMap::new();
    for row in rows {
        if let Some(id) = product(&row) {
            by_product.entry(id).or_default().push(row);
        }
    }
    by_product
}
