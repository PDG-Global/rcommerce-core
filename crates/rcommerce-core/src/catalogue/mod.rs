//! Catalogue sync: a catalogue database publishes its products and categories,
//! and each store copies the products of its assortment.
//!
//! A database with `[catalogue] publish_changes = true` logs every product and
//! category change in `catalogue_changes` (database triggers, migration 024).
//! A store with `[catalogue] source_url` reads that log since its last run and
//! copies each changed product whole: its row, variants, images, options and
//! category links. Copies keep the catalogue's ids.
//!
//! The store keeps what it owns: prices, stock, SKUs, the active and featured
//! flags. New products arrive inactive, so a store only sells what it has
//! priced. A product deleted in the catalogue, or no longer in the store's
//! assortment, is deactivated rather than deleted, because orders refer to it.

pub mod source;
pub mod sync;

pub use source::{
    Assortment, CatalogueChange, CatalogueSource, PgCatalogueSource, ProductBundle, SourceCategory,
};
pub use sync::{CatalogueStatus, CatalogueSync, SyncReport};

use sqlx::PgPool;

/// Change-log entries older than this are pruned when publishing starts
const CHANGE_LOG_DAYS: i32 = 30;

/// Turn change logging on or off for this database (`[catalogue]
/// publish_changes`). Turning it on also prunes old log entries.
pub async fn set_publishing(pool: &PgPool, enabled: bool) -> crate::Result<()> {
    sqlx::query("UPDATE catalogue_settings SET publish_changes = $1")
        .bind(enabled)
        .execute(pool)
        .await?;
    if enabled {
        sqlx::query(
            "DELETE FROM catalogue_changes WHERE changed_at < NOW() - make_interval(days => $1)",
        )
        .bind(CHANGE_LOG_DAYS)
        .execute(pool)
        .await?;
    }
    Ok(())
}
