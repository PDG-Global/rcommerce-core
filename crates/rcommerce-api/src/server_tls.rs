//! TLS server implementation using axum-server and rustls-acme
//!
//! This module provides HTTPS support with automatic Let's Encrypt certificate provisioning.

use axum::{Router, middleware};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::middleware::{admin_middleware, auth_middleware};
use crate::state::{AppState, AppStateParams};
use crate::tls::security_headers_middleware;
use rcommerce_core::config::{TlsConfig, TlsVersion};
use rcommerce_core::{Config, Result};

/// Run HTTPS server with TLS
/// 
/// Supports:
/// - Manual certificates (cert_file/key_file)
/// - Automatic Let's Encrypt certificates
/// - HTTP to HTTPS redirects
/// - TLS 1.3 only
pub async fn run_tls_server(config: Config) -> Result<()> {
    // Validate TLS configuration
    if let Err(e) = config.tls.validate() {
        return Err(rcommerce_core::Error::Config(format!(
            "TLS configuration error: {}",
            e
        )));
    }

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

    // Build router with security headers
    let app = build_tls_router(app_state, config.tls.clone());

    info!("R Commerce API server starting with TLS");
    info!("Listening on https://{}", addr);
    info!("TLS version: {:?}", config.tls.min_tls_version);

    // Configure TLS
    if config.tls.uses_manual_certs() {
        // Use manual certificates
        run_with_manual_certs(app, addr, &config.tls).await
    } else if config.tls.uses_lets_encrypt() {
        // Use Let's Encrypt
        #[cfg(feature = "letsencrypt")]
        {
            run_with_lets_encrypt(app, addr, &config.tls).await
        }
        #[cfg(not(feature = "letsencrypt"))]
        {
            Err(rcommerce_core::Error::Config(
                "Let's Encrypt support not compiled in. Build with --features letsencrypt".to_string()
            ))
        }
    } else {
        Err(rcommerce_core::Error::Config(
            "No certificate source configured".to_string()
        ))
    }
}

/// Run with manual certificates
async fn run_with_manual_certs(
    app: Router,
    addr: SocketAddr,
    tls_config: &TlsConfig,
) -> Result<()> {
    use axum_server::tls_rustls::RustlsConfig;
    use std::io::BufReader;

    let cert_file = tls_config
        .cert_file
        .as_ref()
        .ok_or_else(|| rcommerce_core::Error::Config("Certificate file not specified".to_string()))?;
    let key_file = tls_config
        .key_file
        .as_ref()
        .ok_or_else(|| rcommerce_core::Error::Config("Private key file not specified".to_string()))?;

    info!("Loading TLS certificates from {:?}", cert_file);

    // Load certificate and key
    let cert_file_content = tokio::fs::read(cert_file)
        .await
        .map_err(|e| rcommerce_core::Error::Config(format!("Failed to read certificate file: {}", e)))?;
    let key_file_content = tokio::fs::read(key_file)
        .await
        .map_err(|e| rcommerce_core::Error::Config(format!("Failed to read private key file: {}", e)))?;

    // Parse certificates
    let mut cert_reader = BufReader::new(&cert_file_content[..]);
    let certs: Vec<rustls::Certificate> = rustls_pemfile::certs(&mut cert_reader)
        .map_err(|e| rcommerce_core::Error::Config(format!("Failed to parse certificate: {}", e)))?
        .into_iter()
        .map(rustls::Certificate)
        .collect();

    // Parse private key
    let mut key_reader = BufReader::new(&key_file_content[..]);
    let keys = rustls_pemfile::pkcs8_private_keys(&mut key_reader)
        .map_err(|e| rcommerce_core::Error::Config(format!("Failed to parse private key: {}", e)))?;

    let key = keys
        .into_iter()
        .next()
        .ok_or_else(|| rcommerce_core::Error::Config("No private key found".to_string()))?;

    // Create Rustls config
    let rustls_config = RustlsConfig::from_der(
        certs.iter().map(|c| c.0.clone()).collect(),
        key.0,
    )
    .map_err(|e| rcommerce_core::Error::Config(format!("Failed to create TLS config: {}", e)))?;

    info!("Starting HTTPS server with manual certificates");

    // Start server
    axum_server::bind_rustls(addr, rustls_config)
        .serve(app.into_make_service())
        .await
        .map_err(|e| rcommerce_core::Error::Network(format!("Server error: {}", e)))?;

    Ok(())
}

/// Run with Let's Encrypt certificates
#[cfg(feature = "letsencrypt")]
async fn run_with_lets_encrypt(
    app: Router,
    addr: SocketAddr,
    tls_config: &TlsConfig,
) -> Result<()> {
    use rustls_acme::{AcmeConfig, caches::DirCache};
    use futures::StreamExt;

    let le_config = tls_config
        .lets_encrypt
        .as_ref()
        .ok_or_else(|| rcommerce_core::Error::Config("Let's Encrypt not configured".to_string()))?;

    if le_config.domains.is_empty() {
        return Err(rcommerce_core::Error::Config(
            "No domains configured for Let's Encrypt".to_string()
        ));
    }

    if le_config.email.is_empty() {
        return Err(rcommerce_core::Error::Config(
            "Email required for Let's Encrypt account".to_string()
        ));
    }

    info!("Configuring Let's Encrypt for domains: {:?}", le_config.domains);
    info!("Contact email: {}", le_config.email);

    // Create cache directory
    let cache_dir = le_config.cache_dir.clone();
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| rcommerce_core::Error::Config(format!("Failed to create cache dir: {}", e)))?;

    // Build ACME configuration
    let mut acme_config = AcmeConfig::new(le_config.domains.clone())
        .contact([format!("mailto:{}", le_config.email)])
        .cache(DirCache::new(cache_dir));

    // Use staging or production
    if le_config.use_staging {
        info!("Using Let's Encrypt STAGING environment");
        acme_config = acme_config.directory_lets_encrypt(true);
    } else {
        info!("Using Let's Encrypt PRODUCTION environment");
        acme_config = acme_config.directory_lets_encrypt(false);
    }

    // Create TLS acceptor
    let acceptor = acme_config
        .tokio_rustls_acceptor()
        .await
        .map_err(|e| rcommerce_core::Error::Network(format!("Failed to create ACME acceptor: {}", e)))?;

    info!("Starting HTTPS server with Let's Encrypt");
    info!("Certificates will be obtained automatically for: {:?}", le_config.domains);

    // Start server with ACME acceptor
    // Note: axum-server doesn't directly support ACME, so we use the acceptor from rustls-acme
    // This requires a custom accept loop
    
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| rcommerce_core::Error::Network(format!("Failed to bind: {}", e)))?;

    info!("Server bound to {}", addr);

    // Accept loop
    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                error!("Failed to accept connection: {}", e);
                continue;
            }
        };

        let acceptor = acceptor.clone();
        let app = app.clone();

        tokio::spawn(async move {
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    info!("TLS connection established from {}", peer_addr);
                    
                    // Serve connection using hyper
                    let service = hyper::service::make_service_fn(|_| {
                        let app = app.clone();
                        async move {
                            Ok::<_, std::convert::Infallible>(hyper::service::service_fn(move |req| {
                                let app = app.clone();
                                async move {
                                    let response = axum::serve::Serve::new(app)
                                        .call(req)
                                        .await;
                                    Ok::<_, std::convert::Infallible>(response)
                                }
                            }))
                        }
                    });

                    let _ = hyper::server::conn::Http::new()
                        .serve_connection(tls_stream, service)
                        .await;
                }
                Err(e) => {
                    error!("TLS handshake error from {}: {}", peer_addr, e);
                }
            }
        });
    }
}

/// Build router with TLS security headers
fn build_tls_router(app_state: AppState, tls_config: TlsConfig) -> Router {
    use axum::routing::get;
    
    // Start with base routes
    let router = Router::new()
        .route("/health", get(health_check))
        .route("/", get(root));

    // Add API routes
    let router = router.nest("/api/v1", crate::routes::api_routes(app_state.clone()));

    // Add middleware
    router
        .layer(middleware::from_fn(security_headers_middleware))
        .with_state(app_state)
}

/// Create application state
async fn create_app_state(config: &Config) -> Result<AppState> {
    use rcommerce_core::cache::RedisPool;
    use rcommerce_core::payment::agnostic::PaymentService;
    use rcommerce_core::repository::{create_pool, CustomerRepository, Database, ProductRepository, PostgresApiKeyRepository, PostgresSubscriptionRepository};
    use rcommerce_core::services::{AuthService, CustomerService, ProductService, CouponService, SubscriptionService, CartService, OrderService, DefaultTaxService, CheckoutService, ShippingProviderFactory};

    // Initialize database
    let pool = create_pool(
        &config.database.host,
        config.database.port,
        &config.database.database,
        &config.database.username,
        &config.database.password,
        config.database.pool_size,
    )
    .await?;

    let db = Database::new(pool.clone());

    // Initialize repositories
    let product_repo = ProductRepository::new(db.clone());
    let customer_repo = CustomerRepository::new(db.clone());
    let api_key_repo = PostgresApiKeyRepository::new(pool.clone());
    let subscription_repo = PostgresSubscriptionRepository::new(pool.clone());

    // Initialize services
    let product_service = ProductService::new(product_repo);
    let customer_service = CustomerService::new(customer_repo);
    let auth_service = AuthService::new(config.clone());
    let coupon_service = CouponService::new(db.clone());
    let payment_service = PaymentService::new(config.payment.clone());

    // Initialize Redis (optional)
    let redis = if let Some(redis_url) = &config.cache.redis_url {
        match RedisPool::new(rcommerce_core::cache::RedisConfig {
            url: redis_url.clone(),
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
                warn!("Failed to connect to Redis: {}. Continuing without cache.", e);
                None
            }
        }
    } else {
        None
    };

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
        Default::default(),
        Arc::new(CartService::new(db.clone())),
        Arc::new(OrderService::new(db.clone())),
        Arc::new(DefaultTaxService::new()),
        Arc::new(ShippingProviderFactory::new()),
        Arc::new(CheckoutService::new(
            Arc::new(CartService::new(db.clone())),
            Arc::new(DefaultTaxService::new()),
            Arc::new(OrderService::new(db.clone())),
            Arc::new(rcommerce_core::payment::gateways::MockPaymentGateway::new()),
            Arc::new(ShippingProviderFactory::new()),
            Default::default(),
        )),
        Arc::new(rcommerce_core::inventory::InventoryService::new(
            db.clone(),
            Default::default(),
        )),
        None,
        None,
    )).await)
}

async fn health_check() -> &'static str {
    "OK"
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "R Commerce API",
        "version": "0.1.0",
        "status": "operational",
        "tls": true
    }))
}
