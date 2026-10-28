//! Catalogue sync between two real databases: a catalogue that publishes its
//! changes and a store that copies its assortment.
//!
//! Needs TEST_DATABASE_URL: the test creates two databases on that server and
//! drops them at the end. Skipped without it.

use std::sync::Arc;

use rcommerce_core::catalogue::{
    set_publishing, Assortment, CatalogueSync, PgCatalogueSource, SyncReport,
};
use rcommerce_core::Migrator;
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

/// A fresh, migrated database next to the one in `server_url`
async fn fresh_database(admin: &PgPool, server_url: &str, name: &str) -> PgPool {
    sqlx::query(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .execute(admin)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(admin)
        .await
        .unwrap();
    let (base, _) = server_url
        .rsplit_once('/')
        .expect("TEST_DATABASE_URL has a path");
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(&format!("{base}/{name}"))
        .await
        .unwrap();
    Migrator::new(pool.clone()).migrate().await.unwrap();
    pool
}

async fn exec(pool: &PgPool, sql: &str, ids: &[Uuid]) {
    let mut query = sqlx::query(sql);
    for id in ids {
        query = query.bind(*id);
    }
    query.execute(pool).await.unwrap();
}

/// Categories A > A1 and B; P1 in A1 with a variant, an image and an option;
/// P2 in B
async fn seed_catalogue(src: &PgPool, ids: &Ids) {
    exec(src, "INSERT INTO product_categories (id, name, slug) VALUES ($1, 'Tools', 'tools'), ($2, 'Drills', 'drills'), ($3, 'Paint', 'paint')", &[ids.a, ids.a1, ids.b]).await;
    exec(
        src,
        "UPDATE product_categories SET parent_id = $1 WHERE id = $2",
        &[ids.a, ids.a1],
    )
    .await;
    exec(src, "INSERT INTO products (id, title, slug, price) VALUES ($1, 'Drill', 'drill', 99.00), ($2, 'White paint', 'white-paint', 25.00)", &[ids.p1, ids.p2]).await;
    exec(src, "INSERT INTO product_variants (id, product_id, title, price) VALUES ($1, $2, '18 V', 99.00)", &[ids.v1, ids.p1]).await;
    exec(src, "INSERT INTO product_images (id, product_id, src, position) VALUES ($1, $2, 'https://cdn.example/drill.jpg', 0)", &[Uuid::new_v4(), ids.p1]).await;
    exec(
        src,
        "INSERT INTO product_options (id, product_id, name) VALUES ($1, $2, 'Voltage')",
        &[ids.o1, ids.p1],
    )
    .await;
    exec(src, "INSERT INTO product_option_values (id, option_id, variant_id, value) VALUES ($1, $2, $3, '18 V')", &[Uuid::new_v4(), ids.o1, ids.v1]).await;
    exec(src, "INSERT INTO product_category_relations (product_id, category_id, is_primary) VALUES ($1, $2, true), ($3, $4, true)", &[ids.p1, ids.a1, ids.p2, ids.b]).await;
}

struct Ids {
    a: Uuid,
    a1: Uuid,
    b: Uuid,
    p1: Uuid,
    p2: Uuid,
    v1: Uuid,
    o1: Uuid,
}

async fn count(pool: &PgPool, sql: &str, id: Uuid) -> i64 {
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn stores_copy_their_assortment_from_the_catalogue() {
    let Ok(admin_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("Skipping catalogue sync test: set TEST_DATABASE_URL");
        return;
    };
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_url)
        .await
        .unwrap();
    let src = fresh_database(&admin, &admin_url, "rc_catsync_src").await;
    let dst = fresh_database(&admin, &admin_url, "rc_catsync_dst").await;
    set_publishing(&src, true).await.unwrap();
    let ids = Ids {
        a: Uuid::new_v4(),
        a1: Uuid::new_v4(),
        b: Uuid::new_v4(),
        p1: Uuid::new_v4(),
        p2: Uuid::new_v4(),
        v1: Uuid::new_v4(),
        o1: Uuid::new_v4(),
    };
    seed_catalogue(&src, &ids).await;
    let sync = CatalogueSync::new(Arc::new(PgCatalogueSource::new(src.clone())), dst.clone());

    // First sync of a category assortment: the sub-category's product, inactive
    sync.set_assortment(&Assortment {
        categories: vec![ids.a],
        ..Assortment::default()
    })
    .await
    .unwrap();
    let report = sync.run(false).await.unwrap();
    assert_eq!(
        (
            report.full,
            report.products_copied,
            report.categories_copied
        ),
        (true, 1, 3)
    );
    let (active, price): (bool, Decimal) =
        sqlx::query_as("SELECT is_active, price FROM products WHERE id = $1")
            .bind(ids.p1)
            .fetch_one(&dst)
            .await
            .unwrap();
    assert!(!active, "new catalogue products arrive inactive");
    assert_eq!(price, Decimal::new(9900, 2));
    assert_eq!(
        count(&dst, "SELECT COUNT(*) FROM products WHERE id = $1", ids.p2).await,
        0
    );
    assert_eq!(
        count(
            &dst,
            "SELECT COUNT(*) FROM product_variants WHERE product_id = $1",
            ids.p1
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &dst,
            "SELECT COUNT(*) FROM product_images WHERE product_id = $1",
            ids.p1
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &dst,
            "SELECT COUNT(*) FROM product_option_values WHERE option_id = $1",
            ids.o1
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &dst,
            "SELECT COUNT(*) FROM product_category_relations WHERE product_id = $1",
            ids.p1
        )
        .await,
        1
    );

    // The store prices, activates and files the product in its own category
    let local = Uuid::new_v4();
    exec(
        &dst,
        "UPDATE products SET price = 89.00, is_active = true WHERE id = $1",
        &[ids.p1],
    )
    .await;
    exec(
        &dst,
        "INSERT INTO product_categories (id, name, slug) VALUES ($1, 'Deals', 'deals')",
        &[local],
    )
    .await;
    exec(
        &dst,
        "INSERT INTO product_category_relations (product_id, category_id) VALUES ($1, $2)",
        &[ids.p1, local],
    )
    .await;

    // The catalogue renames it, changes its price and adds an image
    exec(
        &src,
        "UPDATE products SET title = 'Cordless drill', price = 120.00 WHERE id = $1",
        &[ids.p1],
    )
    .await;
    exec(&src, "INSERT INTO product_images (id, product_id, src, position) VALUES ($1, $2, 'https://cdn.example/drill-2.jpg', 1)", &[Uuid::new_v4(), ids.p1]).await;
    let report = sync.run(false).await.unwrap();
    assert!(!report.full);
    assert_eq!(report.products_copied, 1);
    let (title, price, active): (String, Decimal, bool) =
        sqlx::query_as("SELECT title, price, is_active FROM products WHERE id = $1")
            .bind(ids.p1)
            .fetch_one(&dst)
            .await
            .unwrap();
    assert_eq!(title, "Cordless drill");
    assert_eq!(price, Decimal::new(8900, 2), "the store's price is kept");
    assert!(active, "the store's active flag is kept");
    assert_eq!(
        count(
            &dst,
            "SELECT COUNT(*) FROM product_images WHERE product_id = $1",
            ids.p1
        )
        .await,
        2
    );
    assert_eq!(
        count(
            &dst,
            "SELECT COUNT(*) FROM product_category_relations WHERE product_id = $1",
            ids.p1
        )
        .await,
        2
    );

    // Deleted from the catalogue: deactivated in the store, not deleted
    exec(&src, "DELETE FROM products WHERE id = $1", &[ids.p1]).await;
    let report = sync.run(false).await.unwrap();
    assert_eq!(report.products_deactivated, 1);
    assert_eq!(
        count(
            &dst,
            "SELECT COUNT(*) FROM products WHERE id = $1 AND NOT is_active",
            ids.p1
        )
        .await,
        1
    );

    // A wider assortment: the whole catalogue
    sync.set_assortment(&Assortment {
        all: true,
        ..Assortment::default()
    })
    .await
    .unwrap();
    let report: SyncReport = sync.run(false).await.unwrap();
    assert!(report.full);
    assert_eq!(
        report.products_copied, 1,
        "P2; P1 is gone from the catalogue"
    );
    let status = sync.status().await.unwrap();
    assert!(status.assortment.all && status.last_error.is_none());
    assert_eq!(status.catalogue_products, 2);

    src.close().await;
    dst.close().await;
    for name in ["rc_catsync_src", "rc_catsync_dst"] {
        sqlx::query(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .execute(&admin)
            .await
            .ok();
    }
}
