//! The store side: copies the store's assortment from a catalogue into the
//! store's own database (see the module documentation).

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::source::{
    Assortment, CatalogueChange, CatalogueSource, ProductBundle, SourceCategory, SourceVariant,
};
use crate::{Error, Result};

/// Change-log entries read per query
const PAGE: i64 = 1000;
/// Products copied per transaction
const BATCH: usize = 100;
/// Changes younger than this are applied again on the next run: a slower
/// catalogue transaction may still commit a lower sequence number
const OVERLAP_SECONDS: i64 = 300;
/// Advisory lock key: one sync at a time per store database
const LOCK_KEY: i64 = 0x7263_6174_7379_6e63;

type Tx<'a> = Transaction<'a, Postgres>;

/// What a sync run did
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    /// Whole assortment copied (first run, new assortment, or asked for)
    pub full: bool,
    pub categories_copied: usize,
    pub products_copied: usize,
    pub products_deactivated: usize,
    /// Change-log position the store has caught up with
    pub last_seq: i64,
}

/// A store's sync state and assortment
#[derive(Debug, Clone, Serialize)]
pub struct CatalogueStatus {
    pub last_seq: i64,
    pub needs_full_sync: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// Products copied from the catalogue (active or not)
    pub catalogue_products: i64,
    pub assortment: Assortment,
}

/// Copies a store's assortment from a catalogue into the store's database
pub struct CatalogueSync {
    source: Arc<dyn CatalogueSource>,
    pool: PgPool,
}

impl CatalogueSync {
    /// A sync from `source` into the store database behind `pool`
    pub fn new(source: Arc<dyn CatalogueSource>, pool: PgPool) -> Self {
        Self { source, pool }
    }

    /// One sync: incremental from the change log, or full when the store never
    /// synced, its assortment changed, it fell behind the log, or `full` is set.
    /// Refused while another sync of the same store is running.
    pub async fn run(&self, full: bool) -> Result<SyncReport> {
        let mut lock = self.pool.acquire().await?;
        let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(LOCK_KEY)
            .fetch_one(&mut *lock)
            .await?;
        if !locked {
            return Err(Error::HttpError(
                http::StatusCode::CONFLICT,
                "A catalogue sync is already running".to_string(),
            ));
        }
        let result = self.run_locked(full).await;
        let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(LOCK_KEY)
            .execute(&mut *lock)
            .await;
        self.record_run(result.as_ref().err()).await?;
        result
    }

    async fn run_locked(&self, full: bool) -> Result<SyncReport> {
        let assortment = self.assortment().await?;
        let (last_seq, needs_full) = self.position().await?;
        let behind = self.source.first_seq().await? > last_seq + 1;
        if full || needs_full || behind {
            self.full_sync(&assortment).await
        } else {
            self.incremental(&assortment, last_seq).await
        }
    }

    async fn full_sync(&self, assortment: &Assortment) -> Result<SyncReport> {
        // Taken first: changes made while copying are applied on the next run
        let last_seq = self.source.last_seq().await?;
        let categories_copied = self.copy_categories().await?;
        let ids = if assortment.is_empty() {
            Vec::new()
        } else {
            self.source.assortment_ids(assortment, None).await?
        };
        let products_copied = self.copy_products(&ids).await?;
        let products_deactivated = self.deactivate_all_but(&ids).await?;
        self.save_position(last_seq).await?;
        Ok(SyncReport {
            full: true,
            categories_copied,
            products_copied,
            products_deactivated,
            last_seq,
        })
    }

    async fn incremental(&self, assortment: &Assortment, from: i64) -> Result<SyncReport> {
        let changes = self.changes_since(from).await?;
        let mut report = SyncReport {
            last_seq: settled_seq(&changes, from, Utc::now()),
            ..SyncReport::default()
        };
        if changes.iter().any(|c| c.entity == "category") {
            report.categories_copied = self.copy_categories().await?;
        }
        let changed = changed_products(&changes);
        if !changed.is_empty() {
            let keep = if assortment.is_empty() {
                Vec::new()
            } else {
                self.source
                    .assortment_ids(assortment, Some(&changed))
                    .await?
            };
            report.products_copied = self.copy_products(&keep).await?;
            let keep: HashSet<Uuid> = keep.into_iter().collect();
            let gone: Vec<Uuid> = changed
                .into_iter()
                .filter(|id| !keep.contains(id))
                .collect();
            report.products_deactivated = self.deactivate(&gone).await?;
        }
        self.save_position(report.last_seq).await?;
        Ok(report)
    }

    async fn changes_since(&self, from: i64) -> Result<Vec<CatalogueChange>> {
        let mut changes = Vec::new();
        let mut seq = from;
        loop {
            let page = self.source.changes_since(seq, PAGE).await?;
            let Some(last) = page.last() else { break };
            seq = last.seq;
            let done = (page.len() as i64) < PAGE;
            changes.extend(page);
            if done {
                break;
            }
        }
        Ok(changes)
    }
}

/// The log position a run may record: up to the last change older than the
/// overlap window. Younger changes are read again next time.
fn settled_seq(changes: &[CatalogueChange], from: i64, now: DateTime<Utc>) -> i64 {
    let cutoff = now - Duration::seconds(OVERLAP_SECONDS);
    changes
        .iter()
        .take_while(|c| c.changed_at <= cutoff)
        .map(|c| c.seq)
        .last()
        .unwrap_or(from)
}

/// The products the changes are about, each once
fn changed_products(changes: &[CatalogueChange]) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    changes
        .iter()
        .filter(|c| c.entity == "product" && seen.insert(c.entity_id))
        .map(|c| c.entity_id)
        .collect()
}

/// Copying rows
impl CatalogueSync {
    async fn copy_categories(&self) -> Result<usize> {
        let categories = self.source.categories().await?;
        let mut tx = self.pool.begin().await?;
        // Parents in a second pass: a child may come before its parent
        for c in &categories {
            upsert_category(&mut tx, c).await?;
        }
        for c in &categories {
            sqlx::query("UPDATE product_categories SET parent_id = $2 WHERE id = $1")
                .bind(c.id)
                .bind(c.parent_id)
                .execute(&mut *tx)
                .await?;
        }
        let ids: Vec<Uuid> = categories.iter().map(|c| c.id).collect();
        sqlx::query(
            "UPDATE product_categories SET is_active = false, updated_at = NOW() \
             WHERE catalogue_synced_at IS NOT NULL AND NOT (id = ANY($1))",
        )
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(categories.len())
    }

    async fn copy_products(&self, ids: &[Uuid]) -> Result<usize> {
        let mut copied = 0;
        for chunk in ids.chunks(BATCH) {
            let bundles = self.source.products(chunk).await?;
            let mut tx = self.pool.begin().await?;
            for bundle in &bundles {
                copy_product(&mut tx, bundle).await?;
            }
            tx.commit().await?;
            copied += bundles.len();
        }
        Ok(copied)
    }

    /// Deactivate catalogue products that are gone from the catalogue or the
    /// store's assortment (kept for order history)
    async fn deactivate(&self, ids: &[Uuid]) -> Result<usize> {
        let result = sqlx::query(
            "UPDATE products SET is_active = false, updated_at = NOW() \
             WHERE id = ANY($1) AND catalogue_synced_at IS NOT NULL AND is_active",
        )
        .bind(ids)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as usize)
    }

    async fn deactivate_all_but(&self, keep: &[Uuid]) -> Result<usize> {
        let result = sqlx::query(
            "UPDATE products SET is_active = false, updated_at = NOW() \
             WHERE catalogue_synced_at IS NOT NULL AND is_active AND NOT (id = ANY($1))",
        )
        .bind(keep)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as usize)
    }
}

/// The catalogue's slug, unless one of the store's own rows already uses it:
/// then the slug with the start of the id appended
async fn free_slug(tx: &mut Tx<'_>, table: &'static str, id: Uuid, slug: &str) -> Result<String> {
    let taken: bool = sqlx::query_scalar(&format!(
        "SELECT EXISTS (SELECT 1 FROM {table} WHERE slug = $1 AND id <> $2)"
    ))
    .bind(slug)
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(if taken {
        format!("{slug}-{}", &id.simple().to_string()[..8])
    } else {
        slug.to_string()
    })
}

async fn upsert_category(tx: &mut Tx<'_>, c: &SourceCategory) -> Result<()> {
    let slug = free_slug(tx, "product_categories", c.id, &c.slug).await?;
    sqlx::query(
        r#"
        INSERT INTO product_categories (id, name, slug, description, image_url, seo_title,
            seo_description, sort_order, is_active, catalogue_synced_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
        ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, slug = EXCLUDED.slug,
            description = EXCLUDED.description, image_url = EXCLUDED.image_url,
            seo_title = EXCLUDED.seo_title, seo_description = EXCLUDED.seo_description,
            sort_order = EXCLUDED.sort_order, is_active = EXCLUDED.is_active,
            catalogue_synced_at = NOW(), updated_at = NOW()
        "#,
    )
    .bind(c.id)
    .bind(&c.name)
    .bind(slug)
    .bind(&c.description)
    .bind(&c.image_url)
    .bind(&c.seo_title)
    .bind(&c.seo_description)
    .bind(c.sort_order)
    .bind(c.is_active)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// One product and everything that comes with it
async fn copy_product(tx: &mut Tx<'_>, bundle: &ProductBundle) -> Result<()> {
    upsert_product(tx, bundle).await?;
    copy_variants(tx, bundle).await?;
    copy_images(tx, bundle).await?;
    copy_options(tx, bundle).await?;
    copy_category_links(tx, bundle).await
}

/// The catalogue owns the description of a product; the store owns its price,
/// SKU, currency, stock and flags, which are only set when the product arrives
/// (inactive). A product the catalogue deactivates is deactivated in the store.
async fn upsert_product(tx: &mut Tx<'_>, bundle: &ProductBundle) -> Result<()> {
    let p = &bundle.product;
    let slug = free_slug(tx, "products", p.id, &p.slug).await?;
    sqlx::query(
        r#"
        INSERT INTO products (id, title, slug, description, sku, product_type, price,
            compare_at_price, currency, weight, weight_unit, requires_shipping, is_active,
            seo_title, seo_description, gtin, published_at, catalogue_synced_at)
        VALUES ($1, $2, $3, $4, $5, $6::product_type, $7, $8, $9::currency, $10,
            $11::weight_unit, $12, false, $13, $14, $15, $16, NOW())
        ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, slug = EXCLUDED.slug,
            description = EXCLUDED.description, product_type = EXCLUDED.product_type,
            weight = EXCLUDED.weight, weight_unit = EXCLUDED.weight_unit,
            requires_shipping = EXCLUDED.requires_shipping, seo_title = EXCLUDED.seo_title,
            seo_description = EXCLUDED.seo_description, gtin = EXCLUDED.gtin,
            published_at = EXCLUDED.published_at, is_active = products.is_active AND $17,
            catalogue_synced_at = NOW(), updated_at = NOW()
        "#,
    )
    .bind(p.id)
    .bind(&p.title)
    .bind(slug)
    .bind(&p.description)
    .bind(&p.sku)
    .bind(&p.product_type)
    .bind(p.price)
    .bind(p.compare_at_price)
    .bind(&p.currency)
    .bind(p.weight)
    .bind(&p.weight_unit)
    .bind(p.requires_shipping)
    .bind(&p.seo_title)
    .bind(&p.seo_description)
    .bind(&p.gtin)
    .bind(p.published_at)
    .bind(p.is_active)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Variants follow the same ownership as products. Variants removed from the
/// catalogue are deactivated: orders may refer to them.
async fn copy_variants(tx: &mut Tx<'_>, bundle: &ProductBundle) -> Result<()> {
    for v in &bundle.variants {
        upsert_variant(tx, v).await?;
    }
    let ids: Vec<Uuid> = bundle.variants.iter().map(|v| v.id).collect();
    sqlx::query(
        "UPDATE product_variants SET is_active = false, updated_at = NOW() \
         WHERE product_id = $1 AND NOT (id = ANY($2))",
    )
    .bind(bundle.product.id)
    .bind(&ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_variant(tx: &mut Tx<'_>, v: &SourceVariant) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO product_variants (id, product_id, title, sku, price, compare_at_price,
            currency, weight, weight_unit, requires_shipping, is_active, gtin)
        VALUES ($1, $2, $3, $4, $5, $6, $7::currency, $8, $9::weight_unit, $10, $11, $12)
        ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, weight = EXCLUDED.weight,
            weight_unit = EXCLUDED.weight_unit, requires_shipping = EXCLUDED.requires_shipping,
            gtin = EXCLUDED.gtin, is_active = product_variants.is_active AND EXCLUDED.is_active,
            updated_at = NOW()
        "#,
    )
    .bind(v.id)
    .bind(v.product_id)
    .bind(&v.title)
    .bind(&v.sku)
    .bind(v.price)
    .bind(v.compare_at_price)
    .bind(&v.currency)
    .bind(v.weight)
    .bind(&v.weight_unit)
    .bind(v.requires_shipping)
    .bind(v.is_active)
    .bind(&v.gtin)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Images are the catalogue's: the store gets exactly its set
async fn copy_images(tx: &mut Tx<'_>, bundle: &ProductBundle) -> Result<()> {
    let ids: Vec<Uuid> = bundle.images.iter().map(|i| i.id).collect();
    sqlx::query("DELETE FROM product_images WHERE product_id = $1 AND NOT (id = ANY($2))")
        .bind(bundle.product.id)
        .bind(&ids)
        .execute(&mut **tx)
        .await?;
    for i in &bundle.images {
        sqlx::query(
            r#"
            INSERT INTO product_images (id, product_id, variant_id, position, src, alt_text,
                width, height)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET variant_id = EXCLUDED.variant_id,
                position = EXCLUDED.position, src = EXCLUDED.src, alt_text = EXCLUDED.alt_text,
                width = EXCLUDED.width, height = EXCLUDED.height, updated_at = NOW()
            "#,
        )
        .bind(i.id)
        .bind(i.product_id)
        .bind(i.variant_id)
        .bind(i.position)
        .bind(&i.src)
        .bind(&i.alt_text)
        .bind(i.width)
        .bind(i.height)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Options and their values are the catalogue's: the store gets exactly its set
async fn copy_options(tx: &mut Tx<'_>, bundle: &ProductBundle) -> Result<()> {
    let option_ids: Vec<Uuid> = bundle.options.iter().map(|o| o.id).collect();
    sqlx::query("DELETE FROM product_options WHERE product_id = $1 AND NOT (id = ANY($2))")
        .bind(bundle.product.id)
        .bind(&option_ids)
        .execute(&mut **tx)
        .await?;
    for o in &bundle.options {
        sqlx::query(
            "INSERT INTO product_options (id, product_id, name, position) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, position = EXCLUDED.position, \
             updated_at = NOW()",
        )
        .bind(o.id)
        .bind(o.product_id)
        .bind(&o.name)
        .bind(o.position)
        .execute(&mut **tx)
        .await?;
    }
    copy_option_values(tx, bundle, &option_ids).await
}

async fn copy_option_values(
    tx: &mut Tx<'_>,
    bundle: &ProductBundle,
    option_ids: &[Uuid],
) -> Result<()> {
    let ids: Vec<Uuid> = bundle.option_values.iter().map(|v| v.id).collect();
    sqlx::query(
        "DELETE FROM product_option_values WHERE option_id = ANY($1) AND NOT (id = ANY($2))",
    )
    .bind(option_ids)
    .bind(&ids)
    .execute(&mut **tx)
    .await?;
    for v in &bundle.option_values {
        sqlx::query(
            "INSERT INTO product_option_values (id, option_id, variant_id, value) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (id) DO UPDATE SET option_id = EXCLUDED.option_id, \
             variant_id = EXCLUDED.variant_id, value = EXCLUDED.value, updated_at = NOW()",
        )
        .bind(v.id)
        .bind(v.option_id)
        .bind(v.variant_id)
        .bind(&v.value)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Links to catalogue categories follow the catalogue; links the store added
/// to its own categories stay
async fn copy_category_links(tx: &mut Tx<'_>, bundle: &ProductBundle) -> Result<()> {
    let category_ids: Vec<Uuid> = bundle.categories.iter().map(|(id, _)| *id).collect();
    sqlx::query(
        "DELETE FROM product_category_relations r USING product_categories c \
         WHERE r.product_id = $1 AND c.id = r.category_id \
           AND c.catalogue_synced_at IS NOT NULL AND NOT (r.category_id = ANY($2))",
    )
    .bind(bundle.product.id)
    .bind(&category_ids)
    .execute(&mut **tx)
    .await?;
    for (category_id, is_primary) in &bundle.categories {
        sqlx::query(
            "INSERT INTO product_category_relations (product_id, category_id, is_primary) \
             SELECT $1, $2, $3 WHERE EXISTS (SELECT 1 FROM product_categories WHERE id = $2) \
             ON CONFLICT (product_id, category_id) DO UPDATE SET is_primary = EXCLUDED.is_primary",
        )
        .bind(bundle.product.id)
        .bind(category_id)
        .bind(is_primary)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Sync state and assortment, in the store database
impl CatalogueSync {
    /// The products the store takes from the catalogue
    pub async fn assortment(&self) -> Result<Assortment> {
        let rules: Vec<(String, Option<Uuid>)> = sqlx::query_as(
            "SELECT kind, target_id FROM catalogue_subscriptions ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut assortment = Assortment::default();
        for (kind, target) in rules {
            match (kind.as_str(), target) {
                ("all", _) => assortment.all = true,
                ("category", Some(id)) => assortment.categories.push(id),
                ("product", Some(id)) => assortment.products.push(id),
                _ => {}
            }
        }
        Ok(assortment)
    }

    /// Replace the assortment. The next sync copies it whole and deactivates
    /// the catalogue products it no longer contains.
    pub async fn set_assortment(&self, assortment: &Assortment) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM catalogue_subscriptions")
            .execute(&mut *tx)
            .await?;
        let rules = assortment_rules(assortment);
        for (kind, target) in rules {
            sqlx::query("INSERT INTO catalogue_subscriptions (kind, target_id) VALUES ($1, $2)")
                .bind(kind)
                .bind(target)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE catalogue_sync_state SET needs_full_sync = true")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The store's sync state and assortment
    pub async fn status(&self) -> Result<CatalogueStatus> {
        let (last_seq, needs_full_sync, last_run_at, last_error): (
            i64,
            bool,
            Option<DateTime<Utc>>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT last_seq, needs_full_sync, last_run_at, last_error FROM catalogue_sync_state",
        )
        .fetch_one(&self.pool)
        .await?;
        let catalogue_products: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM products WHERE catalogue_synced_at IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(CatalogueStatus {
            last_seq,
            needs_full_sync,
            last_run_at,
            last_error,
            catalogue_products,
            assortment: self.assortment().await?,
        })
    }

    async fn position(&self) -> Result<(i64, bool)> {
        Ok(
            sqlx::query_as("SELECT last_seq, needs_full_sync FROM catalogue_sync_state")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    async fn save_position(&self, last_seq: i64) -> Result<()> {
        sqlx::query("UPDATE catalogue_sync_state SET last_seq = $1, needs_full_sync = false")
            .bind(last_seq)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn record_run(&self, error: Option<&Error>) -> Result<()> {
        sqlx::query("UPDATE catalogue_sync_state SET last_run_at = NOW(), last_error = $1")
            .bind(error.map(|e| e.to_string()))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// Assortment as subscription rows: (kind, target id)
fn assortment_rules(assortment: &Assortment) -> Vec<(&'static str, Option<Uuid>)> {
    let mut rules = Vec::new();
    if assortment.all {
        rules.push(("all", None));
    }
    let categories: HashSet<Uuid> = assortment.categories.iter().copied().collect();
    rules.extend(categories.into_iter().map(|id| ("category", Some(id))));
    let products: HashSet<Uuid> = assortment.products.iter().copied().collect();
    rules.extend(products.into_iter().map(|id| ("product", Some(id))));
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(seq: i64, entity: &str, id: Uuid, age_seconds: i64) -> CatalogueChange {
        CatalogueChange {
            seq,
            entity: entity.to_string(),
            entity_id: id,
            deleted: false,
            changed_at: Utc::now() - Duration::seconds(age_seconds),
        }
    }

    #[test]
    fn the_position_stops_before_recent_changes() {
        let id = Uuid::new_v4();
        let changes = [
            change(5, "product", id, 900),
            change(6, "product", id, 600),
            change(8, "product", id, 10),
            change(9, "product", id, 700),
        ];
        assert_eq!(settled_seq(&changes, 4, Utc::now()), 6);
        assert_eq!(settled_seq(&changes[2..], 7, Utc::now()), 7);
        assert_eq!(settled_seq(&[], 3, Utc::now()), 3);
    }

    #[test]
    fn each_changed_product_once() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let changes = [
            change(1, "product", a, 0),
            change(2, "category", b, 0),
            change(3, "product", b, 0),
            change(4, "product", a, 0),
        ];
        assert_eq!(changed_products(&changes), vec![a, b]);
    }

    #[test]
    fn assortment_rules_are_unique() {
        let c = Uuid::new_v4();
        let rules = assortment_rules(&Assortment {
            all: true,
            categories: vec![c, c],
            products: vec![],
        });
        assert_eq!(rules, vec![("all", None), ("category", Some(c))]);
        assert!(assortment_rules(&Assortment::default()).is_empty());
    }
}
