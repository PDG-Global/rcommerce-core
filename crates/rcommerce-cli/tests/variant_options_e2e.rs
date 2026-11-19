//! End-to-end tests for CLI variant/option creation (rcommerce-core issue #48).
//!
//! Proves the acceptance bar's two API-level tests against the REAL cart and
//! product routes (no mocks):
//!   (i)  a variant belonging to another product is refused at cart-add;
//!   (iv) a variable product whose options/variants were created by the CLI
//!        exposes its selectors through the product-detail API (the data the
//!        storefront renders) and the chosen variant adds to cart at the
//!        SERVER-STORED price — client-sent price/pack_size fields are ignored.
//!
//! Requires PostgreSQL via `TEST_DATABASE_URL` (default
//! `postgres://postgres@localhost:5432/rcommerce_scratch`); skipped without it.
//! The base product row is seeded with SQL because `product create` is
//! TTY-interactive; the option/variant creation under test runs through the
//! actual `rcommerce` binary (`product add-option` / `product add-variant`).

use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use rust_decimal::Decimal;
use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;
use tokio::net::TcpListener;
use uuid::Uuid;

use rcommerce_api::state::{AppState, AppStateParams};
use rcommerce_core::inventory::{InventoryConfig, InventoryService};
use rcommerce_core::order::lifecycle::OrderEventDispatcher;
use rcommerce_core::payment::agnostic::PaymentService;
use rcommerce_core::repository::cart_repository::PgCartRepository;
use rcommerce_core::repository::coupon_repository::PgCouponRepository;
use rcommerce_core::repository::{
    CustomerRepository, Database, PostgresApiKeyRepository, PostgresSubscriptionRepository,
    ProductRepository,
};
use rcommerce_core::services::{
    AuthService, CartService, CheckoutConfig, CheckoutService, CouponService, CustomerService,
    OrderService, ProductService,
};
use rcommerce_core::shipping::ShippingProviderFactory;
use rcommerce_core::{Config, FileUploadService, StripeGateway};

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string())
}

/// Full AppState over the real services (same construction as the
/// auth_security tests), eagerly connected.
async fn build_app_state(pool: PgPool) -> anyhow::Result<AppState> {
    let db = Database::new(pool.clone());

    let mut core_config = Config::default();
    core_config.security.jwt.secret = "test-secret-do-not-use-in-production-32-bytes".to_string();

    let product_service = ProductService::new(ProductRepository::new(db.clone()));
    let customer_service = CustomerService::new(CustomerRepository::new(db.clone()));
    let auth_service = AuthService::new(core_config);

    let cart_repo: Arc<dyn rcommerce_core::repository::CartRepository> =
        Arc::new(PgCartRepository::new(pool.clone()));
    let coupon_repo: Arc<dyn rcommerce_core::repository::CouponRepository> =
        Arc::new(PgCouponRepository::new(pool.clone()));

    let coupon_service = CouponService::new(coupon_repo.clone(), cart_repo.clone());
    let bundle_service = Arc::new(rcommerce_core::BundleService::new(db.clone()));
    let cart_service = Arc::new(
        CartService::new(
            cart_repo.clone(),
            coupon_repo.clone(),
            Arc::new(coupon_service.clone()),
        )
        .with_bundle_service(bundle_service)
        .with_database(db.clone()),
    );

    let payment_service = PaymentService::new("mock".to_string());
    let inventory_service = Arc::new(InventoryService::new(
        db.clone(),
        InventoryConfig::default(),
    ));
    let order_service = Arc::new(OrderService::new(
        db.clone(),
        Box::new(StripeGateway::new(
            "sk_test_dummy".to_string(),
            "whsec_dummy".to_string(),
        )),
        inventory_service.as_ref().clone(),
        OrderEventDispatcher::new(),
    ));

    let tax_service = Arc::new(rcommerce_core::tax::DefaultTaxService::new(pool.clone()));
    let mut shipping_factory = ShippingProviderFactory::new();
    shipping_factory.register(Box::new(
        rcommerce_core::shipping::carriers::ManualProvider::new(),
    ));
    let shipping_factory = Arc::new(shipping_factory);

    let checkout_service = Arc::new(CheckoutService::new(
        cart_service.clone(),
        tax_service.clone(),
        order_service.clone(),
        Arc::new(rcommerce_core::payment::gateways::MockPaymentGateway::new()),
        shipping_factory.clone(),
        CheckoutConfig::default(),
    ));

    let upload_dir = std::env::temp_dir().join(format!("rcommerce-e2e-uploads-{}", Uuid::new_v4()));
    let file_upload_service =
        FileUploadService::new_local(upload_dir, "http://localhost:8080/uploads".to_string())?;

    let params = AppStateParams::new(
        product_service,
        customer_service,
        auth_service,
        db,
        None,
        PostgresApiKeyRepository::new(pool.clone()),
        PostgresSubscriptionRepository::new(pool.clone()),
        coupon_service,
        payment_service,
        file_upload_service,
        cart_service,
        order_service,
        tax_service,
        shipping_factory,
        checkout_service,
        None,
        inventory_service,
        None,
        None,
        None,
        Default::default(),
        Default::default(),
        "USD".to_string(),
        None,
    );

    Ok(AppState::new(params).await)
}

/// The real product + guest-cart routes behind the real optional-auth
/// middleware (guests get `None`, exactly as in production).
fn app_router(state: AppState) -> Router {
    Router::new()
        .merge(rcommerce_api::routes::product::router())
        .merge(rcommerce_api::routes::cart::public_router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rcommerce_api::middleware::optional_auth_middleware,
        ))
        .with_state(state)
}

async fn serve(app: Router) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    addr
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()
        .unwrap()
}

/// Write a 0600 CLI config pointing at the test database (the CLI refuses
/// world-readable config files).
fn write_cli_config(url: &str) -> anyhow::Result<PathBuf> {
    let opts: PgConnectOptions = url.parse()?;
    let mut config = Config::default();
    config.database.host = opts.get_host().to_string();
    config.database.port = opts.get_port();
    config.database.database = opts.get_database().unwrap_or("postgres").to_string();
    config.database.username = opts.get_username().to_string();
    // PgConnectOptions exposes no password getter; take it from the URL userinfo.
    config.database.password = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('@').next())
        .and_then(|userinfo| userinfo.split_once(':'))
        .map(|(_, password)| password.to_string())
        .unwrap_or_default();
    config.security.jwt.secret = "test-secret-do-not-use-in-production-32-bytes".to_string();

    let dir = std::env::temp_dir().join(format!("rcommerce-e2e-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    std::fs::write(&path, toml::to_string(&config)?)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok(path)
}

/// Run the real CLI binary; returns stdout, panics with stderr on failure.
fn run_cli(config_path: &PathBuf, args: &[&str]) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rcommerce"))
        .arg("--config")
        .arg(config_path)
        .args(args)
        .output()
        .expect("failed to spawn rcommerce CLI");
    assert!(
        output.status.success(),
        "CLI {:?} failed:\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Run the real CLI binary expecting FAILURE; returns combined output.
fn run_cli_expect_err(config_path: &PathBuf, args: &[&str]) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rcommerce"))
        .arg("--config")
        .arg(config_path)
        .args(args)
        .output()
        .expect("failed to spawn rcommerce CLI");
    assert!(
        !output.status.success(),
        "CLI {:?} unexpectedly succeeded:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

async fn seed_product(pool: &PgPool, title: &str, product_type: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO products (id, title, slug, price, product_type, is_active) \
         VALUES ($1, $2, $3, 10.00, $4::product_type, true)",
    )
    .bind(id)
    .bind(title)
    .bind(format!("e2e-{id}"))
    .bind(product_type)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn variant_id_by_title(pool: &PgPool, product_id: Uuid, title: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM product_variants WHERE product_id = $1 AND title = $2")
        .bind(product_id)
        .bind(title)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_variant_stock(pool: &PgPool, product_id: Uuid, variant_id: Uuid, stock: i32) -> Uuid {
    let location_id = Uuid::new_v4();
    sqlx::query("INSERT INTO inventory_locations (id, name, code) VALUES ($1, 'E2E', $2)")
        .bind(location_id)
        .bind(format!("E2E-{}", &location_id.to_string()[..8]))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO inventory_levels (product_id, variant_id, location_id, available_quantity) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(product_id)
    .bind(variant_id)
    .bind(location_id)
    .bind(stock)
    .execute(pool)
    .await
    .unwrap();
    location_id
}

#[tokio::test]
async fn cli_created_variable_product_is_selectable_and_adds_at_server_price() {
    // --- setup: real DB, real AppState, real routes -------------------------
    let url = test_database_url();
    let pool = match PgPool::connect(&url).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("Skipping variant-options E2E: cannot connect to {url}: {e}");
            return;
        }
    };
    if let Err(e) = rcommerce_core::ensure_test_migrations(&pool).await {
        eprintln!("Skipping variant-options E2E: migration failed: {e}");
        return;
    }
    let state = build_app_state(pool.clone()).await.unwrap();
    let addr = serve(app_router(state)).await;
    let base = format!("http://{addr}");
    let http = client();

    let product_a = seed_product(&pool, "E2E Variable Tee", "variable").await;
    let product_b = seed_product(&pool, "E2E Other Product", "variable").await;
    let config_path = write_cli_config(&url).unwrap();
    let pid_a = product_a.to_string();
    let pid_b = product_b.to_string();

    // --- create options + variants through the REAL CLI --------------------
    run_cli(
        &config_path,
        &[
            "product",
            "add-option",
            &pid_a,
            "--name",
            "Size",
            "--position",
            "0",
        ],
    );
    run_cli(
        &config_path,
        &[
            "product",
            "add-option",
            &pid_a,
            "--name",
            "Color",
            "--position",
            "1",
        ],
    );
    run_cli(
        &config_path,
        &[
            "product",
            "add-variant",
            &pid_a,
            "--title",
            "M / Red",
            "--price",
            "19.99",
            "--inventory",
            "5",
            "--option",
            "Size=M",
            "--option",
            "Color=Red",
        ],
    );
    run_cli(
        &config_path,
        &[
            "product",
            "add-variant",
            &pid_a,
            "--title",
            "L / Blue",
            "--price",
            "24.99",
            "--inventory",
            "5",
            "--option",
            "Size=L",
            "--option",
            "Color=Blue",
        ],
    );
    // A cheap variant on ANOTHER product, for the cross-product refusal test.
    run_cli(
        &config_path,
        &["product", "add-option", &pid_b, "--name", "Size"],
    );
    run_cli(
        &config_path,
        &[
            "product",
            "add-variant",
            &pid_b,
            "--title",
            "B-M",
            "--price",
            "1.00",
            "--inventory",
            "5",
            "--option",
            "Size=M",
        ],
    );

    let listing = run_cli(&config_path, &["product", "variants", &pid_a]);
    assert!(listing.contains("M / Red"), "listing: {listing}");
    assert!(listing.contains("Size=M"), "listing: {listing}");
    assert!(listing.contains("Color=Red"), "listing: {listing}");

    let variant_m = variant_id_by_title(&pool, product_a, "M / Red").await;
    let variant_b = variant_id_by_title(&pool, product_b, "B-M").await;
    let loc_a = seed_variant_stock(&pool, product_a, variant_m, 5).await;
    let loc_b = seed_variant_stock(&pool, product_b, variant_b, 5).await;

    // --- (iv) selectors: product detail exposes options + variants ---------
    let resp = http
        .get(format!("{base}/products/{pid_a}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail: serde_json::Value = resp.json().await.unwrap();
    let product = &detail["product"];
    assert_eq!(product["product_type"], "variable");
    let options = product["options"].as_array().unwrap();
    assert_eq!(
        options.len(),
        2,
        "storefront needs both selectors: {detail}"
    );
    assert_eq!(options[0]["name"], "Size");
    assert_eq!(options[1]["name"], "Color");
    // The Size selector offers M, bound to the M/Red variant.
    let size_values = options[0]["values"].as_array().unwrap();
    let m_value = size_values
        .iter()
        .find(|v| v["value"] == "M")
        .expect("Size=M value present");
    assert_eq!(m_value["variant_id"], variant_m.to_string());
    // The variant carries the server-stored price.
    let variants = product["variants"].as_array().unwrap();
    let api_variant = variants
        .iter()
        .find(|v| v["id"] == variant_m.to_string())
        .expect("M/Red variant present");
    assert_eq!(api_variant["price"], "19.99");

    // --- (iv) cart-add at the server-stored price ---------------------------
    let resp = http
        .post(format!("{base}/carts/guest"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let cart: serde_json::Value = resp.json().await.unwrap();
    let cart_id = cart["cart"]["id"].as_str().unwrap().to_string();

    // Tampered client fields (price, pack_size) must be ignored: the request
    // schema has no such fields and the server uses the stored variant price.
    let resp = http
        .post(format!("{base}/carts/{cart_id}/items"))
        .json(&serde_json::json!({
            "product_id": product_a,
            "variant_id": variant_m,
            "quantity": 2,
            "price": "0.01",
            "pack_size": 999
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "add failed: {}",
        resp.text().await.unwrap()
    );
    let item: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(item["variant_id"], variant_m.to_string());
    // Compare as exact decimals: the cart columns store more decimal places
    // than the variant price, so the JSON scale differs ("19.9900").
    let unit_price: Decimal = item["unit_price"].as_str().unwrap().parse().unwrap();
    let subtotal: Decimal = item["subtotal"].as_str().unwrap().parse().unwrap();
    let total: Decimal = item["total"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        unit_price,
        Decimal::new(1999, 2),
        "server-stored price, not client's"
    );
    assert_eq!(subtotal, Decimal::new(3998, 2));
    assert_eq!(total, Decimal::new(3998, 2));
    assert_eq!(item["quantity"], 2);

    // --- (i) cross-product variant refused at cart-add ----------------------
    let resp = http
        .post(format!("{base}/carts/{cart_id}/items"))
        .json(&serde_json::json!({
            "product_id": product_a,
            "variant_id": variant_b,
            "quantity": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Product variant not found"),
        "body: {body}"
    );

    // A variable product with no variant chosen is refused too.
    let resp = http
        .post(format!("{base}/carts/{cart_id}/items"))
        .json(&serde_json::json!({ "product_id": product_a, "quantity": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Please select an option"));

    // --- duplicate combination refused through the CLI ----------------------
    let out = run_cli_expect_err(
        &config_path,
        &[
            "product",
            "add-variant",
            &pid_a,
            "--title",
            "M / Red copy",
            "--price",
            "19.99",
            "--option",
            "Color=Red",
            "--option",
            "Size=M",
        ],
    );
    assert!(out.contains("Duplicate option combination"), "out: {out}");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM product_variants WHERE product_id = $1")
            .bind(product_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        count, 2,
        "failed CLI create must not leave a partial variant"
    );

    // --- cleanup -------------------------------------------------------------
    sqlx::query("DELETE FROM carts WHERE id = $1")
        .bind(Uuid::parse_str(&cart_id).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_levels WHERE product_id = ANY($1)")
        .bind([product_a, product_b].as_slice())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_locations WHERE id = ANY($1)")
        .bind([loc_a, loc_b].as_slice())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM products WHERE id = ANY($1)")
        .bind([product_a, product_b].as_slice())
        .execute(&pool)
        .await
        .unwrap();
    let _ = std::fs::remove_dir_all(config_path.parent().unwrap());
}

/// Issue #49 E2E: pack pricing is computed server-side at cart-add (client
/// price/pack_size ignored), stock is checked in UNITS inside the atomic cart
/// mutation, variant NULL pack_size inherits the product's, and the CLI
/// validates pack size on every write path.
#[tokio::test]
async fn pack_size_pricing_is_server_side_and_stock_is_in_units() {
    let url = test_database_url();
    let pool = match PgPool::connect(&url).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("Skipping pack-size E2E: cannot connect to {url}: {e}");
            return;
        }
    };
    if let Err(e) = rcommerce_core::ensure_test_migrations(&pool).await {
        eprintln!("Skipping pack-size E2E: migration failed: {e}");
        return;
    }
    let state = build_app_state(pool.clone()).await.unwrap();
    let addr = serve(app_router(state)).await;
    let base = format!("http://{addr}");
    let http = client();
    let config_path = write_cli_config(&url).unwrap();

    // $2.50 per unit, sold in packs of 6, 12 UNITS in stock (= 2 packs).
    let product = seed_product(&pool, "E2E Pack Tea", "simple").await;
    let pid = product.to_string();
    sqlx::query("UPDATE products SET price = 2.50 WHERE id = $1")
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
    let location = Uuid::new_v4();
    sqlx::query("INSERT INTO inventory_locations (id, name, code) VALUES ($1, 'Pack E2E', $2)")
        .bind(location)
        .bind(format!("PACKE2E-{}", &location.to_string()[..8]))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO inventory_levels (product_id, variant_id, location_id, available_quantity) \
         VALUES ($1, NULL, $2, 12)",
    )
    .bind(product)
    .bind(location)
    .execute(&pool)
    .await
    .unwrap();

    // --- CLI pack-size validation + set ------------------------------------
    for bad in ["0", "-1", "1001"] {
        let out = run_cli_expect_err(&config_path, &["product", "set-pack-size", &pid, bad]);
        assert!(
            out.contains("Pack size must be between 1 and 1000"),
            "out: {out}"
        );
    }
    let out = run_cli(&config_path, &["product", "set-pack-size", &pid, "6"]);
    assert!(out.contains("6 units per pack"), "out: {out}");

    // Product detail exposes the pack size for storefront labelling.
    let detail: serde_json::Value = http
        .get(format!("{base}/products/{pid}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["product"]["pack_size"], 6);

    // --- tampered cart-add: server math wins --------------------------------
    let cart: serde_json::Value = http
        .post(format!("{base}/carts/guest"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cart_id = cart["cart"]["id"].as_str().unwrap().to_string();

    let resp = http
        .post(format!("{base}/carts/{cart_id}/items"))
        .json(&serde_json::json!({
            "product_id": product,
            "quantity": 2,
            "price": "0.01",
            "pack_size": 999
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "add failed: {}",
        resp.text().await.unwrap()
    );
    let item: serde_json::Value = resp.json().await.unwrap();
    let item_id = item["id"].as_str().unwrap().to_string();
    let unit_price: Decimal = item["unit_price"].as_str().unwrap().parse().unwrap();
    let subtotal: Decimal = item["subtotal"].as_str().unwrap().parse().unwrap();
    assert_eq!(unit_price, Decimal::new(1500, 2), "2.50 unit × 6 pack");
    assert_eq!(subtotal, Decimal::new(3000, 2), "× 2 packs");
    assert_eq!(item["quantity"], 2);

    // --- stock is in UNITS: a third pack (18 units) is refused --------------
    let resp = http
        .post(format!("{base}/carts/{cart_id}/items"))
        .json(&serde_json::json!({ "product_id": product, "quantity": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Only 12 units"),
        "body: {body}"
    );

    // Quantity update converts too (3 packs = 18 units > 12).
    let resp = http
        .put(format!("{base}/carts/{cart_id}/items/{item_id}"))
        .json(&serde_json::json!({ "quantity": 3 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Only 12 units"));

    // Large quantity is refused, never wraps (checked arithmetic).
    let resp = http
        .post(format!("{base}/carts/{cart_id}/items"))
        .json(&serde_json::json!({ "product_id": product, "quantity": i32::MAX }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // --- variant NULL inherits; explicit value overrides ---------------------
    run_cli(
        &config_path,
        &["product", "add-option", &pid, "--name", "Size"],
    );
    run_cli(
        &config_path,
        &[
            "product",
            "add-variant",
            &pid,
            "--title",
            "Inherits",
            "--price",
            "2.50",
            "--option",
            "Size=M",
        ],
    );
    let out = run_cli_expect_err(
        &config_path,
        &[
            "product",
            "add-variant",
            &pid,
            "--title",
            "Bad pack",
            "--price",
            "2.50",
            "--option",
            "Size=L",
            "--pack-size",
            "1001",
        ],
    );
    assert!(
        out.contains("Pack size must be between 1 and 1000"),
        "out: {out}"
    );
    run_cli(
        &config_path,
        &[
            "product",
            "add-variant",
            &pid,
            "--title",
            "Overrides",
            "--price",
            "2.50",
            "--option",
            "Size=L",
            "--pack-size",
            "2",
        ],
    );
    let v_inherit = variant_id_by_title(&pool, product, "Inherits").await;
    let v_override = variant_id_by_title(&pool, product, "Overrides").await;
    let loc_inherit = seed_variant_stock(&pool, product, v_inherit, 12).await;
    let loc_override = seed_variant_stock(&pool, product, v_override, 12).await;

    let detail: serde_json::Value = http
        .get(format!("{base}/products/{pid}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let variants = detail["product"]["variants"].as_array().unwrap();
    let api_inherit = variants
        .iter()
        .find(|v| v["id"] == v_inherit.to_string())
        .unwrap();
    let api_override = variants
        .iter()
        .find(|v| v["id"] == v_override.to_string())
        .unwrap();
    assert!(
        api_inherit["pack_size"].is_null(),
        "NULL = inherit: {detail}"
    );
    assert_eq!(api_override["pack_size"], 2);

    // Inheriting variant: 2.50 × 6 = 15.00 per pack.
    let resp = http
        .post(format!("{base}/carts/{cart_id}/items"))
        .json(&serde_json::json!({ "product_id": product, "variant_id": v_inherit, "quantity": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "add failed: {}",
        resp.text().await.unwrap()
    );
    let item: serde_json::Value = resp.json().await.unwrap();
    let unit_price: Decimal = item["unit_price"].as_str().unwrap().parse().unwrap();
    assert_eq!(unit_price, Decimal::new(1500, 2));

    // Overriding variant: 2.50 × 2 = 5.00 per pack.
    let resp = http
        .post(format!("{base}/carts/{cart_id}/items"))
        .json(
            &serde_json::json!({ "product_id": product, "variant_id": v_override, "quantity": 1 }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "add failed: {}",
        resp.text().await.unwrap()
    );
    let item: serde_json::Value = resp.json().await.unwrap();
    let unit_price: Decimal = item["unit_price"].as_str().unwrap().parse().unwrap();
    assert_eq!(unit_price, Decimal::new(500, 2));

    // --- cleanup -------------------------------------------------------------
    sqlx::query("DELETE FROM carts WHERE id = $1")
        .bind(Uuid::parse_str(&cart_id).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_levels WHERE product_id = $1")
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_locations WHERE id = ANY($1)")
        .bind([location, loc_inherit, loc_override].as_slice())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
    let _ = std::fs::remove_dir_all(config_path.parent().unwrap());
}
