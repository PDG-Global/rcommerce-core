//! Database migration system for R Commerce
//!
//! This module provides automatic database schema management:
//! - Runs migrations on startup
//! - Tracks applied migrations
//! - Supports seeding demo data
//!
//! MIGRATION SYSTEM NOTES:
//! - Migration 1 (001_complete_schema.sql) is a comprehensive, self-contained migration
//!   that creates the entire database schema in correct dependency order.
//! - This is the ONLY migration needed for fresh installations.
//! - All previous migrations (001-025) have been consolidated into this single file.
//! - The migration is idempotent - it drops everything first and recreates from scratch.

use sqlx::{PgPool, Row};
use tracing::{error, info, warn};

use crate::{Error, Result};

/// Migration record tracking applied migrations
#[derive(Debug, Clone)]
pub struct Migration {
    pub version: i64,
    pub name: String,
    pub applied_at: chrono::DateTime<chrono::Utc>,
}

/// Database migration manager
pub struct Migrator {
    pool: PgPool,
}

impl Migrator {
    /// Create a new migrator instance
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Initialize migration tracking table
    async fn init_migration_table(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _migrations (
                version BIGINT PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Get list of applied migrations
    async fn get_applied_migrations(&self) -> Result<Vec<Migration>> {
        let rows =
            sqlx::query(r#"SELECT version, name, applied_at FROM _migrations ORDER BY version"#)
                .fetch_all(&self.pool)
                .await
                .map_err(Error::Database)?;

        let migrations = rows
            .into_iter()
            .map(|row| Migration {
                version: row.get("version"),
                name: row.get("name"),
                applied_at: row.get("applied_at"),
            })
            .collect();

        Ok(migrations)
    }

    /// Record a migration as applied
    async fn record_migration(&self, version: i64, name: &str) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO _migrations (version, name) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
        )
        .bind(version)
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Run all pending migrations
    pub async fn migrate(&self) -> Result<()> {
        info!("Initializing migration system...");
        self.init_migration_table().await?;

        let applied = self.get_applied_migrations().await?;
        info!("Found {} applied migrations", applied.len());

        // Define all migrations
        // Note: Only migration 1 is needed - it's a comprehensive schema creation
        let migrations = vec![
            (
                1,
                "complete_schema",
                include_str!("../../migrations/001_complete_schema.sql"),
            ),
            (
                2,
                "tax_system",
                include_str!("../../migrations/002_tax_system.sql"),
            ),
            (
                3,
                "inventory_notifications_fulfillment",
                include_str!("../../migrations/003_inventory_notifications_fulfillment.sql"),
            ),
            (
                4,
                "add_customer_imported_flag",
                include_str!("../../migrations/004_add_customer_imported_flag.sql"),
            ),
            (
                5,
                "add_email_verified",
                include_str!("../../migrations/005_add_email_verified.sql"),
            ),
            (
                6,
                "fix_order_item_titles",
                include_str!("../../migrations/004_fix_order_item_titles.sql"),
            ),
            (
                7,
                "notification_delivery_and_tracking_tables",
                include_str!("../../migrations/007_notification_delivery_and_tracking_tables.sql"),
            ),
            (
                8,
                "product_reviews_and_newsletter",
                include_str!("../../migrations/008_product_reviews_and_newsletter.sql"),
            ),
            (
                9,
                "rename_orders_email",
                include_str!("../../migrations/009_rename_orders_email.sql"),
            ),
            (
                10,
                "align_currency_columns",
                include_str!("../../migrations/010_align_currency_columns.sql"),
            ),
            (
                11,
                "widen_api_key_prefix",
                include_str!("../../migrations/011_widen_api_key_prefix.sql"),
            ),
            (
                12,
                "accounting_export",
                include_str!("../../migrations/012_accounting_export.sql"),
            ),
            (
                13,
                "stock_reservations_order_id_nullable",
                include_str!("../../migrations/013_stock_reservations_order_id_nullable.sql"),
            ),
            (
                14,
                "inventory_levels_dedupe",
                include_str!("../../migrations/014_inventory_levels_dedupe.sql"),
            ),
            (
                15,
                "stock_movement_type_enum",
                include_str!("../../migrations/015_stock_movement_type_enum.sql"),
            ),
            (
                16,
                "fresh_install_drift_fix",
                include_str!("../../migrations/016_fresh_install_drift_fix.sql"),
            ),
            (
                17,
                "product_gtin",
                include_str!("../../migrations/017_product_gtin.sql"),
            ),
            (
                18,
                "product_variant_gtin",
                include_str!("../../migrations/018_product_variant_gtin.sql"),
            ),
            (
                19,
                "payments_legacy",
                include_str!("../../migrations/019_payments_legacy.sql"),
            ),
            (
                20,
                "subscriptions_money",
                include_str!("../../migrations/020_subscriptions_money.sql"),
            ),
            (
                21,
                "default_warehouse",
                include_str!("../../migrations/021_default_warehouse.sql"),
            ),
            (
                22,
                "catalog_translations",
                include_str!("../../migrations/022_catalog_translations.sql"),
            ),
        ];

        for (version, name, sql) in migrations {
            if applied.iter().any(|m| m.version == version) {
                info!("Migration {} ({}) already applied, skipping", version, name);
                continue;
            }

            info!("Applying migration {} ({})...", version, name);

            // Execute the entire migration SQL as a single batch
            // This is necessary because splitting by semicolons breaks DO blocks
            sqlx::raw_sql(sql).execute(&self.pool).await.map_err(|e| {
                error!("Failed to execute migration {}: {}", version, e);
                Error::Database(e)
            })?;

            // Record migration
            self.record_migration(version, name).await?;
            info!("Migration {} ({}) applied successfully", version, name);
        }

        info!("All migrations completed successfully!");
        Ok(())
    }

    /// Reset database (drop all tables and re-run migrations)
    pub async fn reset(&self) -> Result<()> {
        warn!("RESETTING DATABASE - This will delete all data!");

        // Drop all tables
        sqlx::query(
            r#"
            DO $$ DECLARE
                r RECORD;
            BEGIN
                FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') LOOP
                    EXECUTE 'DROP TABLE IF EXISTS ' || quote_ident(r.tablename) || ' CASCADE';
                END LOOP;
            END $$;
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Drop all types
        sqlx::query(
            r#"
            DO $$ DECLARE
                r RECORD;
            BEGIN
                FOR r IN (SELECT typname FROM pg_type WHERE typtype = 'e' AND typnamespace = 'public'::regnamespace) LOOP
                    EXECUTE 'DROP TYPE IF EXISTS ' || quote_ident(r.typname) || ' CASCADE';
                END LOOP;
            END $$;
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        info!("Database reset complete. Re-running migrations...");
        self.migrate().await?;

        Ok(())
    }

    /// Seed database with demo data
    pub async fn seed(&self) -> Result<()> {
        info!("Seeding database with demo data...");

        // Check if products already exist
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        if count > 0 {
            warn!(
                "Database already contains {} products, skipping seed",
                count
            );
            return Ok(());
        }

        // Insert demo products
        sqlx::query(
            r#"
            INSERT INTO products (
                id, title, slug, description, sku, price, compare_at_price, cost_price,
                currency, inventory_quantity, inventory_policy, product_type, inventory_management,
                weight, weight_unit, requires_shipping, is_active, is_featured,
                seo_title, seo_description, created_at, updated_at, published_at
            ) VALUES 
            (
                '550e8400-e29b-41d4-a716-446655440001',
                'Premium Wireless Headphones',
                'premium-wireless-headphones',
                'High-quality wireless headphones with active noise cancellation and 30-hour battery life.',
                'HEADPHONES-001', 299.99, 349.99, 150.00, 'USD', 100, 'deny', 'simple', true,
                0.25, 'kg', true, true, true,
                'Premium Wireless Headphones - 30hr Battery | R Commerce',
                'Experience premium sound with our wireless headphones.',
                NOW(), NOW(), NOW()
            ),
            (
                '550e8400-e29b-41d4-a716-446655440002',
                'Smart Watch Pro',
                'smart-watch-pro',
                'Advanced fitness tracking smartwatch with heart rate monitoring and GPS.',
                'WATCH-001', 399.99, 449.99, 200.00, 'USD', 75, 'deny', 'simple', true,
                0.05, 'kg', true, true, true,
                'Smart Watch Pro - Fitness & Health Tracking | R Commerce',
                'Track your fitness and health with Smart Watch Pro.',
                NOW(), NOW(), NOW()
            ),
            (
                '550e8400-e29b-41d4-a716-446655440003',
                'Portable Bluetooth Speaker',
                'portable-bluetooth-speaker',
                '360-degree immersive sound in a compact, portable design.',
                'SPEAKER-001', 149.99, 179.99, 75.00, 'USD', 150, 'deny', 'simple', true,
                0.5, 'kg', true, true, false,
                'Portable Bluetooth Speaker - 360 Sound | R Commerce',
                'Take your music anywhere with our portable Bluetooth speaker.',
                NOW(), NOW(), NOW()
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Insert product images
        sqlx::query(
            r#"
            INSERT INTO product_images (id, product_id, position, src, alt_text, created_at) VALUES
            ('550e8400-e29b-41d4-a716-446655441001', '550e8400-e29b-41d4-a716-446655440001', 1, '/uploads/products/headphones-main.jpg', 'Premium Wireless Headphones - Main View', NOW()),
            ('550e8400-e29b-41d4-a716-446655441002', '550e8400-e29b-41d4-a716-446655440002', 1, '/uploads/products/watch-main.jpg', 'Smart Watch Pro - Main View', NOW()),
            ('550e8400-e29b-41d4-a716-446655441003', '550e8400-e29b-41d4-a716-446655440003', 1, '/uploads/products/speaker-main.jpg', 'Portable Bluetooth Speaker - Main View', NOW())
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Insert product variants
        sqlx::query(
            r#"
            INSERT INTO product_variants (
                id, product_id, title, sku, price, compare_at_price, cost_price,
                currency, inventory_quantity, inventory_policy, is_active, created_at, updated_at
            ) VALUES
            ('550e8400-e29b-41d4-a716-446655442001', '550e8400-e29b-41d4-a716-446655440001', 'Default', 'HEADPHONES-001-DEFAULT', 299.99, 349.99, 150.00, 'USD', 100, 'deny', true, NOW(), NOW()),
            ('550e8400-e29b-41d4-a716-446655442002', '550e8400-e29b-41d4-a716-446655440002', 'Default', 'WATCH-001-DEFAULT', 399.99, 449.99, 200.00, 'USD', 75, 'deny', true, NOW(), NOW()),
            ('550e8400-e29b-41d4-a716-446655442003', '550e8400-e29b-41d4-a716-446655440003', 'Default', 'SPEAKER-001-DEFAULT', 149.99, 179.99, 75.00, 'USD', 150, 'deny', true, NOW(), NOW())
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        info!("Demo data seeded successfully!");
        Ok(())
    }

    /// Get database status
    pub async fn status(&self) -> Result<DbStatus> {
        self.init_migration_table().await?;

        let applied = self.get_applied_migrations().await?;

        // Get table counts
        let product_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let customer_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let order_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orders")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        Ok(DbStatus {
            applied_migrations: applied.len() as i64,
            product_count,
            customer_count,
            order_count,
        })
    }
}

/// Database status information
#[derive(Debug, Clone)]
pub struct DbStatus {
    pub applied_migrations: i64,
    pub product_count: i64,
    pub customer_count: i64,
    pub order_count: i64,
}

/// Run migrations automatically on server start
pub async fn auto_migrate(pool: &PgPool) -> Result<()> {
    let migrator = Migrator::new(pool.clone());
    migrator.migrate().await?;
    Ok(())
}
