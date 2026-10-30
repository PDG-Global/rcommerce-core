use std::{collections::HashSet, sync::Arc};

use rcommerce_core::{
    models::{AddToCartInput, UpdateCartItemInput},
    repository::{Database, PgCartRepository, PgCouponRepository},
    services::{
        cart_service::{BundleCartDetails, BundleComponentDetails, ProductDetails},
        CartService, CouponService,
    },
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

fn test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string())
}

async fn setup() -> Option<(PgPool, Arc<CartService>)> {
    let url = test_db_url();
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("Skipping cart concurrency tests: cannot connect to {url}: {error}");
            return None;
        }
    };
    if let Err(error) = rcommerce_core::ensure_test_migrations(&pool).await {
        eprintln!("Skipping cart concurrency tests: migration failed: {error}");
        return None;
    }

    let db = Database::new(pool.clone());
    let cart_repo = Arc::new(PgCartRepository::new(pool.clone()));
    let coupon_repo = Arc::new(PgCouponRepository::new(pool.clone()));
    let coupon_service = Arc::new(CouponService::new(coupon_repo.clone(), cart_repo.clone()));
    let service = CartService::new(cart_repo, coupon_repo, coupon_service).with_database(db);
    Some((pool, Arc::new(service)))
}

fn product_details(price: Decimal) -> ProductDetails {
    ProductDetails {
        unit_price: price,
        original_price: price,
        sku: Some("TEST".to_string()),
        title: "Concurrency Test Product".to_string(),
        variant_title: None,
        image_url: None,
        requires_shipping: true,
        is_gift_card: false,
    }
}

async fn seed_cart(pool: &PgPool) -> Uuid {
    let cart_id = Uuid::new_v4();
    sqlx::query("INSERT INTO carts (id, session_token) VALUES ($1, $2)")
        .bind(cart_id)
        .bind(format!("test-{cart_id}"))
        .execute(pool)
        .await
        .unwrap();
    cart_id
}

async fn seed_product(pool: &PgPool, stock: i32, price: Decimal) -> (Uuid, Uuid) {
    let product_id = Uuid::new_v4();
    let location_id = Uuid::new_v4();
    sqlx::query("INSERT INTO inventory_locations (id, name, code) VALUES ($1, 'Cart Test', $2)")
        .bind(location_id)
        .bind(format!("CT-{}", &location_id.to_string()[..8]))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO products (id, title, slug, price) VALUES ($1, 'Cart Test', $2, $3)")
        .bind(product_id)
        .bind(format!("cart-test-{product_id}"))
        .bind(price)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO inventory_levels (product_id, location_id, available_quantity) VALUES ($1, $2, $3)",
    )
    .bind(product_id)
    .bind(location_id)
    .bind(stock)
    .execute(pool)
    .await
    .unwrap();
    (product_id, location_id)
}

async fn cleanup_product(pool: &PgPool, cart_id: Uuid, product_id: Uuid, location_id: Uuid) {
    sqlx::query("DELETE FROM carts WHERE id = $1")
        .bind(cart_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_levels WHERE product_id = $1")
        .bind(product_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM bundle_components WHERE bundle_product_id = $1 OR component_product_id = $1",
    )
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

#[tokio::test]
async fn concurrent_adds_preserve_quantities_ids_and_header_totals() {
    let Some((pool, service)) = setup().await else {
        return;
    };
    let cart_id = seed_cart(&pool).await;
    let (product_id, location_id) = seed_product(&pool, 6, Decimal::new(10, 0)).await;
    let coupon_code = format!("CART-{}", &cart_id.to_string()[..8]);
    sqlx::query(
        "INSERT INTO coupons (code, discount_type, discount_value) VALUES ($1, 'percentage', 10)",
    )
    .bind(&coupon_code)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE carts SET coupon_code = $2, shipping_method = 'express', shipping_total = 5 WHERE id = $1",
    )
    .bind(cart_id)
    .bind(&coupon_code)
    .execute(&pool)
    .await
    .unwrap();

    let mut tasks = Vec::new();
    for _ in 0..6 {
        let service = service.clone();
        tasks.push(tokio::spawn(async move {
            service
                .add_item(
                    cart_id,
                    AddToCartInput {
                        product_id,
                        variant_id: None,
                        quantity: 1,
                        custom_attributes: None,
                    },
                    product_details(Decimal::new(10, 0)),
                )
                .await
                .unwrap()
        }));
    }
    let mut item_ids = HashSet::new();
    for task in tasks {
        item_ids.insert(task.await.unwrap().id);
    }

    let stored = service.get_cart_with_items(cart_id).await.unwrap();
    assert_eq!(stored.items.len(), 1);
    assert_eq!(stored.items[0].quantity, 6);
    assert_eq!(stored.items[0].subtotal, Decimal::new(60, 0));
    assert_eq!(item_ids.len(), 1);
    assert_eq!(stored.cart.subtotal, Decimal::new(60, 0));
    assert_eq!(stored.cart.discount_total, Decimal::new(6, 0));
    assert_eq!(stored.cart.shipping_total, Decimal::new(5, 0));
    assert_eq!(stored.cart.total, Decimal::new(59, 0));
    assert_eq!(stored.cart.shipping_method.as_deref(), Some("express"));
    assert_eq!(
        stored.cart.coupon_code.as_deref(),
        Some(coupon_code.as_str())
    );

    let error = service
        .add_item(
            cart_id,
            AddToCartInput {
                product_id,
                variant_id: None,
                quantity: 1,
                custom_attributes: None,
            },
            product_details(Decimal::new(10, 0)),
        )
        .await
        .expect_err("stock cap must include the existing cart quantity");
    assert!(error.to_string().contains("Only 6 units"));
    assert_eq!(
        service.get_cart_with_items(cart_id).await.unwrap().items[0].quantity,
        6
    );

    sqlx::query("DELETE FROM coupons WHERE code = $1")
        .bind(&coupon_code)
        .execute(&pool)
        .await
        .unwrap();
    cleanup_product(&pool, cart_id, product_id, location_id).await;
}

#[tokio::test]
async fn bundle_add_update_and_remove_preserve_lineage_and_stock() {
    let Some((pool, service)) = setup().await else {
        return;
    };
    let cart_id = seed_cart(&pool).await;
    let bundle_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO products (id, title, slug, product_type, price) VALUES ($1, 'Bundle Test', $2, 'bundle', 100)",
    )
    .bind(bundle_id)
    .bind(format!("bundle-test-{bundle_id}"))
    .execute(&pool)
    .await
    .unwrap();
    let (component_id, location_id) = seed_product(&pool, 20, Decimal::new(10, 0)).await;
    let details = BundleCartDetails {
        bundle_price: Decimal::new(100, 0),
        original_price: Decimal::new(100, 0),
        sku: Some("BUNDLE".to_string()),
        title: "Bundle Test".to_string(),
        image_url: None,
        requires_shipping: true,
        components: vec![BundleComponentDetails {
            product_id: component_id,
            quantity: 4,
            unit_price: Decimal::new(10, 0),
            sku: Some("COMPONENT".to_string()),
            title: "Bundle Component".to_string(),
            image_url: None,
            requires_shipping: true,
        }],
    };

    let first = service
        .add_bundle_item(cart_id, bundle_id, 2, details.clone())
        .await
        .unwrap();
    let parent_id = first[0].id;
    service
        .add_bundle_item(cart_id, bundle_id, 1, details.clone())
        .await
        .unwrap();
    let stored = service.get_cart_with_items(cart_id).await.unwrap();
    assert_eq!(stored.items.len(), 2);
    let parent = stored
        .items
        .iter()
        .find(|item| item.id == parent_id)
        .unwrap();
    let component = stored
        .items
        .iter()
        .find(|item| item.id != parent_id)
        .unwrap();
    assert_eq!(parent.quantity, 3);
    assert_eq!(component.quantity, 12);
    assert_eq!(stored.cart.subtotal, Decimal::new(300, 0));
    assert_eq!(stored.cart.total, Decimal::new(300, 0));

    service
        .update_item(
            cart_id,
            parent_id,
            UpdateCartItemInput {
                quantity: 4,
                custom_attributes: None,
            },
        )
        .await
        .unwrap();
    let stored = service.get_cart_with_items(cart_id).await.unwrap();
    let parent = stored
        .items
        .iter()
        .find(|item| item.id == parent_id)
        .unwrap();
    let component = stored
        .items
        .iter()
        .find(|item| item.id != parent_id)
        .unwrap();
    assert_eq!(parent.quantity, 4);
    assert_eq!(component.quantity, 16);
    assert_eq!(
        parent.custom_attributes.as_ref().unwrap()["is_bundle_parent"],
        true
    );
    assert_eq!(
        component.custom_attributes.as_ref().unwrap()["is_bundle_component"],
        true
    );

    service
        .update_item(
            cart_id,
            parent_id,
            UpdateCartItemInput {
                quantity: 6,
                custom_attributes: None,
            },
        )
        .await
        .expect_err("bundle stock must include existing component quantities");
    let stored = service.get_cart_with_items(cart_id).await.unwrap();
    assert_eq!(
        stored
            .items
            .iter()
            .find(|item| item.id == parent_id)
            .unwrap()
            .quantity,
        4
    );
    assert_eq!(
        stored
            .items
            .iter()
            .find(|item| item.id != parent_id)
            .unwrap()
            .quantity,
        16
    );

    service
        .add_item(
            cart_id,
            AddToCartInput {
                product_id: component_id,
                variant_id: None,
                quantity: 1,
                custom_attributes: None,
            },
            product_details(Decimal::new(10, 0)),
        )
        .await
        .expect_err("standalone and bundle component lines must not merge");

    service.remove_item(cart_id, parent_id).await.unwrap();
    assert!(service
        .get_cart_with_items(cart_id)
        .await
        .unwrap()
        .items
        .is_empty());

    sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(bundle_id)
        .execute(&pool)
        .await
        .unwrap();
    cleanup_product(&pool, cart_id, component_id, location_id).await;
}
