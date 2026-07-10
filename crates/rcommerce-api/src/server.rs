use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

use crate::middleware::{
    admin_middleware, auth_middleware, optional_auth_middleware, security_headers_middleware,
};

use crate::state::{AppState, AppStateParams};
use rcommerce_core::cache::RedisPool;
use rcommerce_core::config::{CorsConfig, TlsConfig};
use rcommerce_core::inventory::{InventoryConfig, InventoryService};
use rcommerce_core::notification::channels::email::SmtpConfig;
use rcommerce_core::order::lifecycle::OrderEventDispatcher;
use rcommerce_core::payment::agnostic::PaymentService;
use rcommerce_core::payment::gateways::airwallex::AirwallexGateway;
use rcommerce_core::payment::gateways::airwallex_agnostic::AirwallexAgnosticGateway;
use rcommerce_core::payment::gateways::alipay_agnostic::AliPayAgnosticGateway;
use rcommerce_core::payment::gateways::stripe::StripeGateway;
use rcommerce_core::payment::gateways::stripe_agnostic::StripeAgnosticGateway;
use rcommerce_core::payment::gateways::wechatpay_agnostic::WeChatPayAgnosticGateway;
use rcommerce_core::payment::gateways::MockPaymentGateway;
use rcommerce_core::repository::{
    create_pool, CustomerRepository, Database, PgCartRepository, PgCouponRepository,
    PostgresApiKeyRepository, PostgresSubscriptionRepository, ProductRepository,
};
use rcommerce_core::services::{
    AuthService, BundleService, CartService, CheckoutConfig, CheckoutService, CouponService,
    CustomerService, OrderService, ProductService,
};
use rcommerce_core::shipping::ShippingProviderFactory;
use rcommerce_core::tax::DefaultTaxService;
use rcommerce_core::{Config, Result};
use std::sync::Arc;

pub async fn run(config: Config) -> Result<()> {
    // Check if TLS is enabled
    if config.tls.enabled {
        run_tls_server(config).await
    } else {
        run_http_server(config).await
    }
}

/// Run HTTP server (non-TLS mode)
async fn run_http_server(config: Config) -> Result<()> {
    let addr = SocketAddr::from((
        config
            .server
            .host
            .parse::<std::net::IpAddr>()
            .map_err(|e| rcommerce_core::Error::Config(format!("Invalid host: {}", e)))?,
        config.server.port,
    ));

    // Initialize application state
    let app_state = create_app_state(&config).await?;

    // Build router
    let app = build_router(app_state, None, &config.server.cors);

    info!("R Commerce API server listening on http://{}", addr);
    log_routes(&config);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| rcommerce_core::Error::Network(e.to_string()))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| rcommerce_core::Error::Network(e.to_string()))?;

    Ok(())
}

/// Run HTTPS server with TLS
///
/// This implementation:
/// 1. Starts an HTTP server on port 80 for ACME challenges and redirects
/// 2. Sets up TLS configuration with rustls (TLS 1.3 only)
/// 3. Starts an HTTPS server on port 443
/// 4. Supports both manual certificates and Let's Encrypt
async fn run_tls_server(config: Config) -> Result<()> {
    // Validate TLS configuration
    if let Err(e) = config.tls.validate() {
        return Err(rcommerce_core::Error::Config(format!(
            "TLS configuration error: {}",
            e
        )));
    }

    let https_addr = SocketAddr::from((
        config
            .server
            .host
            .parse::<std::net::IpAddr>()
            .map_err(|e| rcommerce_core::Error::Config(format!("Invalid host: {}", e)))?,
        config.tls.https_port,
    ));

    let http_addr = SocketAddr::from((
        config
            .server
            .host
            .parse::<std::net::IpAddr>()
            .map_err(|e| rcommerce_core::Error::Config(format!("Invalid host: {}", e)))?,
        config.tls.http_port,
    ));

    // Initialize application state
    let app_state = create_app_state(&config).await?;

    // Build main API router (HTTPS)
    let api_app = build_router(
        app_state.clone(),
        Some(config.tls.clone()),
        &config.server.cors,
    );

    // Build HTTP challenge router (HTTP port 80)
    let http_app = build_http_challenge_router(app_state.clone());

    info!("R Commerce API server starting with TLS enabled");
    info!("HTTPS server will listen on https://{}", https_addr);
    info!("HTTP challenge server will listen on http://{}", http_addr);
    log_routes(&config);

    // Start HTTP challenge server (port 80) - handles ACME challenges and redirects
    let http_listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .map_err(|e| {
            rcommerce_core::Error::Network(format!(
            "Failed to bind HTTP port {}: {}. Note: Binding port 80 may require root privileges.",
            config.tls.http_port, e
        ))
        })?;

    let http_server = tokio::spawn(async move {
        if let Err(e) = axum::serve(http_listener, http_app).await {
            error!("HTTP challenge server error: {}", e);
        }
    });

    // Start HTTPS server based on certificate type
    let https_server = if config.tls.uses_lets_encrypt() {
        start_lets_encrypt_server(https_addr, api_app, &config.tls).await?
    } else {
        start_manual_tls_server(https_addr, api_app, &config.tls).await?
    };

    // Wait for both servers
    tokio::select! {
        _ = http_server => {
            warn!("HTTP challenge server terminated");
        }
        _ = https_server => {
            warn!("HTTPS server terminated");
        }
    }

    Ok(())
}

/// Start HTTPS server with manual certificates
async fn start_manual_tls_server(
    addr: SocketAddr,
    app: Router,
    tls_config: &TlsConfig,
) -> Result<tokio::task::JoinHandle<()>> {
    use axum_server::tls_rustls::RustlsConfig;

    let cert_file = tls_config.cert_file.as_ref().ok_or_else(|| {
        rcommerce_core::Error::Config("Certificate file not specified".to_string())
    })?;
    let key_file = tls_config.key_file.as_ref().ok_or_else(|| {
        rcommerce_core::Error::Config("Private key file not specified".to_string())
    })?;

    info!("Loading TLS certificates from {:?}", cert_file);

    // Load certificates using axum-server's RustlsConfig
    let rustls_config = RustlsConfig::from_pem_file(cert_file, key_file)
        .await
        .map_err(|e| {
            rcommerce_core::Error::Config(format!("Failed to load TLS certificates: {}", e))
        })?;

    info!("TLS certificates loaded successfully");
    info!("Starting HTTPS server with manual certificates on {}", addr);

    let handle = tokio::spawn(async move {
        if let Err(e) = axum_server::bind_rustls(addr, rustls_config)
            .serve(app.into_make_service())
            .await
        {
            error!("HTTPS server error: {}", e);
        }
    });

    Ok(handle)
}

/// Start HTTPS server with Let's Encrypt automatic certificates
#[cfg(feature = "letsencrypt")]
async fn start_lets_encrypt_server(
    addr: SocketAddr,
    app: Router,
    tls_config: &TlsConfig,
) -> Result<tokio::task::JoinHandle<()>> {
    use futures::StreamExt;
    use rustls_acme::caches::DirCache;
    use rustls_acme::AcmeConfig;

    let le_config = tls_config
        .lets_encrypt
        .as_ref()
        .ok_or_else(|| rcommerce_core::Error::Config("Let's Encrypt not configured".to_string()))?;

    info!(
        "Starting HTTPS server with Let's Encrypt for domains: {:?}",
        le_config.domains
    );

    // Prepare domains and contact email
    let domains: Vec<String> = le_config.domains.clone();
    let emails: Vec<String> = vec![format!("mailto:{}", le_config.email)];
    let cache_dir = le_config.cache_dir.clone();
    let use_staging = !le_config.use_staging; // rustls-acme uses `prod` flag, we use `use_staging`

    // Ensure cache directory exists
    if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
        warn!("Failed to create cache directory: {}", e);
    }

    // Set up ACME configuration following rustls-acme example pattern
    let mut state = AcmeConfig::new(domains)
        .contact(emails)
        .cache(DirCache::new(cache_dir))
        .directory_lets_encrypt(use_staging)
        .state();

    let acceptor = state.axum_acceptor(state.default_rustls_config());

    // Spawn ACME event loop for certificate management
    tokio::spawn(async move {
        loop {
            match state.next().await {
                Some(Ok(ok)) => info!("ACME event: {:?}", ok),
                Some(Err(err)) => error!("ACME error: {:?}", err),
                None => {
                    warn!("ACME state stream ended");
                    break;
                }
            }
        }
    });

    info!("Let's Encrypt HTTPS server ready on {}", addr);

    let handle = tokio::spawn(async move {
        if let Err(e) = axum_server::bind(addr)
            .acceptor(acceptor)
            .serve(app.into_make_service())
            .await
        {
            error!("HTTPS server error: {}", e);
        }
    });

    Ok(handle)
}

/// Stub for when letsencrypt feature is disabled
#[cfg(not(feature = "letsencrypt"))]
async fn start_lets_encrypt_server(
    _addr: SocketAddr,
    _app: Router,
    _tls_config: &TlsConfig,
) -> Result<tokio::task::JoinHandle<()>> {
    Err(rcommerce_core::Error::Config(
        "Let's Encrypt support not compiled in. Rebuild with --features letsencrypt".to_string(),
    ))
}

/// Build HTTP challenge router for port 80
/// Handles ACME HTTP-01 challenges and redirects everything else to HTTPS
fn build_http_challenge_router(app_state: AppState) -> Router {
    Router::new()
        // ACME HTTP-01 challenge endpoint
        .route(
            "/.well-known/acme-challenge/:token",
            get(handle_acme_challenge),
        )
        // Health check (also available on HTTP)
        .route("/health", get(health_check))
        // Redirect all other requests to HTTPS
        .fallback(handle_http_redirect)
        .with_state(app_state)
}

/// Handle ACME HTTP-01 challenge
async fn handle_acme_challenge(
    axum::extract::Path(token): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    info!("ACME challenge request for token: {}", token);

    // In a real implementation, this would look up the challenge response
    // from the Let's Encrypt manager based on the token
    // For now, return a placeholder response
    (
        axum::http::StatusCode::OK,
        format!("Challenge response for token: {}", token),
    )
}

/// Redirect HTTP requests to HTTPS
async fn handle_http_redirect(
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
    axum::extract::Host(host): axum::extract::Host,
) -> impl axum::response::IntoResponse {
    let path = uri.path();
    let query = uri.query().map(|q| format!("?{}", q)).unwrap_or_default();
    let https_url = format!("https://{}{}{}", host, path, query);

    (
        axum::http::StatusCode::MOVED_PERMANENTLY,
        [(axum::http::header::LOCATION, https_url)],
        "Redirecting to HTTPS...",
    )
}

/// Create application state
async fn create_app_state(config: &Config) -> Result<AppState> {
    // Initialize database connection
    info!("Connecting to PostgreSQL database...");
    let pool = create_pool(
        &config.database.host,
        config.database.port,
        &config.database.database,
        &config.database.username,
        &config.database.password,
        config.database.pool_size,
    )
    .await?;
    let db = Database::new(pool);

    // Initialize repositories
    let product_repo = ProductRepository::new(db.clone());
    let customer_repo = CustomerRepository::new(db.clone());
    let api_key_repo = PostgresApiKeyRepository::new(db.pool().clone());
    let subscription_repo = PostgresSubscriptionRepository::new(db.pool().clone());
    let coupon_repo = Arc::new(PgCouponRepository::new(db.pool().clone()));
    let cart_repo = Arc::new(PgCartRepository::new(db.pool().clone()));

    // Initialize services
    let product_service = ProductService::new(product_repo);
    let customer_service = CustomerService::new(customer_repo);
    let auth_service = AuthService::new(config.clone());
    let coupon_service = CouponService::new(coupon_repo.clone(), cart_repo.clone());
    let bundle_service = Arc::new(BundleService::new(db.clone()));
    let cart_service = CartService::new(
        cart_repo.clone(),
        coupon_repo.clone(),
        Arc::new(coupon_service.clone()),
    )
    .with_bundle_service(bundle_service.clone())
    .with_database(db.clone());

    // Initialize payment service with gateways
    let default_gateway = config.payment.default_gateway.clone();
    let mut payment_service = PaymentService::new(default_gateway);

    // Register mock gateway for testing/development
    let mock_gateway = Box::new(MockPaymentGateway::new());
    payment_service.register_gateway("mock".to_string(), mock_gateway);
    info!("Mock payment gateway registered");

    // Register Stripe gateway if enabled and API key is available
    if config.payment.stripe.enabled {
        if let Some(stripe_key) = config
            .payment
            .stripe
            .secret_key
            .clone()
            .or_else(|| std::env::var("STRIPE_API_KEY").ok())
        {
            let webhook_secret = config
                .payment
                .stripe
                .webhook_secret
                .clone()
                .unwrap_or_else(|| std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default());
            let stripe_gateway = Box::new(StripeAgnosticGateway::new(stripe_key, webhook_secret));
            payment_service.register_gateway("stripe".to_string(), stripe_gateway);
            info!("Stripe gateway registered");
        } else {
            warn!("Stripe gateway enabled but API key not found");
        }
    }

    // Register WeChat Pay gateway if enabled
    if config.payment.wechatpay.enabled {
        if let (Some(mch_id), Some(app_id), Some(api_key), Some(serial_no), Some(private_key)) = (
            config
                .payment
                .wechatpay
                .mch_id
                .clone()
                .or_else(|| std::env::var("WECHATPAY_MCH_ID").ok()),
            config
                .payment
                .wechatpay
                .app_id
                .clone()
                .or_else(|| std::env::var("WECHATPAY_APP_ID").ok()),
            config
                .payment
                .wechatpay
                .api_key
                .clone()
                .or_else(|| std::env::var("WECHATPAY_API_KEY").ok()),
            config
                .payment
                .wechatpay
                .serial_no
                .clone()
                .or_else(|| std::env::var("WECHATPAY_SERIAL_NO").ok()),
            config
                .payment
                .wechatpay
                .private_key
                .clone()
                .or_else(|| std::env::var("WECHATPAY_PRIVATE_KEY").ok()),
        ) {
            let wechatpay_gateway = Box::new(WeChatPayAgnosticGateway::new(
                mch_id,
                api_key,
                app_id,
                serial_no,
                private_key,
                config.payment.wechatpay.sandbox,
            ));
            payment_service.register_gateway("wechatpay".to_string(), wechatpay_gateway);
            info!(
                "WeChat Pay gateway registered (sandbox: {})",
                config.payment.wechatpay.sandbox
            );
        } else {
            warn!("WeChat Pay gateway enabled but configuration incomplete");
        }
    }

    // Register AliPay gateway if enabled
    if config.payment.alipay.enabled {
        if let (Some(app_id), Some(private_key), Some(alipay_public_key)) = (
            config
                .payment
                .alipay
                .app_id
                .clone()
                .or_else(|| std::env::var("ALIPAY_APP_ID").ok()),
            config
                .payment
                .alipay
                .private_key
                .clone()
                .or_else(|| std::env::var("ALIPAY_PRIVATE_KEY").ok()),
            config
                .payment
                .alipay
                .alipay_public_key
                .clone()
                .or_else(|| std::env::var("ALIPAY_PUBLIC_KEY").ok()),
        ) {
            let alipay_gateway = Box::new(AliPayAgnosticGateway::new(
                app_id,
                private_key,
                alipay_public_key,
                config.payment.alipay.sandbox,
            ));
            payment_service.register_gateway("alipay".to_string(), alipay_gateway);
            info!(
                "AliPay gateway registered (sandbox: {})",
                config.payment.alipay.sandbox
            );
        } else {
            warn!("AliPay gateway enabled but configuration incomplete");
        }
    }

    // Register Airwallex gateway if enabled
    let airwallex_config: Option<(String, String, String, bool)> =
        if config.payment.airwallex.enabled {
            if let (Some(client_id), Some(api_key)) = (
                config
                    .payment
                    .airwallex
                    .client_id
                    .clone()
                    .or_else(|| std::env::var("AIRWALLEX_CLIENT_ID").ok()),
                config
                    .payment
                    .airwallex
                    .api_key
                    .clone()
                    .or_else(|| std::env::var("AIRWALLEX_API_KEY").ok()),
            ) {
                let webhook_secret = config
                    .payment
                    .airwallex
                    .webhook_secret
                    .clone()
                    .unwrap_or_else(|| {
                        std::env::var("AIRWALLEX_WEBHOOK_SECRET").unwrap_or_default()
                    });
                let airwallex_gateway = Box::new(AirwallexAgnosticGateway::new(
                    client_id.clone(),
                    api_key.clone(),
                    webhook_secret.clone(),
                    config.payment.airwallex.demo,
                ));
                payment_service.register_gateway("airwallex".to_string(), airwallex_gateway);
                info!(
                    "Airwallex gateway registered (demo: {})",
                    config.payment.airwallex.demo
                );
                Some((
                    client_id,
                    api_key,
                    webhook_secret,
                    config.payment.airwallex.demo,
                ))
            } else {
                warn!("Airwallex gateway enabled but configuration incomplete");
                None
            }
        } else {
            None
        };

    // Initialize Redis (optional)
    let redis = init_redis(config).await;

    // Initialize file upload service
    let file_upload_service = rcommerce_core::FileUploadService::from_config(config)?;

    // Initialize order service
    let inventory_config = InventoryConfig {
        low_stock_threshold: 20,
        enable_restock_alerts: true,
        enable_reservations: true,
        reservation_timeout_minutes: 30,
    };
    let inventory_service = InventoryService::new(db.clone(), inventory_config);
    let event_dispatcher = OrderEventDispatcher::new();

    // Resolve legacy payment gateway for order/checkout service
    let stripe_legacy_config: Option<String> =
        if config.payment.default_gateway == "stripe" || config.payment.stripe.enabled {
            config
                .payment
                .stripe
                .secret_key
                .clone()
                .or_else(|| std::env::var("STRIPE_API_KEY").ok())
        } else {
            None
        };

    let order_payment_gateway: Box<dyn rcommerce_core::payment::PaymentGateway> =
        if let Some(stripe_key) = &stripe_legacy_config {
            let webhook_secret = config
                .payment
                .stripe
                .webhook_secret
                .clone()
                .unwrap_or_else(|| std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default());
            Box::new(StripeGateway::new(stripe_key.clone(), webhook_secret))
        } else if let Some((client_id, api_key, webhook_secret, _demo)) = &airwallex_config {
            Box::new(AirwallexGateway::new(
                client_id.clone(),
                api_key.clone(),
                webhook_secret.clone(),
            ))
        } else {
            Box::new(MockPaymentGateway::new())
        };
    let order_service = Arc::new(OrderService::new(
        db.clone(),
        order_payment_gateway,
        inventory_service.clone(),
        event_dispatcher,
    ));

    // Initialize tax service
    let tax_service = Arc::new(DefaultTaxService::new(db.pool().clone()));

    // Initialize shipping provider factory
    let shipping_factory = Arc::new(ShippingProviderFactory::from_config(&config.shipping));
    info!("Shipping provider factory initialized");

    // Wrap cart_service in Arc for checkout service
    let cart_service = Arc::new(cart_service);

    // Initialize checkout service
    let checkout_config = CheckoutConfig {
        stripe_publishable_key: config.payment.stripe.publishable_key.clone(),
        ..CheckoutConfig::default()
    };
    let checkout_payment_gateway: Arc<dyn rcommerce_core::payment::PaymentGateway> =
        if let Some(stripe_key) = &stripe_legacy_config {
            let webhook_secret = config
                .payment
                .stripe
                .webhook_secret
                .clone()
                .unwrap_or_else(|| std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default());
            Arc::new(StripeGateway::new(stripe_key.clone(), webhook_secret))
        } else if let Some((client_id, api_key, webhook_secret, _demo)) = &airwallex_config {
            Arc::new(AirwallexGateway::new(
                client_id.clone(),
                api_key.clone(),
                webhook_secret.clone(),
            ))
        } else {
            Arc::new(MockPaymentGateway::new())
        };
    let checkout_service = Arc::new(CheckoutService::new(
        cart_service.clone(),
        tax_service.clone(),
        order_service.clone(),
        checkout_payment_gateway,
        shipping_factory.clone(),
        checkout_config,
    ));
    info!("Checkout service initialized");

    // Build SMTP config from notification settings if enabled
    let smtp_config = if config.notifications.enabled {
        if let (Some(host), Some(user), Some(pass), Some(from_email)) = (
            config.notifications.email.smtp_host.as_ref(),
            config.notifications.email.smtp_user.as_ref(),
            config.notifications.email.smtp_pass.as_ref(),
            config.notifications.email.from_email.as_ref(),
        ) {
            let smtp = SmtpConfig {
                host: host.clone(),
                port: config.notifications.email.smtp_port.unwrap_or(465),
                username: user.clone(),
                password: pass.clone(),
                from_address: from_email.clone(),
                from_name: config
                    .notifications
                    .email
                    .from_name
                    .clone()
                    .unwrap_or_else(|| "R Commerce".to_string()),
                use_tls: config.notifications.email.smtp_port == Some(465),
                use_starttls: config.notifications.email.smtp_port != Some(465)
                    && config.notifications.email.smtp_tls,
            };
            info!("SMTP configuration found for {}", smtp.host);
            Some(smtp)
        } else {
            info!("Notification emails enabled but SMTP not fully configured");
            None
        }
    } else {
        info!("Notifications disabled in config");
        None
    };

    // Create app state
    Ok(AppState::new(AppStateParams::new(
        product_service,
        customer_service,
        auth_service,
        db,
        redis,
        api_key_repo,
        subscription_repo,
        coupon_service,
        payment_service,
        file_upload_service,
        cart_service,
        order_service,
        tax_service,
        shipping_factory,
        checkout_service,
        Arc::new(inventory_service),
        smtp_config,
    )).await)
}

/// Build CORS layer from configuration
fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    let cors = CorsLayer::new();

    // Origins - check if wildcard is used
    let allow_any_origin = config.allowed_origins.contains(&"*".to_string());

    // Methods
    let methods: Vec<axum::http::Method> = config
        .allowed_methods
        .iter()
        .filter_map(|m| m.parse().ok())
        .collect();
    let cors = cors.allow_methods(methods);

    // Headers
    let headers: Vec<axum::http::HeaderName> = config
        .allowed_headers
        .iter()
        .filter_map(|h| h.parse().ok())
        .collect();
    let cors = cors.allow_headers(headers);

    // Security: Cannot combine Any origin with credentials
    // See: https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS/Errors/CORSNotSupportingCredentials
    if allow_any_origin {
        // When allowing any origin, we must NOT allow credentials
        warn!("CORS: Allowing any origin (*) - credentials disabled for security");
        cors.allow_origin(Any)
            .allow_credentials(false)
            .max_age(Duration::from_secs(config.max_age.unwrap_or(3600)))
    } else {
        let origins: Vec<axum::http::HeaderValue> = config
            .allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        cors.allow_origin(origins)
            .allow_credentials(config.allow_credentials)
            .max_age(Duration::from_secs(config.max_age.unwrap_or(3600)))
    }
}

/// Build the main API router
fn build_router(
    app_state: AppState,
    tls_config: Option<TlsConfig>,
    cors_config: &CorsConfig,
) -> Router {
    // Configure CORS from config
    let cors = build_cors_layer(cors_config);

    // Build main router with API v1 routes
    let mut app = Router::new()
        .route("/health", get(health_check))
        .route("/", get(root))
        .nest("/api/v1", api_routes(app_state.clone()))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Add security headers middleware (always, not just with TLS)
    // HSTS header will only be added when TLS is enabled
    app = app.layer(middleware::from_fn_with_state(
        tls_config,
        security_headers_middleware,
    ));

    app.with_state(app_state)
}

/// Log available routes
fn log_routes(config: &Config) {
    let protocol = if config.tls.enabled { "https" } else { "http" };
    let port = if config.tls.enabled {
        config.tls.https_port
    } else {
        config.server.port
    };

    info!("Available routes ({}://localhost:{}):", protocol, port);
    info!("  GET  /health                      - Health check");
    info!("  GET  /                            - API info");
    info!("  GET  /api/v1/products             - List products");
    info!("  GET  /api/v1/products/:id         - Get product");
    info!("  GET  /api/v1/customers            - List customers");
    info!("  GET  /api/v1/customers/:id        - Get customer");
    info!("  GET  /api/v1/orders               - List orders");
    info!("  GET  /api/v1/orders/:id           - Get order");
    info!("  POST /api/v1/checkout/initiate    - Initiate checkout (with tax/shipping calc)");
    info!("  POST /api/v1/checkout/shipping    - Select shipping method");
    info!("  POST /api/v1/checkout/complete    - Complete checkout");
    info!("  POST /api/v1/auth/login           - Login");
    info!("  POST /api/v1/auth/register        - Register");
    info!("  POST /api/v1/carts/guest          - Create guest cart");
    info!("  GET  /api/v1/carts/me             - Get customer cart");
    info!("  POST /api/v1/carts/merge          - Merge carts");
    info!("  GET  /api/v1/coupons              - List coupons");
    info!("  POST /api/v1/coupons              - Create coupon");
    info!("  POST /api/v1/payments/methods     - Get payment methods");
    info!("  POST /api/v1/payments             - Create payment");
    info!("  GET  /api/v1/payments/:id         - Get payment status");
    info!("  POST /api/v1/payments/:id/complete - Complete payment");
    info!("  POST /api/v1/payments/:id/refund  - Refund payment");
    info!("  GET  /api/v1/admin/statistics/dashboard - Dashboard stats (admin)");
    info!("  GET  /api/v1/admin/statistics/sales     - Sales statistics (admin)");
    info!("  GET  /api/v1/admin/statistics/orders    - Order statistics (admin)");
    info!("  GET  /api/v1/admin/statistics/products  - Product performance (admin)");
    info!("  GET  /api/v1/admin/statistics/customers - Customer statistics (admin)");
    info!("  GET  /api/v1/admin/statistics/revenue   - Revenue trends (admin)");
    info!("  GET  /api/v1/admin/statistics/compare   - Period comparison (admin)");

    if config.tls.enabled {
        info!("  (HTTP port {} redirects to HTTPS)", config.tls.http_port);
        info!("  /.well-known/acme-challenge/*     - ACME challenge endpoint");
    }
}

/// Initialize Redis connection pool (optional)
async fn init_redis(config: &Config) -> Option<RedisPool> {
    // Check if Redis URL is configured
    let redis_url = config
        .cache
        .redis_url
        .clone()
        .or_else(|| std::env::var("REDIS_URL").ok());

    if let Some(url) = redis_url {
        info!("Connecting to Redis at {}...", url);
        match RedisPool::new(rcommerce_core::cache::RedisConfig {
            url,
            pool_size: config.cache.redis_pool_size as usize,
            ..Default::default()
        })
        .await
        {
            Ok(pool) => {
                info!("Redis connected successfully");
                Some(pool)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to connect to Redis: {}. Continuing without cache.",
                    e
                );
                None
            }
        }
    } else {
        info!("Redis not configured. Running without cache.");
        None
    }
}

/// API v1 routes
fn api_routes(app_state: AppState) -> Router<AppState> {
    // Public storefront routes (no auth required)
    let public_routes = Router::new()
        // Product catalog
        .merge(crate::routes::product_router())
        // Customer authentication (register/login)
        .merge(crate::routes::auth_public_router())
        // Cart operations are public; the cart ID acts as the bearer token
        .merge(crate::routes::cart_router())
        // Guest checkout with optional customer authentication
        .merge(crate::routes::checkout_router())
        // Webhooks are public (signature verification handles security)
        .route(
            "/webhooks/:gateway_id",
            post(crate::routes::payment::handle_webhook),
        )
        .route_layer(middleware::from_fn_with_state(
            app_state.clone(),
            optional_auth_middleware,
        ));

    // Protected routes (JWT auth required)
    let protected_routes = Router::new()
        .merge(crate::routes::customer_router())
        .merge(crate::routes::order_router())
        // Auth routes requiring authentication (password reset request)
        .merge(crate::routes::auth_protected_router())
        // Payment routes except webhooks
        .merge(crate::routes::payment::payment_routes())
        // Subscription routes
        .merge(crate::routes::subscription_router())
        // Dunning routes
        .merge(crate::routes::dunning_router())
        .route_layer(middleware::from_fn_with_state(
            app_state.clone(),
            auth_middleware,
        ));

    // Admin routes (admin auth required)
    let admin_routes = Router::new()
        .nest("/admin", crate::routes::admin_router())
        .merge(crate::routes::product_admin_router())
        .merge(crate::routes::statistics_router())
        .merge(crate::routes::coupon_router())
        .route_layer(middleware::from_fn_with_state(app_state, admin_middleware));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(admin_routes)
}

async fn health_check() -> &'static str {
    "OK"
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "R Commerce API",
        "version": "0.1.0",
        "status": "operational",
        "phase": "Phase 1 MVP"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        let response = health_check().await;
        assert_eq!(response, "OK");
    }
}
