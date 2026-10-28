//! Integration tests for variant-aware inventory availability.
//!
//! These tests require a PostgreSQL database. Point `TEST_DATABASE_URL` at a
//! scratch database (default: `postgres://postgres@localhost:5432/rcommerce_scratch`);
//! if the database is unreachable the tests skip gracefully so `cargo test`
//! passes on machines without Postgres.

use chrono::Duration;
use rcommerce_core::inventory::{InventoryConfig, InventoryService, StockReservation};
use rcommerce_core::repository::Database;
use rcommerce_core::Migrator;
use sqlx::PgPool;
use uuid::Uuid;

fn test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string())
}

/// Connect to the test database, run migrations, and return a ready service.
/// Returns None (test skips) if no database is available.
async fn setup() -> Option<(PgPool, InventoryService)> {
    let url = test_db_url();
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect(&url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping inventory variant tests: cannot connect to {url}: {e}");
            return None;
        }
    };

    // Build the schema through the real migration path.
    if let Err(e) = Migrator::new(pool.clone()).migrate().await {
        eprintln!("Skipping inventory variant tests: migration failed: {e}");
        return None;
    }

    // Migration 013 is not registered in the migrator (applied by hand on the
    // live databases). Apply it here so stock_reservations matches what the
    // code expects: nullable order_id and the reservation_status enum.
    let fixup = include_str!("../migrations/013_stock_reservations_order_id_nullable.sql");
    if let Err(e) = sqlx::raw_sql(fixup).execute(&pool).await {
        eprintln!("Skipping inventory variant tests: 013 fixup failed: {e}");
        return None;
    }

    let service = InventoryService::new(Database::new(pool.clone()), InventoryConfig::default());
    Some((pool, service))
}

/// Seed a variable product with two variants (A: 10 units, B: 30 units)
/// at one location, using per-variant inventory_levels rows.
/// Returns (product_id, variant_a, variant_b, location_id).
async fn seed_variable_product(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid) {
    let product_id = Uuid::new_v4();
    let variant_a = Uuid::new_v4();
    let variant_b = Uuid::new_v4();
    let location_id = Uuid::new_v4();

    sqlx::query("DELETE FROM product_variants WHERE product_id = $1")
        .bind(product_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_levels WHERE product_id = $1")
        .bind(product_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_locations WHERE id = $1")
        .bind(location_id)
        .execute(pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO products (id, title, slug, price) VALUES ($1, 'Variant Test', $2, 10.00)",
    )
    .bind(product_id)
    .bind(format!("variant-test-{}", product_id))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO inventory_locations (id, name, code) VALUES ($1, 'Test Warehouse', $2)",
    )
    .bind(location_id)
    .bind(format!("TST-{}", &location_id.to_string()[..8]))
    .execute(pool)
    .await
    .unwrap();

    for (vid, title, qty) in [(variant_a, "A", 10), (variant_b, "B", 30)] {
        sqlx::query(
            "INSERT INTO product_variants (id, product_id, title, price, inventory_quantity) \
             VALUES ($1, $2, $3, 10.00, $4)",
        )
        .bind(vid)
        .bind(product_id)
        .bind(title)
        .bind(qty)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO inventory_levels (product_id, variant_id, location_id, available_quantity) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(product_id)
        .bind(vid)
        .bind(location_id)
        .bind(qty)
        .execute(pool)
        .await
        .unwrap();
    }

    (product_id, variant_a, variant_b, location_id)
}

#[tokio::test]
async fn inventory_reserve_variant_does_not_use_other_variants_stock() {
    let Some((pool, service)) = setup().await else {
        return;
    };
    let (product_id, variant_a, variant_b, location_id) = seed_variable_product(&pool).await;
    let expires = chrono::Utc::now() + Duration::minutes(30);

    // Variant A has 10 units; variant B has 30. Reserving 11 of A must fail
    // even though the product has 40 units in total.
    let err = service
        .reserve_stock(StockReservation::new(
            product_id,
            Some(variant_a),
            location_id,
            None,
            11,
            expires,
        ))
        .await
        .expect_err("reserving more than variant A's stock must fail");
    assert!(
        err.to_string().contains("Insufficient stock"),
        "unexpected error: {err}"
    );

    // Reserving 10 of A (exactly its level) succeeds.
    service
        .reserve_stock(StockReservation::new(
            product_id,
            Some(variant_a),
            location_id,
            None,
            10,
            expires,
        ))
        .await
        .expect("reserving variant A's exact stock must succeed");

    // Variant B is untouched and can still be fully reserved.
    service
        .reserve_stock(StockReservation::new(
            product_id,
            Some(variant_b),
            location_id,
            None,
            30,
            expires,
        ))
        .await
        .expect("reserving variant B's stock must be unaffected by variant A");

    cleanup(&pool, product_id, location_id).await;
}

#[tokio::test]
async fn inventory_commit_reservation_no_matching_row_does_not_fail() {
    let Some((pool, service)) = setup().await else {
        return;
    };
    let (product_id, variant_a, _variant_b, location_id) = seed_variable_product(&pool).await;

    // A reservation whose inventory row is missing (e.g. NULL-variant
    // reservation on a variants-only product) must not fail the commit; it
    // reports Ok(false) and logs a loud warning. The variant itself must
    // exist (FK) but has no inventory_levels row.
    let orphan_variant = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO product_variants (id, product_id, title, price, inventory_quantity) \
         VALUES ($1, $2, 'Orphan', 10.00, 0)",
    )
    .bind(orphan_variant)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    let orphan = StockReservation::new(
        product_id,
        Some(orphan_variant),
        location_id,
        None,
        2,
        chrono::Utc::now() + Duration::minutes(30),
    );
    let orphan_id = orphan.id;
    sqlx::query(
        "INSERT INTO stock_reservations \
         (id, product_id, variant_id, location_id, order_id, quantity, status, expires_at) \
         VALUES ($1, $2, $3, $4, NULL, $5, 'active', $6)",
    )
    .bind(orphan.id)
    .bind(orphan.product_id)
    .bind(orphan.variant_id)
    .bind(orphan.location_id)
    .bind(orphan.quantity)
    .bind(orphan.expires_at)
    .execute(&pool)
    .await
    .unwrap();

    let committed = service
        .commit_reservation(orphan_id)
        .await
        .expect("commit with no matching inventory row must not error");
    assert!(
        !committed,
        "no row was updated, so commit must report false"
    );

    let status: String =
        sqlx::query_scalar("SELECT status::text FROM stock_reservations WHERE id = $1")
            .bind(orphan_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "committed", "reservation must still be committed");

    cleanup(&pool, product_id, location_id).await;
}

#[tokio::test]
async fn inventory_simple_product_reservation_uses_null_variant_rows() {
    let Some((pool, service)) = setup().await else {
        return;
    };
    let product_id = Uuid::new_v4();
    let location_id = Uuid::new_v4();
    let expires = chrono::Utc::now() + Duration::minutes(30);

    sqlx::query("DELETE FROM inventory_locations WHERE id = $1")
        .bind(location_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO products (id, title, slug, price) VALUES ($1, 'Simple Test', $2, 5.00)",
    )
    .bind(product_id)
    .bind(format!("simple-test-{}", product_id))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inventory_locations (id, name, code) VALUES ($1, 'Test Warehouse 2', $2)",
    )
    .bind(location_id)
    .bind(format!("TS2-{}", &location_id.to_string()[..8]))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inventory_levels (product_id, location_id, available_quantity) \
         VALUES ($1, $2, 7)",
    )
    .bind(product_id)
    .bind(location_id)
    .execute(&pool)
    .await
    .unwrap();

    // Simple-product (NULL variant) reservations read the NULL-variant rows.
    service
        .reserve_stock(StockReservation::new(
            product_id,
            None,
            location_id,
            None,
            7,
            expires,
        ))
        .await
        .expect("reserving a simple product's exact stock must succeed");

    let err = service
        .reserve_stock(StockReservation::new(
            product_id,
            None,
            location_id,
            None,
            8,
            expires,
        ))
        .await
        .expect_err("requesting more than the simple product's level must fail");
    assert!(err.to_string().contains("Insufficient stock"));

    // receive_stock must accumulate onto the existing NULL-variant row
    // (update-then-insert), not create a duplicate row.
    service
        .receive_stock(product_id, location_id, 3, None)
        .await
        .unwrap();
    let row_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM inventory_levels WHERE product_id = $1 AND location_id = $2",
    )
    .bind(product_id)
    .bind(location_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row_count, 1, "receive_stock must not duplicate rows");

    cleanup(&pool, product_id, location_id).await;
}

#[tokio::test]
async fn inventory_product_level_availability_is_variant_aware() {
    let Some((pool, service)) = setup().await else {
        return;
    };
    let (product_id, variant_a, variant_b, location_id) = seed_variable_product(&pool).await;

    // Variant-scoped availability.
    let a: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels \
         WHERE product_id = $1 AND variant_id IS NOT DISTINCT FROM $2",
    )
    .bind(product_id)
    .bind(variant_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(a, 10);

    let b: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels \
         WHERE product_id = $1 AND variant_id IS NOT DISTINCT FROM $2",
    )
    .bind(product_id)
    .bind(variant_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(b, 30);

    // Service-level cross-location check mirrors the cart validation path.
    let avail_a = service
        .get_available_stock_for_product(product_id, Some(variant_a))
        .await
        .unwrap();
    let avail_b = service
        .get_available_stock_for_product(product_id, Some(variant_b))
        .await
        .unwrap();
    assert_eq!(avail_a, 10);
    assert_eq!(avail_b, 30);

    cleanup(&pool, product_id, location_id).await;
}

async fn cleanup(pool: &PgPool, product_id: Uuid, location_id: Uuid) {
    sqlx::query("DELETE FROM stock_reservations WHERE product_id = $1")
        .bind(product_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_levels WHERE product_id = $1")
        .bind(product_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM product_variants WHERE product_id = $1")
        .bind(product_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_locations WHERE id = $1")
        .bind(location_id)
        .execute(pool)
        .await
        .unwrap();
}
