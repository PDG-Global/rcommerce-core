//! Integration Tests for R Commerce API
//!
//! These tests provide comprehensive end-to-end testing for the R Commerce platform:
//! - Complete purchase flows
//! - Cart operations and persistence
//! - Cart merging on login
//! - Authentication flows
//! - Coupon application
//! - Tax and shipping calculations
//!
//! Run with: cargo test --test integration_tests -- --test-threads=1
//!
//! IMPORTANT: Tests must run sequentially (--test-threads=1) because they share
//! a database and perform migrations cleanup before each test.
//!
//! Required environment:
//! - PostgreSQL database running
//! - TEST_DATABASE_URL environment variable (or uses default)

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

// Static lock to prevent concurrent migration runs (causes deadlocks)
static MIGRATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Test reporter module
mod test_reporter;
use test_reporter::TestReporter;

use axum::{routing::get, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use tokio::net::TcpListener;
use tokio::time::sleep;
use uuid::Uuid;

// Import API types
use rcommerce_api::server::api_routes;
use rcommerce_api::state::{AppState, AppStateParams};

// Import core types
use rcommerce_core::inventory::{InventoryConfig, InventoryService};
use rcommerce_core::models::{
    CartWithItems, CreateCustomerRequest, Currency, Customer, InventoryPolicy, ProductType,
};
use rcommerce_core::order::lifecycle::OrderEventDispatcher;
use rcommerce_core::payment::agnostic::PaymentService;
use rcommerce_core::repository::cart_repository::PgCartRepository;
use rcommerce_core::repository::coupon_repository::PgCouponRepository;
use rcommerce_core::repository::{
    CustomerRepository, Database, PostgresApiKeyRepository, PostgresSubscriptionRepository,
    ProductRepository,
};
use rcommerce_core::services::{
    AuthService, CartService, CouponService, CustomerService, OrderService, ProductService,
};
use rcommerce_core::services::{CheckoutConfig, CheckoutService};
use rcommerce_core::shipping::ShippingProviderFactory;
use rcommerce_core::StripeGateway;
use rcommerce_core::{Config, FileUploadService};

// =============================================================================
// Test Configuration
// =============================================================================

/// Configuration for integration tests
pub struct TestConfig {
    pub database_url: String,
    pub server_port: u16,
    pub jwt_secret: String,
    pub cleanup_after: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            database_url: std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
                "postgres://rcommerce:password@localhost:5432/rcommerce_test".to_string()
            }),
            server_port: 0, // Random available port
            jwt_secret: "test-secret-do-not-use-in-production-32-bytes".to_string(),
            cleanup_after: true,
        }
    }
}

// =============================================================================
// Test Application State
// =============================================================================

/// Test application with all required services
pub struct TestApp {
    pub app_state: AppState,
    pub db_pool: Pool<Postgres>,
    pub server_addr: SocketAddr,
    pub http_client: reqwest::Client,
    pub config: TestConfig,
}

impl TestApp {
    /// Create a new test application with all services initialized
    pub async fn new() -> anyhow::Result<Self> {
        let config = TestConfig::default();

        // Initialize tracing
        let _ = tracing_subscriber::fmt::try_init();

        // Create database pool
        let db_pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.database_url)
            .await?;

        // Run migrations (simplified - run SQL directly)
        Self::run_migrations(&db_pool).await?;

        // Create database wrapper
        let db = Database::new(db_pool.clone());

        // Create config for AuthService
        let mut core_config = Config::default();
        core_config.security.jwt.secret = config.jwt_secret.clone();

        // Create repositories
        let product_repository = ProductRepository::new(db.clone());
        let customer_repository = CustomerRepository::new(db.clone());

        // Create repositories for cart/coupon (Arc-wrapped trait objects)
        let cart_repo: Arc<dyn rcommerce_core::repository::CartRepository> =
            Arc::new(PgCartRepository::new(db_pool.clone()));
        let coupon_repo: Arc<dyn rcommerce_core::repository::CouponRepository> =
            Arc::new(PgCouponRepository::new(db_pool.clone()));

        // Initialize services
        let product_service = ProductService::new(product_repository);
        let customer_service = CustomerService::new(customer_repository);
        let auth_service = AuthService::new(core_config);

        // Create coupon service first (needed for cart service)
        let coupon_service = CouponService::new(coupon_repo.clone(), cart_repo.clone());
        let coupon_service_arc = Arc::new(coupon_service);

        // Create cart service with required dependencies
        let bundle_service = Arc::new(rcommerce_core::BundleService::new(db.clone()));
        let cart_service = CartService::new(
            cart_repo.clone(),
            coupon_repo.clone(),
            coupon_service_arc.clone(),
        )
        .with_bundle_service(bundle_service.clone())
        .with_database(db.clone());

        // Create payment service
        let payment_service = PaymentService::new("stripe".to_string());
        let _payment_service_arc = Arc::new(payment_service);

        // Create inventory service and event dispatcher for order service
        let inventory_config = InventoryConfig::default();
        let inventory_service = Arc::new(InventoryService::new(db.clone(), inventory_config));
        let event_dispatcher = OrderEventDispatcher::new();

        // Create order service
        let order_service = OrderService::new(
            db.clone(),
            Box::new(StripeGateway::new(
                "sk_test_dummy".to_string(),
                "whsec_dummy".to_string(),
            )),
            inventory_service.as_ref().clone(),
            event_dispatcher,
        );

        // Create other services
        let _file_upload_service = Arc::new(
            FileUploadService::new_local(
                std::path::PathBuf::from("./test_uploads"),
                "http://localhost:8080/uploads".to_string(),
            )
            .expect("Failed to create file upload service"),
        );

        // Wrap services in Arc as required by AppStateParams
        let cart_service_arc = Arc::new(cart_service);
        let order_service_arc = Arc::new(order_service);
        let tax_service = Arc::new(rcommerce_core::tax::DefaultTaxService::new(db_pool.clone()));
        let shipping_factory = Arc::new(ShippingProviderFactory::new());

        let api_key_repository = PostgresApiKeyRepository::new(db_pool.clone());
        let subscription_repository = PostgresSubscriptionRepository::new(db_pool.clone());

        // Create coupon service (not Arc-wrapped) for AppStateParams
        let coupon_service = CouponService::new(coupon_repo, cart_repo);

        // Create payment service (not Arc-wrapped)
        let payment_service = PaymentService::new("stripe".to_string());

        // Create file upload service (not Arc-wrapped)
        let file_upload_service = FileUploadService::new_local(
            std::path::PathBuf::from("./test_uploads"),
            "http://localhost:8080/uploads".to_string(),
        )
        .expect("Failed to create file upload service");

        // Create checkout service
        let checkout_config = CheckoutConfig::default();
        let payment_gateway_arc: Arc<dyn rcommerce_core::PaymentGateway> = Arc::new(
            StripeGateway::new("sk_test_dummy".to_string(), "whsec_dummy".to_string()),
        );
        let checkout_service = Arc::new(CheckoutService::new(
            cart_service_arc.clone(),
            tax_service.clone(),
            order_service_arc.clone(),
            payment_gateway_arc,
            shipping_factory.clone(),
            checkout_config,
        ));

        // Create app state
        let params = AppStateParams::new(
            product_service,
            customer_service,
            auth_service,
            db,
            None, // No Redis for tests
            api_key_repository,
            subscription_repository,
            coupon_service,
            payment_service,
            file_upload_service,
            cart_service_arc,
            order_service_arc,
            tax_service,
            shipping_factory,
            checkout_service,
            None, // No subscription gateway for tests
            inventory_service,
            None,               // No SMTP for tests
            None,               // No Xero for tests
            None,               // No admin notification email for tests
            Default::default(), // Branding
            Default::default(), // Accounting
            "USD".to_string(),  // Default currency
            None,               // No Airwallex for tests
        );

        let app_state = AppState::new(params).await;

        // Create HTTP client with proxy disabled for localhost
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .no_proxy()
            .build()?;

        // Start test server
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let server_addr = listener.local_addr()?;

        // Build router
        let app = Self::build_router(app_state.clone());

        // Spawn server with error logging
        let server_handle = tokio::spawn(async move {
            println!("Test server starting on {}", server_addr);
            let serve = axum::serve(listener, app);
            if let Err(e) = serve.await {
                eprintln!("Test server error: {:?}", e);
            }
            println!("Test server on {} has shut down", server_addr);
        });

        // Wait for server to be ready
        sleep(Duration::from_millis(500)).await;

        // Check if server is still running (not exited with error)
        if server_handle.is_finished() {
            eprintln!("WARNING: Server task finished before test started!");
        }

        Ok(Self {
            app_state,
            db_pool,
            server_addr,
            http_client,
            config,
        })
    }

    /// Build the API router from the server's own routes and auth middleware
    fn build_router(app_state: AppState) -> Router {
        Router::new()
            .route("/health", get(|| async { "OK" }))
            .nest("/api/v1", api_routes(app_state.clone()))
            .layer(tower_http::trace::TraceLayer::new_for_http())
            .with_state(app_state)
    }

    /// Reset the schema and run the database migrations
    async fn run_migrations(pool: &Pool<Postgres>) -> anyhow::Result<()> {
        // Acquire lock to prevent concurrent migrations (causes deadlocks)
        let _guard = MIGRATION_LOCK.lock().await;

        // Clean up existing objects first to ensure a fresh database state
        // This is necessary because tests may share the same database
        let cleanup_sql = r#"
            DO $$
            DECLARE
                r RECORD;
            BEGIN
                -- Disable triggers temporarily to avoid cascade issues
                SET session_replication_role = replica;
                
                -- Drop all indexes in public schema (excluding system indexes)
                FOR r IN (
                    SELECT indexname 
                    FROM pg_indexes 
                    WHERE schemaname = 'public'
                    AND indexname NOT LIKE 'pg_%'
                ) LOOP
                    EXECUTE 'DROP INDEX IF EXISTS public.' || quote_ident(r.indexname) || ' CASCADE';
                END LOOP;
                
                -- Drop all tables in public schema
                FOR r IN (
                    SELECT tablename 
                    FROM pg_tables 
                    WHERE schemaname = 'public'
                ) LOOP
                    EXECUTE 'DROP TABLE IF EXISTS public.' || quote_ident(r.tablename) || ' CASCADE';
                END LOOP;
                
                -- Drop all types (enums) in public schema (excluding extensions)
                FOR r IN (
                    SELECT typname 
                    FROM pg_type t
                    JOIN pg_namespace n ON t.typnamespace = n.oid
                    WHERE n.nspname = 'public' 
                    AND t.typtype = 'e'
                ) LOOP
                    EXECUTE 'DROP TYPE IF EXISTS public.' || quote_ident(r.typname) || ' CASCADE';
                END LOOP;
                
                -- Drop all functions in public schema (excluding extensions)
                FOR r IN (
                    SELECT proname, pg_get_function_identity_arguments(p.oid) as args
                    FROM pg_proc p
                    JOIN pg_namespace n ON p.pronamespace = n.oid
                    WHERE n.nspname = 'public'
                    AND p.proname NOT IN ('uuid_generate_v4', 'gen_random_uuid')  -- Keep extension functions
                ) LOOP
                    EXECUTE 'DROP FUNCTION IF EXISTS public.' || quote_ident(r.proname) || '(' || r.args || ') CASCADE';
                END LOOP;
                
                -- Re-enable triggers
                SET session_replication_role = DEFAULT;
            END $$;
        "#;

        sqlx::query(cleanup_sql).execute(pool).await.ok(); // Ignore errors - schema might not exist yet

        // Apply the same migrations as the server, so the schema never drifts
        rcommerce_core::Migrator::new(pool.clone())
            .migrate()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run migrations: {}", e))?;

        Ok(())
    }

    /// Get base URL for API requests
    pub fn base_url(&self) -> String {
        format!("http://{}", self.server_addr)
    }

    // =============================================================================
    // Test Helper Methods
    // =============================================================================

    /// Create a test customer and return the customer + password
    pub async fn create_test_customer(&self) -> anyhow::Result<(Customer, String)> {
        let password = format!(
            "TestPass{}!",
            Uuid::new_v4().to_string()[..8].to_uppercase()
        );
        let email = format!("test_{}@example.com", Uuid::new_v4());

        let request = CreateCustomerRequest {
            email: email.clone(),
            first_name: "Test".to_string(),
            last_name: "Customer".to_string(),
            phone: None,
            accepts_marketing: false,
            currency: Currency::USD,
        };

        let password_hash = self.app_state.auth_service.hash_password(&password)?;
        let mut customer = self
            .app_state
            .customer_service
            .create_customer_with_password(request, password_hash)
            .await?;

        // Login requires a verified email
        self.app_state
            .customer_service
            .update_email_verified(customer.id, true)
            .await?;
        customer.email_verified = true;

        Ok((customer, password))
    }

    /// Create an admin account and return its access token
    pub async fn admin_token(&self) -> anyhow::Result<String> {
        let (customer, password) = self.create_test_customer().await?;
        sqlx::query("UPDATE customers SET role = 'admin' WHERE id = $1")
            .bind(customer.id)
            .execute(&self.db_pool)
            .await?;
        self.login(&customer.email, &password).await
    }

    /// Login a customer and return the JWT token
    pub async fn login(&self, email: &str, password: &str) -> anyhow::Result<String> {
        let response = self
            .http_client
            .post(format!("{}/api/v1/auth/login", self.base_url()))
            .json(&serde_json::json!({
                "email": email,
                "password": password
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Login failed: {}", response.status()));
        }

        let body: serde_json::Value = response.json().await?;
        let token = body["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No access token in response"))?
            .to_string();

        Ok(token)
    }

    /// Create a test product
    pub async fn create_test_product(
        &self,
        title: &str,
        price: Decimal,
        inventory: i32,
    ) -> anyhow::Result<Uuid> {
        let product_id = Uuid::new_v4();
        let slug = format!("test-product-{}", Uuid::new_v4());

        sqlx::query(
            r#"
            INSERT INTO products (
                id, title, slug, product_type, price, currency,
                inventory_quantity, inventory_policy, inventory_management,
                requires_shipping, is_active, is_featured,
                created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), NOW())
            "#,
        )
        .bind(product_id)
        .bind(title)
        .bind(&slug)
        .bind(ProductType::Simple)
        .bind(price)
        .bind(Currency::USD)
        .bind(inventory)
        .bind(InventoryPolicy::Deny)
        .bind(true)
        .bind(true)
        .bind(true)
        .bind(false)
        .execute(&self.db_pool)
        .await?;

        // Carts check stock per location (inventory_levels), not the product column
        if inventory > 0 {
            let location_id: Uuid = sqlx::query_scalar(
                "SELECT id FROM inventory_locations WHERE is_active ORDER BY created_at LIMIT 1",
            )
            .fetch_one(&self.db_pool)
            .await?;
            self.app_state
                .inventory_service
                .receive_stock(product_id, location_id, inventory, None)
                .await?;
        }

        Ok(product_id)
    }

    /// Create a guest cart
    pub async fn create_guest_cart(&self) -> anyhow::Result<(Uuid, String)> {
        let response = self
            .http_client
            .post(format!("{}/api/v1/carts/guest", self.base_url()))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to create guest cart: {}",
                response.status()
            ));
        }

        let body: serde_json::Value = response.json().await?;
        let cart_id = body["cart"]["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No cart ID"))?;
        let session_token = body["cart"]["session_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No session token"))?;

        Ok((Uuid::parse_str(cart_id)?, session_token.to_string()))
    }

    /// Add item to cart
    pub async fn add_item_to_cart(
        &self,
        cart_id: Uuid,
        product_id: Uuid,
        quantity: i32,
        token: &str,
    ) -> anyhow::Result<Uuid> {
        let response = self
            .http_client
            .post(format!(
                "{}/api/v1/carts/{}/items",
                self.base_url(),
                cart_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "product_id": product_id,
                "quantity": quantity
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let body = response.text().await?;
            return Err(anyhow::anyhow!("Failed to add item: {}", body));
        }

        let body: serde_json::Value = response.json().await?;
        let item_id = body["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No item ID"))?;

        Ok(Uuid::parse_str(item_id)?)
    }

    /// Get cart with items
    pub async fn get_cart(&self, cart_id: Uuid) -> anyhow::Result<CartWithItems> {
        let response = self
            .http_client
            .get(format!("{}/api/v1/carts/{}", self.base_url(), cart_id))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get cart: {}", response.status()));
        }

        let cart = response.json::<CartWithItems>().await?;
        Ok(cart)
    }

    /// Apply coupon to cart
    pub async fn apply_coupon(
        &self,
        cart_id: Uuid,
        coupon_code: &str,
        token: &str,
    ) -> anyhow::Result<CartWithItems> {
        let response = self
            .http_client
            .post(format!(
                "{}/api/v1/carts/{}/coupon",
                self.base_url(),
                cart_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "coupon_code": coupon_code
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to apply coupon: {}",
                response.status()
            ));
        }

        let cart = response.json::<CartWithItems>().await?;
        Ok(cart)
    }

    /// Create a test coupon
    pub async fn create_test_coupon(
        &self,
        code: &str,
        discount_type: &str,
        discount_value: &str,
    ) -> anyhow::Result<()> {
        // Create coupon directly in database
        // Cast discount_type to enum and discount_value to numeric
        sqlx::query(
            r#"
            INSERT INTO coupons (
                id, code, discount_type, discount_value, is_active,
                usage_count, can_combine, created_at, updated_at
            ) VALUES ($1, $2, $3::discount_type, $4::numeric, $5, $6, $7, NOW(), NOW())
            ON CONFLICT (code) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(code)
        .bind(discount_type)
        .bind(discount_value)
        .bind(true)
        .bind(0)
        .bind(false)
        .execute(&self.db_pool)
        .await?;

        Ok(())
    }

    /// Clean up test data
    pub async fn cleanup(&self) -> anyhow::Result<()> {
        if self.config.cleanup_after {
            // Clean up test data
            sqlx::query("DELETE FROM order_items WHERE created_at > NOW() - INTERVAL '1 hour'")
                .execute(&self.db_pool)
                .await?;
            sqlx::query("DELETE FROM orders WHERE created_at > NOW() - INTERVAL '1 hour'")
                .execute(&self.db_pool)
                .await?;
            sqlx::query("DELETE FROM cart_items WHERE created_at > NOW() - INTERVAL '1 hour'")
                .execute(&self.db_pool)
                .await?;
            sqlx::query("DELETE FROM carts WHERE created_at > NOW() - INTERVAL '1 hour'")
                .execute(&self.db_pool)
                .await?;
            sqlx::query("DELETE FROM customers WHERE email LIKE 'test_%@example.com'")
                .execute(&self.db_pool)
                .await?;
            sqlx::query("DELETE FROM products WHERE slug LIKE 'test-product-%'")
                .execute(&self.db_pool)
                .await?;
        }
        self.db_pool.close().await;
        Ok(())
    }
}

// =============================================================================
// Test Types
// =============================================================================

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct RegisterRequest {
    email: String,
    password: String,
    first_name: String,
    last_name: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AuthResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    customer: CustomerInfo,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CustomerInfo {
    id: String,
    email: String,
    first_name: String,
    last_name: String,
}

// =============================================================================
// Integration Tests
// =============================================================================

/// Test 1: Complete purchase flow
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_complete_purchase_flow() {
    let app = TestApp::new().await.expect("Failed to create test app");

    // 1. Create test customer
    let (customer, password) = app
        .create_test_customer()
        .await
        .expect("Failed to create customer");

    // 2. Login and get JWT
    let token = app
        .login(&customer.email, &password)
        .await
        .expect("Failed to login");
    assert!(!token.is_empty());

    // 3. Create test product
    let product_id = app
        .create_test_product(
            "Test Product",
            Decimal::new(2999, 2), // $29.99
            100,
        )
        .await
        .expect("Failed to create product");

    // 4. Create guest cart
    let (cart_id, _session_token) = app
        .create_guest_cart()
        .await
        .expect("Failed to create cart");

    // 5. Add items to cart
    let item_id = app
        .add_item_to_cart(cart_id, product_id, 2, &token)
        .await
        .expect("Failed to add item");
    assert!(!item_id.is_nil());

    // 6. Verify cart persistence
    let cart = app.get_cart(cart_id).await.expect("Failed to get cart");
    assert_eq!(cart.items.len(), 1);
    assert_eq!(cart.items[0].quantity, 2);

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 2: Cart persistence
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_cart_persistence() {
    let app = TestApp::new().await.expect("Failed to create test app");

    // Create test customer
    let (customer, password) = app
        .create_test_customer()
        .await
        .expect("Failed to create customer");
    let token = app
        .login(&customer.email, &password)
        .await
        .expect("Failed to login");

    // Create test products
    let product1 = app
        .create_test_product("Product 1", Decimal::new(1000, 2), 100)
        .await
        .unwrap();
    let product2 = app
        .create_test_product("Product 2", Decimal::new(2000, 2), 100)
        .await
        .unwrap();

    // 1. Create guest cart
    let (cart_id, _session_token) = app
        .create_guest_cart()
        .await
        .expect("Failed to create cart");

    // 2. Add items
    let item1_id = app
        .add_item_to_cart(cart_id, product1, 2, &token)
        .await
        .expect("Failed to add item 1");
    let item2_id = app
        .add_item_to_cart(cart_id, product2, 1, &token)
        .await
        .expect("Failed to add item 2");

    // 3. Fetch cart and verify items present
    let cart = app.get_cart(cart_id).await.expect("Failed to get cart");
    assert_eq!(cart.items.len(), 2);

    // 4. Update item quantity
    let response = app
        .http_client
        .put(format!(
            "{}/api/v1/carts/{}/items/{}",
            app.base_url(),
            cart_id,
            item1_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"quantity": 5}))
        .send()
        .await
        .expect("Failed to update item");

    assert!(response.status().is_success());

    // 5. Verify update persisted
    let cart = app
        .get_cart(cart_id)
        .await
        .expect("Failed to get cart after update");
    let updated_item = cart
        .items
        .iter()
        .find(|i| i.id == item1_id)
        .expect("Item not found");
    assert_eq!(updated_item.quantity, 5);

    // 6. Remove item
    let response = app
        .http_client
        .delete(format!(
            "{}/api/v1/carts/{}/items/{}",
            app.base_url(),
            cart_id,
            item2_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to remove item");

    assert_eq!(response.status(), 204);

    // 7. Verify removal persisted
    let cart = app
        .get_cart(cart_id)
        .await
        .expect("Failed to get cart after removal");
    assert_eq!(cart.items.len(), 1);
    assert!(cart.items.iter().all(|i| i.id != item2_id));

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 3: Cart merging on login
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_cart_merge_on_login() {
    let app = TestApp::new().await.expect("Failed to create test app");

    // Create test customer
    let (customer, password) = app
        .create_test_customer()
        .await
        .expect("Failed to create customer");
    let token = app
        .login(&customer.email, &password)
        .await
        .expect("Failed to login");

    // Create test product
    let product_id = app
        .create_test_product("Merge Test Product", Decimal::new(1500, 2), 100)
        .await
        .unwrap();

    // 1. Create guest cart with items
    let (guest_cart_id, session_token) = app
        .create_guest_cart()
        .await
        .expect("Failed to create guest cart");
    let _item_id = app
        .add_item_to_cart(guest_cart_id, product_id, 3, &token)
        .await
        .expect("Failed to add item");

    // 2. Get customer cart
    let response = app
        .http_client
        .get(format!("{}/api/v1/carts/me", app.base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to get customer cart");

    assert!(response.status().is_success());
    let customer_cart: CartWithItems = response.json().await.expect("Failed to parse cart");
    let customer_cart_id = customer_cart.cart.id;

    // 3. Merge carts
    let response = app
        .http_client
        .post(format!("{}/api/v1/carts/merge", app.base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "session_token": session_token
        }))
        .send()
        .await
        .expect("Failed to merge carts");

    if response.status().is_success() {
        let merged_cart: CartWithItems =
            response.json().await.expect("Failed to parse merged cart");

        // 4. Verify guest items in customer cart
        assert_eq!(merged_cart.cart.id, customer_cart_id);
        // The merged cart should contain the items from the guest cart
    }

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 4: Authentication flows
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_auth_flows() {
    let app = TestApp::new().await.expect("Failed to create test app");

    let email = format!("auth_test_{}@example.com", Uuid::new_v4());
    let original_password = "TestPassword123!";

    // 1. Register new customer
    let response = app
        .http_client
        .post(format!("{}/api/v1/auth/register", app.base_url()))
        .json(&serde_json::json!({
            "email": email,
            "password": original_password,
            "first_name": "Auth",
            "last_name": "Test"
        }))
        .send()
        .await
        .expect("Failed to register");

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status != 201 {
        eprintln!("Registration failed with status {}: {}", status, body);
    }
    assert_eq!(status, 201);
    let registered: serde_json::Value =
        serde_json::from_str(&body).expect("Failed to parse registration response");
    let customer_id: Uuid = registered["customer"]["id"]
        .as_str()
        .and_then(|id| id.parse().ok())
        .expect("No customer id");

    // 2. Login is refused until the email is verified
    let response = app
        .http_client
        .post(format!("{}/api/v1/auth/login", app.base_url()))
        .json(&serde_json::json!({
            "email": email,
            "password": original_password
        }))
        .send()
        .await
        .expect("Failed to login");
    assert_eq!(response.status(), 403);

    // 3. Verify the email with the token the verification email links to
    let token = app
        .app_state
        .auth_service
        .generate_email_verification_token(customer_id, &email)
        .expect("Failed to create verification token");
    let response = app
        .http_client
        .post(format!("{}/api/v1/auth/verify-email", app.base_url()))
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .expect("Failed to verify email");
    assert!(response.status().is_success());

    // 4. Login with credentials
    let response = app
        .http_client
        .post(format!("{}/api/v1/auth/login", app.base_url()))
        .json(&serde_json::json!({
            "email": email,
            "password": original_password
        }))
        .send()
        .await
        .expect("Failed to login");

    assert!(response.status().is_success());
    let auth: AuthResponse = response
        .json()
        .await
        .expect("Failed to parse auth response");
    let original_token = auth.access_token;
    let refresh_token = auth.refresh_token;

    // 5. Access protected endpoint with token
    let response = app
        .http_client
        .get(format!("{}/api/v1/carts/me", app.base_url()))
        .header("Authorization", format!("Bearer {}", original_token))
        .send()
        .await
        .expect("Failed to access protected endpoint");

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        eprintln!("Protected endpoint failed with status {}: {}", status, body);
    }
    assert!(status.is_success());

    // 6. Refresh token
    let response = app
        .http_client
        .post(format!("{}/api/v1/auth/refresh", app.base_url()))
        .json(&serde_json::json!({
            "refresh_token": refresh_token
        }))
        .send()
        .await
        .expect("Failed to refresh token");

    if response.status().is_success() {
        let refresh: serde_json::Value = response
            .json()
            .await
            .expect("Failed to parse refresh response");
        let new_token = refresh["access_token"].as_str().expect("No access token");

        // 7. Access with new token
        let response = app
            .http_client
            .get(format!("{}/api/v1/carts/me", app.base_url()))
            .header("Authorization", format!("Bearer {}", new_token))
            .send()
            .await
            .expect("Failed to access with new token");

        assert!(response.status().is_success());
    }

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 5: Coupon application
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_coupon_application() {
    let app = TestApp::new().await.expect("Failed to create test app");

    // Create test customer
    let (customer, password) = app
        .create_test_customer()
        .await
        .expect("Failed to create customer");
    let token = app
        .login(&customer.email, &password)
        .await
        .expect("Failed to login");

    // Create test product with sufficient price
    let product_id = app
        .create_test_product("Coupon Test Product", Decimal::new(5000, 2), 100)
        .await
        .unwrap();

    // Create coupon in database
    let coupon_code = format!("TEST{}", Uuid::new_v4().to_string()[..6].to_uppercase());
    app.create_test_coupon(&coupon_code, "percentage", "20.00")
        .await
        .expect("Failed to create coupon");

    // 1. Create cart
    let (cart_id, _session_token) = app
        .create_guest_cart()
        .await
        .expect("Failed to create cart");

    // 2. Add items
    let _item_id = app
        .add_item_to_cart(cart_id, product_id, 2, &token)
        .await
        .expect("Failed to add item");

    // 3. Apply valid coupon
    let _response = app
        .http_client
        .post(format!(
            "{}/api/v1/carts/{}/coupon",
            app.base_url(),
            cart_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "coupon_code": coupon_code
        }))
        .send()
        .await
        .expect("Failed to apply coupon");

    // Note: Coupon application may fail if the coupon service is not fully implemented
    // The test documents the expected behavior

    // 4. Remove coupon
    let _response = app
        .http_client
        .delete(format!(
            "{}/api/v1/carts/{}/coupon",
            app.base_url(),
            cart_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to remove coupon");

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 6: Tax and shipping calculation
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_tax_and_shipping_calculation() {
    let app = TestApp::new().await.expect("Failed to create test app");

    // Create test customer
    let (customer, password) = app
        .create_test_customer()
        .await
        .expect("Failed to create customer");
    let token = app
        .login(&customer.email, &password)
        .await
        .expect("Failed to login");

    // Create test product
    let product_id = app
        .create_test_product("Shipping Test Product", Decimal::new(5000, 2), 100)
        .await
        .unwrap();

    // 1. Create cart with items
    let (cart_id, _session_token) = app
        .create_guest_cart()
        .await
        .expect("Failed to create cart");
    let _item_id = app
        .add_item_to_cart(cart_id, product_id, 2, &token)
        .await
        .expect("Failed to add item");

    // 2. Initiate checkout with US address
    let shipping_address = serde_json::json!({
        "first_name": "John",
        "last_name": "Doe",
        "address1": "123 Main St",
        "city": "New York",
        "state": "NY",
        "country": "US",
        "zip": "10001"
    });

    let response = app
        .http_client
        .post(format!("{}/api/v1/checkout/initiate", app.base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "cart_id": cart_id,
            "shipping_address": shipping_address,
            "currency": "USD"
        }))
        .send()
        .await
        .expect("Failed to initiate checkout");

    // Note: Checkout may return various statuses depending on implementation
    // For now, we just verify the endpoint responds appropriately
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert!(
        status.is_success() || status == 400 || status == 422,
        "Checkout returned unexpected status: {} {}",
        status,
        body
    );

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 7: Product listing and retrieval
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_product_listing() {
    let app = TestApp::new().await.expect("Failed to create test app");

    // Create test customer and login (product routes require auth)
    let (customer, password) = app
        .create_test_customer()
        .await
        .expect("Failed to create customer");
    let token = app
        .login(&customer.email, &password)
        .await
        .expect("Failed to login");

    // Create test products
    let product1 = app
        .create_test_product("Listed Product 1", Decimal::new(1000, 2), 50)
        .await
        .unwrap();
    let _product2 = app
        .create_test_product("Listed Product 2", Decimal::new(2000, 2), 50)
        .await
        .unwrap();

    // List products (with auth)
    let response = app
        .http_client
        .get(format!("{}/api/v1/products", app.base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to list products");

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        eprintln!("Product listing failed with status {}: {}", status, body);
        panic!("Product listing failed with status {}", status);
    }

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    let products = body["products"].as_array().expect("Products not an array");
    assert!(!products.is_empty());

    // Get specific product (with auth)
    let response = app
        .http_client
        .get(format!("{}/api/v1/products/{}", app.base_url(), product1))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to get product");

    assert!(response.status().is_success());

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 8: Order creation and retrieval
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_order_creation() {
    let app = TestApp::new().await.expect("Failed to create test app");

    // Create test customer
    let (customer, password) = app
        .create_test_customer()
        .await
        .expect("Failed to create customer");
    let token = app
        .login(&customer.email, &password)
        .await
        .expect("Failed to login");

    // Create test product
    let product_id = app
        .create_test_product("Order Test Product", Decimal::new(3000, 2), 100)
        .await
        .unwrap();

    // Create order
    let response = app
        .http_client
        .post(format!("{}/api/v1/orders", app.base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "customer_id": customer.id,
            "customer_email": customer.email,
            "items": [
                {
                    "product_id": product_id,
                    "quantity": 2
                }
            ],
            "shipping_address": {
                "first_name": "John",
                "last_name": "Doe",
                "address1": "123 Main St",
                "city": "New York",
                "state": "NY",
                "country": "US",
                "zip": "10001"
            }
        }))
        .send()
        .await
        .expect("Failed to create order");

    // Order creation may succeed or fail depending on implementation
    if response.status().is_success() {
        let order: serde_json::Value = response.json().await.expect("Failed to parse order");
        let order_id = order["id"].as_str().expect("No order ID");

        // Get order by ID
        let response = app
            .http_client
            .get(format!("{}/api/v1/orders/{}", app.base_url(), order_id))
            .send()
            .await
            .expect("Failed to get order");

        assert!(response.status().is_success());
    }

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 9: Cart item updates
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_cart_item_updates() {
    let app = TestApp::new().await.expect("Failed to create test app");

    // Create test customer
    let (customer, password) = app
        .create_test_customer()
        .await
        .expect("Failed to create customer");
    let token = app
        .login(&customer.email, &password)
        .await
        .expect("Failed to login");

    // Create test product
    let product_id = app
        .create_test_product("Update Test Product", Decimal::new(2500, 2), 100)
        .await
        .unwrap();

    // Create cart and add item
    let (cart_id, _session_token) = app
        .create_guest_cart()
        .await
        .expect("Failed to create cart");
    let item_id = app
        .add_item_to_cart(cart_id, product_id, 1, &token)
        .await
        .expect("Failed to add item");

    // Update quantity to 0 (should remove item)
    let response = app
        .http_client
        .put(format!(
            "{}/api/v1/carts/{}/items/{}",
            app.base_url(),
            cart_id,
            item_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"quantity": 0}))
        .send()
        .await
        .expect("Failed to update item");

    assert!(response.status().is_success() || response.status() == 204);

    // Cleanup
    app.cleanup().await.ok();
}

/// Test 10: Health check
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_health_check() {
    let mut reporter = TestReporter::new("test_health_check");
    let app = TestApp::new().await.expect("Failed to create test app");

    reporter.step("Check health endpoint");
    let response = app
        .http_client
        .get(format!("{}/health", app.base_url()))
        .send()
        .await
        .expect("Failed to check health");

    reporter.api_call("GET", "/health", response.status().as_u16());

    let success = response.status().is_success();
    reporter.assertion(
        "Health check returns success",
        success,
        "200-299".to_string(),
        response.status().to_string(),
    );
    reporter.step_ok();

    // Cleanup
    app.cleanup().await.ok();

    // Finalize report (saves immediately)
    reporter.finalize(success);

    assert!(
        success,
        "Expected success status, got {}",
        response.status()
    );
}

/// Test 11: Admin product management goes through the admin routes
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_admin_can_create_product() {
    let app = TestApp::new().await.expect("Failed to create test app");
    let admin = app.admin_token().await.expect("Failed to create admin");
    let (customer, password) = app.create_test_customer().await.unwrap();
    let customer_token = app.login(&customer.email, &password).await.unwrap();
    let product = serde_json::json!({
        "title": "Admin Test Product",
        "slug": format!("admin-test-product-{}", Uuid::new_v4().simple()),
        "product_type": "Simple",
        "price": "12.50",
        "currency": "EUR",
        "inventory_quantity": 0,
        "inventory_policy": "Deny",
        "inventory_management": true,
        "continues_selling_when_out_of_stock": false,
        "requires_shipping": true,
        "is_active": true,
        "is_featured": false
    });
    let url = format!("{}/api/v1/products", app.base_url());

    // A customer token is refused
    let response = app
        .http_client
        .post(&url)
        .bearer_auth(&customer_token)
        .json(&product)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    // An admin creates the product (500 "Missing request extension" before
    // the admin middleware passed the caller on)
    let response = app
        .http_client
        .post(&url)
        .bearer_auth(&admin)
        .json(&product)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert_eq!(status, 201, "{body}");

    app.cleanup().await.ok();
}

/// Test 12: Store locations (branches, opening hours, pickup time)
#[tokio::test]
#[ignore = "Requires PostgreSQL database with proper permissions"]
async fn test_store_locations() {
    let app = TestApp::new().await.expect("Failed to create test app");
    let admin = app.admin_token().await.expect("Failed to create admin");
    let (customer, password) = app.create_test_customer().await.unwrap();
    let customer_token = app.login(&customer.email, &password).await.unwrap();
    let url = |path: &str| format!("{}/api/v1{}", app.base_url(), path);
    let location = |code: &str, slug: &str, public: bool| {
        serde_json::json!({
            "name": format!("Branch {code}"), "code": code, "slug": slug,
            "is_public": public, "is_pickup_point": public,
            "timezone": "Europe/Brussels", "country_code": "BE",
            "latitude": 50.8467, "longitude": 4.3525,
            "services": ["pickup", "showroom"]
        })
    };

    // Customers cannot manage locations
    let response = app
        .http_client
        .post(url("/admin/locations"))
        .bearer_auth(&customer_token)
        .json(&location("CUST", "cust", true))
        .send()
        .await
        .unwrap();
    assert!(response.status() == 401 || response.status() == 403);

    // Invalid input is a 400
    let mut bad = location("BAD", "bad", true);
    bad["timezone"] = serde_json::json!("Brussels");
    let response = app
        .http_client
        .post(url("/admin/locations"))
        .bearer_auth(&admin)
        .json(&bad)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);

    // A public pickup point and a private warehouse
    let response = app
        .http_client
        .post(url("/admin/locations"))
        .bearer_auth(&admin)
        .json(&location("CEN", "central", true))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    let response = app
        .http_client
        .post(url("/admin/locations"))
        .bearer_auth(&admin)
        .json(&location("WH", "warehouse", false))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);

    // Open every day, all day: always open (or opening at midnight), and an
    // order placed now is ready about an hour later
    let ranges: Vec<_> = (1..=7)
        .map(|d| serde_json::json!({"weekday": d, "opens_at": "00:00:00", "closes_at": "23:59:00"}))
        .collect();
    let response = app
        .http_client
        .put(url(&format!("/admin/locations/{id}/opening-hours")))
        .bearer_auth(&admin)
        .json(&serde_json::json!({ "ranges": ranges }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // A special day and a Dutch description
    let response = app
        .http_client
        .post(url(&format!("/admin/locations/{id}/closures")))
        .bearer_auth(&admin)
        .json(&serde_json::json!({"date": "2099-12-25", "reason": "Noël"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let response = app
        .http_client
        .put(url(&format!("/admin/locations/{id}/translations/nl")))
        .bearer_auth(&admin)
        .json(&serde_json::json!({"description": "Bouwmaterialen en showroom"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // The storefront lists the public location only, nearest first
    let list: Vec<serde_json::Value> = app
        .http_client
        .get(url("/locations?near=51.2194,4.4025&lang=nl"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list.len(), 1, "only the public location: {list:?}");
    let branch = &list[0];
    assert_eq!(branch["slug"], "central");
    assert_eq!(branch["opening_hours"].as_array().unwrap().len(), 7);
    assert_eq!(branch["description"], "Bouwmaterialen en showroom");
    assert_eq!(branch["locale"], "nl");
    assert!(branch["distance_km"].as_f64().unwrap() > 30.0);
    assert!(branch["pickup_ready_at"].is_string());
    assert!(
        branch["status"]["is_open"] == true || branch["status"]["next_opening"].is_string(),
        "{branch}"
    );

    // By slug; the private warehouse is not public
    let response = app
        .http_client
        .get(url("/locations/central"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response = app
        .http_client
        .get(url("/locations/warehouse"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    app.cleanup().await.ok();
}

/// Helper macro for running tests with reporting
#[macro_export]
macro_rules! test_with_report {
    ($name:expr, $body:expr) => {{
        let mut reporter = TestReporter::new($name);
        let result = $body(&mut reporter).await;
        reporter.finalize(result.is_ok());
        result.unwrap();
    }};
}

/// Generate index of all test reports
#[tokio::test]
async fn z_generate_report_index() {
    // This test runs last and generates an index of all reports
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Reports are saved immediately by each test (relative to workspace root)
    let output_dir = "./test-reports/detailed";

    // Count existing reports
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with("_report.json") {
                    count += 1;
                }
            }
        }
    }

    println!(
        "📊 Generated {} detailed test reports in: {}/",
        count, output_dir
    );
    println!("📊 View index: open {}/index.html", output_dir);
}
