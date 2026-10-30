//! Isolated integration tests for the PR #12/#6 rework: client-IP resolution,
//! rate limiting, and login throttling/enumeration resistance.
//!
//! Deliberately self-contained (own TestApp, no shared helpers) so it can run
//! alongside `integration_tests.rs` without interference.
//!
//! - The TCP/middleware tests need no database and run by default.
//! - The login tests need PostgreSQL (`TEST_DATABASE_URL`) and are marked
//!   `#[ignore]`; run with `cargo test -p rcommerce-api --test auth_security
//!   -- --ignored --test-threads=1`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use sqlx::PgPool;
use tokio::net::TcpListener;

use rcommerce_api::middleware::{
    client_ip_middleware, rate_limit_middleware, ClientIp, RateLimiter,
};
use rcommerce_api::state::{AppState, AppStateParams};
use rcommerce_core::config::RateLimitConfig;
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

// =============================================================================
// Test application construction
// =============================================================================

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres@localhost/rcommerce_test".to_string())
}

/// Build a full AppState. With `connect_lazy` no database connection is
/// opened, so middleware-only tests run without PostgreSQL.
async fn build_app_state(rate_limiting: RateLimitConfig, lazy: bool) -> anyhow::Result<AppState> {
    let url = test_database_url();
    let db_pool = if lazy {
        PgPool::connect_lazy(&url)?
    } else {
        PgPool::connect(&url).await?
    };

    let db = Database::new(db_pool.clone());

    let mut core_config = Config::default();
    core_config.security.jwt.secret = "test-secret-do-not-use-in-production-32-bytes".to_string();

    let product_service = ProductService::new(ProductRepository::new(db.clone()));
    let customer_service = CustomerService::new(CustomerRepository::new(db.clone()));
    let auth_service = AuthService::new(core_config);

    let cart_repo: Arc<dyn rcommerce_core::repository::CartRepository> =
        Arc::new(PgCartRepository::new(db_pool.clone()));
    let coupon_repo: Arc<dyn rcommerce_core::repository::CouponRepository> =
        Arc::new(PgCouponRepository::new(db_pool.clone()));

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

    let tax_service = Arc::new(rcommerce_core::tax::DefaultTaxService::new(db_pool.clone()));
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

    let file_upload_service = FileUploadService::new_local(
        std::path::PathBuf::from("./test_uploads_auth_security"),
        "http://localhost:8080/uploads".to_string(),
    )?;

    let params = AppStateParams::new(
        product_service,
        customer_service,
        auth_service,
        db,
        None,
        PostgresApiKeyRepository::new(db_pool.clone()),
        PostgresSubscriptionRepository::new(db_pool.clone()),
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
    )
    .with_rate_limiting(rate_limiting);

    Ok(AppState::new(params).await)
}

async fn serve(app: Router) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (addr, handle)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()
        .unwrap()
}

// =============================================================================
// TCP-level middleware tests (no database required)
// =============================================================================

/// The probe: reads the ClientIp extension installed by the middleware.
async fn probe_handler(request: axum::extract::Request) -> impl IntoResponse {
    match request.extensions().get::<ClientIp>() {
        Some(ip) => (StatusCode::OK, ip.ip().to_string()),
        None => (StatusCode::INTERNAL_SERVER_ERROR, "no ClientIp".to_string()),
    }
}

fn probe_router(state: AppState) -> Router {
    Router::new()
        .route("/probe", get(probe_handler))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            client_ip_middleware,
        ))
        .with_state(state)
}

#[tokio::test]
async fn xff_from_untrusted_peer_is_ignored() {
    let state = build_app_state(RateLimitConfig::default(), true)
        .await
        .unwrap();
    let app = probe_router(state);
    let (addr, _h) = serve(app).await;

    let resp = client()
        .get(format!("http://{addr}/probe"))
        .header("x-forwarded-for", "1.2.3.4")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // The forged header is dropped; the real peer address is used.
    assert_eq!(resp.text().await.unwrap(), "127.0.0.1");
}

#[tokio::test]
async fn trusted_proxy_walks_the_chain_right_to_left() {
    let config = RateLimitConfig {
        trusted_proxies: vec!["127.0.0.0/8".to_string(), "10.0.0.0/8".to_string()],
        ..Default::default()
    };
    let state = build_app_state(config, true).await.unwrap();
    let app = probe_router(state);
    let (addr, _h) = serve(app).await;
    let client = client();

    // The client forged "6.6.6.6"; proxies appended 10.0.0.3 and 10.0.0.2;
    // the peer (127.0.0.1) is trusted, so walk past the trusted entries.
    let resp = client
        .get(format!("http://{addr}/probe"))
        .header("x-forwarded-for", "6.6.6.6, 10.0.0.3, 127.0.0.1")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), "6.6.6.6");

    // Single entry appended by the trusted proxy.
    let resp = client
        .get(format!("http://{addr}/probe"))
        .header("x-forwarded-for", "203.0.113.9")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), "203.0.113.9");

    // No header: the peer itself.
    let resp = client
        .get(format!("http://{addr}/probe"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), "127.0.0.1");
}

#[tokio::test]
async fn malformed_or_overlong_headers_from_trusted_peer_fail_closed() {
    let config = RateLimitConfig {
        trusted_proxies: vec!["127.0.0.0/8".to_string()],
        max_forwarded_hops: 2,
        ..Default::default()
    };
    let state = build_app_state(config, true).await.unwrap();
    let app = probe_router(state);
    let (addr, _h) = serve(app).await;
    let client = client();

    // Unparseable entry.
    let resp = client
        .get(format!("http://{addr}/probe"))
        .header("x-forwarded-for", "not-an-ip")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Empty entry in the middle.
    let resp = client
        .get(format!("http://{addr}/probe"))
        .header("x-forwarded-for", "1.1.1.1,,2.2.2.2")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // More hops than configured.
    let resp = client
        .get(format!("http://{addr}/probe"))
        .header("x-forwarded-for", "1.1.1.1, 2.2.2.2, 3.3.3.3")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn per_ip_rate_limit_counts_the_real_peer_not_the_forged_header() {
    let config = RateLimitConfig {
        requests_per_minute: 2,
        expose_headers: true,
        ..Default::default()
    };
    let state = build_app_state(config.clone(), true).await.unwrap();
    // The limiter reads the ClientIp extension installed by the middleware,
    // so client-IP resolution must run outside the limiter.
    let app: Router = Router::new()
        .route("/probe", get(probe_handler))
        .layer(axum::middleware::from_fn_with_state(
            RateLimiter::new(&config),
            rate_limit_middleware,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            client_ip_middleware,
        ))
        .with_state(state);
    let (addr, _h) = serve(app).await;
    let client = client();

    // Two allowed, with a forged header each time — spoofing does not reset
    // the budget because the header is ignored (untrusted peer).
    for i in 0..2 {
        let resp = client
            .get(format!("http://{addr}/probe"))
            .header("x-forwarded-for", &format!("10.99.0.{}", i + 1))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        if i == 0 {
            assert_eq!(resp.headers()["x-ratelimit-remaining"], "1");
        }
    }
    let resp = client
        .get(format!("http://{addr}/probe"))
        .header("x-forwarded-for", "10.99.0.99")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(resp.headers().get("retry-after").is_some());
}

// =============================================================================
// Login throttling and enumeration-resistance tests (require PostgreSQL)
// =============================================================================

/// Low throttle budgets so tests can trip them quickly. The IP budget stays
/// high; individual tests exercise the (IP, e-mail) pair budget.
fn login_rate_config() -> RateLimitConfig {
    RateLimitConfig {
        login_ip_max_attempts: 1000,
        login_pair_max_failures: 3,
        login_window_secs: 900,
        ..Default::default()
    }
}

struct LoginApp {
    state: AppState,
    pool: PgPool,
    addr: SocketAddr,
}

impl LoginApp {
    async fn new(rate_config: RateLimitConfig) -> anyhow::Result<Self> {
        let pool = PgPool::connect(&test_database_url()).await?;
        // Serialized, once-per-process schema setup (no DDL inside tests).
        rcommerce_core::ensure_test_migrations(&pool).await?;
        let state = build_app_state(rate_config, false).await?;
        let app = Router::new()
            .route("/health", get(|| async { "OK" }))
            .nest("/api/v1", rcommerce_api::server::api_routes(state.clone()))
            .with_state(state.clone());
        let (addr, _h) = serve(app).await;
        Ok(Self { state, pool, addr })
    }

    async fn create_customer(
        &self,
        email: &str,
        password: &str,
        verified: bool,
    ) -> anyhow::Result<rcommerce_core::models::Customer> {
        let hash = self.state.auth_service.hash_password(password)?;
        let customer = self
            .state
            .customer_service
            .create_customer_with_password(
                rcommerce_core::models::CreateCustomerRequest {
                    email: email.to_string(),
                    first_name: "Test".to_string(),
                    last_name: "Customer".to_string(),
                    phone: None,
                    accepts_marketing: false,
                    currency: rcommerce_core::models::Currency::USD,
                },
                hash,
            )
            .await?;
        if verified {
            self.state
                .customer_service
                .update_email_verified(customer.id, true)
                .await?;
        }
        Ok(customer)
    }

    async fn create_imported_customer(&self, email: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO customers (email, first_name, last_name, is_imported) \
             VALUES ($1, 'Imported', 'Customer', true)",
        )
        .bind(email)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn login(
        &self,
        email: &str,
        password: &str,
    ) -> (StatusCode, String, reqwest::header::HeaderMap) {
        let resp = client()
            .post(format!("http://{}/api/v1/auth/login", self.addr))
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await
            .unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        (status, resp.text().await.unwrap(), headers)
    }
}

fn unique_email(tag: &str) -> String {
    format!("{tag}-{}@example.com", uuid::Uuid::new_v4())
}

#[tokio::test]
#[ignore]
async fn unknown_wrong_and_imported_logins_are_indistinguishable() {
    let app = LoginApp::new(login_rate_config()).await.unwrap();
    let email = unique_email("known");
    app.create_customer(&email, "correct-horse-battery", true)
        .await
        .unwrap();
    let imported = unique_email("imported");
    app.create_imported_customer(&imported).await.unwrap();

    let (status_unknown, body_unknown, _) = app.login(&unique_email("nobody"), "whatever123").await;
    let (status_wrong, body_wrong, _) = app.login(&email, "wrong-password").await;
    let (status_imported, body_imported, _) = app.login(&imported, "whatever123").await;

    assert_eq!(status_unknown, StatusCode::UNAUTHORIZED);
    assert_eq!(status_wrong, StatusCode::UNAUTHORIZED);
    assert_eq!(status_imported, StatusCode::UNAUTHORIZED);
    assert_eq!(body_unknown, body_wrong);
    assert_eq!(body_wrong, body_imported);
}

#[tokio::test]
#[ignore]
async fn verification_status_is_revealed_only_after_a_valid_password() {
    let app = LoginApp::new(login_rate_config()).await.unwrap();
    let email = unique_email("unverified");
    app.create_customer(&email, "correct-horse-battery", false)
        .await
        .unwrap();

    // Wrong password: the generic 401, no hint that the account exists or
    // that it is unverified.
    let (status, body, _) = app.login(&email, "wrong-password").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!body.contains("verify"));

    // Correct password: now the verification status may be revealed.
    let (status, body, _) = app.login(&email, "correct-horse-battery").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("email_verification_required"));
}

#[tokio::test]
#[ignore]
async fn successful_login_returns_tokens() {
    let app = LoginApp::new(login_rate_config()).await.unwrap();
    let email = unique_email("happy");
    app.create_customer(&email, "correct-horse-battery", true)
        .await
        .unwrap();

    let (status, body, _) = app.login(&email, "correct-horse-battery").await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(json["access_token"].as_str().is_some());
    assert!(json["refresh_token"].as_str().is_some());
}

#[tokio::test]
#[ignore]
async fn pair_throttle_locks_and_success_clears_only_that_pair() {
    let app = LoginApp::new(login_rate_config()).await.unwrap();

    // Victim pair: three failures exhaust the budget (pair_max = 3).
    let victim = unique_email("victim");
    app.create_customer(&victim, "correct-horse-battery", true)
        .await
        .unwrap();
    for _ in 0..3 {
        let (status, _, _) = app.login(&victim, "wrong-password").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    // Even the correct password is refused now, with a generic 429 +
    // Retry-After.
    let (status, body, headers) = app.login(&victim, "correct-horse-battery").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.get("retry-after").is_some());
    assert!(body.contains("too_many_requests"));
    assert!(!body.contains(&victim));

    // A different e-mail from the same IP is unaffected: no account-only lock.
    let other = unique_email("other");
    app.create_customer(&other, "correct-horse-battery", true)
        .await
        .unwrap();
    let (status, _, _) = app.login(&other, "correct-horse-battery").await;
    assert_eq!(status, StatusCode::OK);

    // Two failures, then success clears the pair; two more failures are then
    // allowed (the counter did not survive the successful login).
    let clearing = unique_email("clearing");
    app.create_customer(&clearing, "correct-horse-battery", true)
        .await
        .unwrap();
    for _ in 0..2 {
        app.login(&clearing, "wrong-password").await;
    }
    let (status, _, _) = app.login(&clearing, "correct-horse-battery").await;
    assert_eq!(status, StatusCode::OK);
    for _ in 0..2 {
        let (status, _, _) = app.login(&clearing, "wrong-password").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _, _) = app.login(&clearing, "correct-horse-battery").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
#[ignore]
async fn ip_attempt_budget_is_enforced() {
    let config = RateLimitConfig {
        login_ip_max_attempts: 5,
        login_pair_max_failures: 100,
        login_window_secs: 900,
        ..Default::default()
    };
    let app = LoginApp::new(config).await.unwrap();

    // Five attempts across *different* e-mails (all unknown, so no pair
    // budget pressure); the sixth hits the IP attempt budget.
    for i in 0..5 {
        let (status, _, _) = app
            .login(&unique_email(&format!("probe{i}")), "whatever")
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _, headers) = app.login(&unique_email("one-more"), "whatever").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.get("retry-after").is_some());
}
