//! R Commerce Demo Frontend Server
//!
//! A standalone CLI tool that serves the demo frontend and securely proxies
//! API requests to the R Commerce backend. This keeps API keys secure by
//! never exposing them to the client-side JavaScript.
//!
//! Usage:
//!   rcommerce-demo --api-url http://localhost:8080 --api-key ak_prefix.secret
//!   rcommerce-demo -c demo-config.toml

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, Request, Response, StatusCode, Uri},
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use clap::Parser;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

use tower_http::cors::CorsLayer;

use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

/// Default frontend files embedded in the binary
const DEFAULT_INDEX_HTML: &str = include_str!("../../../demo-frontend/index.html");
const DEFAULT_PRODUCT_HTML: &str = include_str!("../../../demo-frontend/product.html");
const DEFAULT_CART_HTML: &str = include_str!("../../../demo-frontend/cart.html");
const DEFAULT_CHECKOUT_HTML: &str = include_str!("../../../demo-frontend/checkout.html");
const DEFAULT_CONFIRMATION_HTML: &str = include_str!("../../../demo-frontend/confirmation.html");
const DEFAULT_STYLES_CSS: &str = include_str!("../../../demo-frontend/styles.css");
const DEFAULT_APP_JS: &str = include_str!("../../../demo-frontend/app.js");
const DEFAULT_API_JS: &str = include_str!("../../../demo-frontend/api.js");
const DEFAULT_AUTH_JS: &str = include_str!("../../../demo-frontend/auth.js");
const DEFAULT_CHECKOUT_JS: &str = include_str!("../../../demo-frontend/checkout.js");
const DEFAULT_CHECKOUT_V2_JS: &str = include_str!("../../../demo-frontend/checkout_v2.js");

/// CLI Arguments
#[derive(Parser, Debug)]
#[command(name = "rcommerce-demo")]
#[command(about = "R Commerce Demo Frontend Server - Secure API proxy with static file serving")]
#[command(version)]
struct Cli {
    /// R Commerce API base URL
    #[arg(
        short,
        long,
        env = "RCOMMERCE_API_URL",
        default_value = "http://localhost:8080"
    )]
    api_url: String,

    /// API Key for service-to-service authentication
    #[arg(short, long, env = "RCOMMERCE_API_KEY")]
    api_key: Option<String>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Host to bind the server to
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on
    #[arg(short = 'P', long, default_value = "3000")]
    port: u16,

    /// Directory containing custom frontend files (optional)
    #[arg(short, long)]
    frontend_dir: Option<PathBuf>,

    /// Disable API proxy (serve frontend only)
    #[arg(long)]
    no_proxy: bool,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

/// Configuration file structure
#[derive(Debug, Deserialize, Serialize)]
struct Config {
    api_url: String,
    api_key: Option<String>,
    host: String,
    port: u16,
    frontend_dir: Option<PathBuf>,
    no_proxy: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_url: "http://localhost:8080".to_string(),
            api_key: None,
            host: "127.0.0.1".to_string(),
            port: 3000,
            frontend_dir: None,
            no_proxy: false,
        }
    }
}

/// Application state shared across handlers
#[derive(Clone)]
struct AppState {
    api_url: String,
    api_key: Option<String>,
    http_client: reqwest::Client,
    frontend_dir: Option<PathBuf>,
    no_proxy: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(&cli.log_level)
        .init();

    // Load configuration
    let config = if let Some(config_path) = &cli.config {
        load_config(config_path)?
    } else {
        Config::default()
    };

    // Merge CLI args with config (CLI takes precedence)
    let api_url = if cli.api_url != "http://localhost:8080" {
        cli.api_url
    } else {
        config.api_url
    };

    let api_key = cli.api_key.or(config.api_key);
    let host = if cli.host != "127.0.0.1" {
        cli.host
    } else {
        config.host
    };
    let port = if cli.port != 3000 {
        cli.port
    } else {
        config.port
    };
    let frontend_dir = cli.frontend_dir.or(config.frontend_dir);
    let no_proxy = cli.no_proxy || config.no_proxy;

    // Validate API key if proxy is enabled
    if !no_proxy && api_key.is_none() {
        warn!("No API key provided. API proxy requests will fail with 401 Unauthorized.");
        warn!("Set API key with --api-key or RCOMMERCE_API_KEY environment variable.");
    }

    // Create HTTP client for proxying
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Create application state
    let state = AppState {
        api_url: api_url.clone(),
        api_key,
        http_client,
        frontend_dir: frontend_dir.clone(),
        no_proxy,
    };

    // Build router
    let app = create_router(state);

    // Parse bind address
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    // Print startup banner
    print_banner(&api_url, &addr, no_proxy, frontend_dir.is_some());

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Load configuration from TOML file
fn load_config(path: &PathBuf) -> anyhow::Result<Config> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;
    let config: Config = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;
    Ok(config)
}

/// Create the Axum router
fn create_router(state: AppState) -> Router {
    // API proxy routes - these go to the backend
    let api_routes = Router::new().route("/api/{*path}", any(api_proxy_handler));

    // Static file routes - serve frontend files
    let static_routes = Router::new()
        .route("/styles.css", get(styles_handler))
        .route("/app.js", get(app_js_handler))
        .route("/api.js", get(api_js_handler))
        .route("/auth.js", get(auth_js_handler))
        .route("/checkout.js", get(checkout_js_handler))
        .route("/checkout_v2.js", get(checkout_v2_js_handler));

    // HTML page routes
    let page_routes = Router::new()
        .route("/", get(index_handler))
        .route("/product", get(product_handler))
        .route("/cart", get(cart_handler))
        .route("/checkout", get(checkout_handler))
        .route("/confirmation", get(confirmation_handler));

    // Combine all routes

    Router::new()
        .merge(api_routes)
        .merge(static_routes)
        .merge(page_routes)
        .fallback(fallback_handler)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Print startup banner
fn print_banner(api_url: &str, addr: &SocketAddr, no_proxy: bool, custom_frontend: bool) {
    println!(
        "\n{}",
        "╔═══════════════════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║                                                               ║".bright_cyan()
    );
    println!(
        "{}",
        "║           🛒 R Commerce Demo Frontend Server                  ║"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "║                                                               ║".bright_cyan()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════════════════════════════╝".bright_cyan()
    );
    println!();
    println!(
        "  {} {}",
        "Server URL:".bold(),
        format!("http://{}", addr).bright_green()
    );
    println!("  {} {}", "API Backend:".bold(), api_url.bright_blue());

    if no_proxy {
        println!("  {} {}", "API Proxy:".bold(), "Disabled".yellow());
    } else {
        println!("  {} {}", "API Proxy:".bold(), "Enabled".bright_green());
    }

    if custom_frontend {
        println!(
            "  {} {}",
            "Frontend:".bold(),
            "Custom directory".bright_green()
        );
    } else {
        println!(
            "  {} {}",
            "Frontend:".bold(),
            "Embedded (default)".bright_green()
        );
    }

    println!();
    println!("  {}", "Open your browser at:".dimmed());
    println!(
        "  {}",
        format!("  http://{}", addr).bright_cyan().underline()
    );
    println!();
    println!("  {} \n", "Press Ctrl+C to stop".dimmed());
}

/// API proxy handler - forwards requests to the backend with API key
async fn api_proxy_handler(
    State(state): State<AppState>,
    Path(path): Path<String>,
    req: Request<Body>,
) -> impl IntoResponse {
    if state.no_proxy {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "API proxy is disabled".to_string(),
        )
            .into_response();
    }

    // Build target URL
    let target_url = format!("{}/api/{}", state.api_url, path);

    // Build headers
    let mut headers = HeaderMap::new();

    // Copy relevant headers from incoming request
    for (name, value) in req.headers() {
        if name != header::HOST && name != header::AUTHORIZATION {
            headers.insert(name.clone(), value.clone());
        }
    }

    // Add API key authorization (server-side, never exposed to client)
    if let Some(api_key) = &state.api_key {
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", api_key).parse().unwrap(),
        );
    }

    // Forward the request
    let method = req.method().clone();
    let body_bytes = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read request body: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {}", e),
            )
                .into_response();
        }
    };

    let request_builder = state
        .http_client
        .request(method, &target_url)
        .headers(headers)
        .body(body_bytes);

    match request_builder.send().await {
        Ok(response) => {
            let status = StatusCode::from_u16(response.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

            let mut response_builder = Response::builder().status(status);

            // Copy headers from backend response
            for (name, value) in response.headers() {
                if name != header::TRANSFER_ENCODING && name != header::CONTENT_ENCODING {
                    response_builder = response_builder.header(name, value);
                }
            }

            match response.bytes().await {
                Ok(body) => response_builder.body(Body::from(body)).unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response()
                }),
                Err(e) => {
                    error!("Failed to read response body: {}", e);
                    (StatusCode::BAD_GATEWAY, "Failed to read backend response").into_response()
                }
            }
        }
        Err(e) => {
            error!("Failed to proxy request to {}: {}", target_url, e);
            (
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to backend: {}", e),
            )
                .into_response()
        }
    }
}

/// Handler for index.html
async fn index_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_html(state, "index.html", DEFAULT_INDEX_HTML).await
}

/// Handler for product.html
async fn product_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_html(state, "product.html", DEFAULT_PRODUCT_HTML).await
}

/// Handler for cart.html
async fn cart_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_html(state, "cart.html", DEFAULT_CART_HTML).await
}

/// Handler for checkout.html
async fn checkout_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_html(state, "checkout.html", DEFAULT_CHECKOUT_HTML).await
}

/// Handler for confirmation.html
async fn confirmation_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_html(state, "confirmation.html", DEFAULT_CONFIRMATION_HTML).await
}

/// Generic HTML handler with config injection
async fn serve_html(state: AppState, filename: &str, default_content: &str) -> impl IntoResponse {
    let content = if let Some(frontend_dir) = &state.frontend_dir {
        // Try to read from custom frontend directory
        let path = frontend_dir.join(filename);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(e) => {
                warn!(
                    "Failed to read {} from frontend dir: {}, using default",
                    filename, e
                );
                default_content.to_string()
            }
        }
    } else {
        default_content.to_string()
    };

    // Inject configuration into HTML
    let injected = inject_config(&content, &state);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html")
        .body(Body::from(injected))
        .unwrap()
}

/// Inject configuration into HTML
fn inject_config(html: &str, state: &AppState) -> String {
    // Create a config object that will be available to JavaScript
    let config_script = format!(
        r#"<script>
window.RCOMMERCE_CONFIG = {{
    API_BASE_URL: '/api/v1',
    PROXY_ENABLED: true,
    API_URL: '{}'
}};
</script>"#,
        state.api_url
    );

    // Insert before closing </head> tag
    if let Some(pos) = html.find("</head>") {
        let mut result = html[..pos].to_string();
        result.push_str(&config_script);
        result.push_str(&html[pos..]);
        result
    } else {
        // If no </head>, prepend to body
        format!("{}\n{}", config_script, html)
    }
}

/// Handler for styles.css
async fn styles_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_static_file(state, "styles.css", DEFAULT_STYLES_CSS, "text/css").await
}

/// Handler for app.js
async fn app_js_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_static_file(state, "app.js", DEFAULT_APP_JS, "application/javascript").await
}

/// Handler for api.js - injects config and modifies for proxy mode
async fn api_js_handler(State(state): State<AppState>) -> impl IntoResponse {
    let content = if let Some(frontend_dir) = &state.frontend_dir {
        let path = frontend_dir.join("api.js");
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(_) => DEFAULT_API_JS.to_string(),
        }
    } else {
        DEFAULT_API_JS.to_string()
    };

    // Modify api.js to use relative URLs and remove API key
    let modified = modify_api_js(&content);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/javascript")
        .body(Body::from(modified))
        .unwrap()
}

/// Modify api.js to work with proxy
fn modify_api_js(js: &str) -> String {
    // Replace hardcoded API_BASE_URL with config-based one
    let mut modified = js.replace(
        "const API_BASE_URL = 'http://localhost:8080/api/v1';",
        "const API_BASE_URL = window.RCOMMERCE_CONFIG?.API_BASE_URL || '/api/v1';",
    );

    // Remove API key constant (it's handled server-side)
    modified = modified.replace(
        "const API_KEY = '';  // Format: 'ak_prefix.secret'",
        "const API_KEY = null; // API key handled server-side by proxy",
    );

    // Modify apiKeyFetch to not send Authorization header (proxy adds it)
    modified = modified.replace(
        "async apiKeyFetch(url, options = {}) {",
        "async apiKeyFetch(url, options = {}) {\n        // API key is added server-side by proxy\n        return fetch(url, options);"
    );

    // Remove the old apiKeyFetch implementation that adds the header
    // This is a simple string replacement - in production you might want
    // to use a proper JS parser
    modified
}

/// Handler for auth.js
async fn auth_js_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_static_file(state, "auth.js", DEFAULT_AUTH_JS, "application/javascript").await
}

/// Handler for checkout.js
async fn checkout_js_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_static_file(
        state,
        "checkout.js",
        DEFAULT_CHECKOUT_JS,
        "application/javascript",
    )
    .await
}

/// Handler for checkout_v2.js
async fn checkout_v2_js_handler(State(state): State<AppState>) -> impl IntoResponse {
    serve_static_file(
        state,
        "checkout_v2.js",
        DEFAULT_CHECKOUT_V2_JS,
        "application/javascript",
    )
    .await
}

/// Generic static file handler
async fn serve_static_file(
    state: AppState,
    filename: &str,
    default_content: &str,
    content_type: &str,
) -> impl IntoResponse {
    let content = if let Some(frontend_dir) = &state.frontend_dir {
        let path = frontend_dir.join(filename);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(_) => default_content.to_string(),
        }
    } else {
        default_content.to_string()
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(content))
        .unwrap()
}

/// Fallback handler for 404s
async fn fallback_handler(uri: Uri) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        format!(
            r#"<!DOCTYPE html>
<html>
<head><title>404 - Not Found</title></head>
<body>
<h1>404 - Page Not Found</h1>
<p>The page '{}' was not found.</p>
<p><a href="/">Go to Home</a></p>
</body>
</html>"#,
            uri.path()
        ),
    )
}
