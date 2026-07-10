//! R Commerce Dynamic Frontend Server
//!
//! Features:
//! - Dynamic routes with parameters (/products/:id)
//! - Server-side rendering with Tera templates
//! - API data fetching with caching
//! - Hot reload in development mode
//! - Edge caching headers for CloudFlare/CDN

use anyhow::Result;
use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, Request, State},
    http::{HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::{Datelike, Utc};
use clap::Parser;
use serde::Deserialize;
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tera::{Context as TeraContext, Tera};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

mod cache;
use cache::{Cache, CacheBackend};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    #[serde(default = "default_bind")]
    pub bind: String,
    pub api_url: String,
    pub api_key: String,

    #[serde(default = "default_template_dir")]
    pub template_dir: PathBuf,

    #[serde(default = "default_static_dir")]
    pub static_dir: PathBuf,

    #[serde(default)]
    pub redis_url: Option<String>,

    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,

    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u32,

    #[serde(default)]
    pub dev_mode: bool,

    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_bind() -> String {
    "0.0.0.0:3000".to_string()
}
fn default_template_dir() -> PathBuf {
    PathBuf::from("templates")
}
fn default_static_dir() -> PathBuf {
    PathBuf::from("static")
}
fn default_cache_ttl() -> u64 {
    300
}
fn default_rate_limit() -> u32 {
    60
}
fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    pub fn load(args: &Args) -> Result<Self> {
        let mut config = config::Config::builder();
        config = config.set_default("bind", default_bind())?;
        config = config.set_default("template_dir", default_template_dir().to_str().unwrap())?;
        config = config.set_default("static_dir", default_static_dir().to_str().unwrap())?;
        config = config.set_default("cache_ttl_secs", default_cache_ttl() as i64)?;
        config = config.set_default("rate_limit_per_minute", default_rate_limit() as i64)?;
        config = config.set_default("dev_mode", false)?;
        config = config.set_default("log_level", default_log_level())?;

        if let Some(ref path) = args.config {
            config = config.add_source(config::File::from(path.as_path()));
        }
        config = config.add_source(config::File::with_name("frontend-server").required(false));
        config = config.add_source(config::Environment::with_prefix("FRONTEND").separator("_"));

        if let Some(ref bind) = args.bind {
            config = config.set_override("bind", bind.as_str())?;
        }
        if let Some(ref api_url) = args.api_url {
            config = config.set_override("api_url", api_url.as_str())?;
        }
        if let Some(ref api_key) = args.api_key {
            config = config.set_override("api_key", api_key.as_str())?;
        }

        let config: Config = config.build()?.try_deserialize()?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.api_url.is_empty() {
            anyhow::bail!("API URL required");
        }
        if self.api_key.is_empty() {
            anyhow::bail!("API key required");
        }
        if !self.template_dir.exists() {
            warn!(
                "Template dir not found, creating: {}",
                self.template_dir.display()
            );
            std::fs::create_dir_all(&self.template_dir)?;
        }
        if !self.static_dir.exists() {
            std::fs::create_dir_all(&self.static_dir)?;
        }
        Ok(())
    }
}

// App state with template engine
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub http_client: reqwest::Client,
    pub cache: Arc<Cache>,
    pub tera: Arc<RwLock<Tera>>,
    pub rate_limiter: Arc<RwLock<RateLimiter>>,
}

pub struct RateLimiter {
    requests: HashMap<String, Vec<Instant>>,
    limit: u32,
    window: Duration,
}

impl RateLimiter {
    pub fn new(limit: u32) -> Self {
        Self {
            requests: HashMap::new(),
            limit,
            window: Duration::from_secs(60),
        }
    }

    pub fn check(&mut self, ip: &str) -> bool {
        let now = Instant::now();
        let window_start = now - self.window;
        let entries = self.requests.entry(ip.to_string()).or_default();
        entries.retain(|&t| t > window_start);

        if entries.len() >= self.limit as usize {
            return false;
        }
        entries.push(now);
        true
    }
}

#[derive(Parser)]
#[command(name = "rcommerce-frontend")]
#[command(about = "Dynamic frontend server with SSR")]
pub struct Args {
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    #[arg(short, long, env = "FRONTEND_BIND")]
    pub bind: Option<String>,
    #[arg(short = 'u', long, env = "FRONTEND_API_URL")]
    pub api_url: Option<String>,
    #[arg(short = 'k', long, env = "FRONTEND_API_KEY")]
    pub api_key: Option<String>,
}

// Route parameters
#[derive(Deserialize)]
struct ProductParams {
    id: String,
}

#[derive(Deserialize)]
struct PageParams {
    slug: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::load(&args)?;

    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    info!("R Commerce Frontend Server v{}", env!("CARGO_PKG_VERSION"));
    info!(
        "Mode: {}",
        if config.dev_mode {
            "DEVELOPMENT"
        } else {
            "PRODUCTION"
        }
    );

    // Initialize Tera template engine
    let template_glob = format!("{}/**/*.html", config.template_dir.display());
    let tera = match Tera::new(&template_glob) {
        Ok(t) => {
            info!("Loaded {} templates", t.get_template_names().count());
            t
        }
        Err(e) => {
            warn!("Failed to load templates: {}. Using default.", e);
            Tera::default()
        }
    };

    // Initialize cache
    let cache_backend: Arc<dyn CacheBackend> = match &config.redis_url {
        Some(url) => match cache::RedisCache::new(url, config.cache_ttl_secs).await {
            Ok(redis) => Arc::new(redis),
            Err(e) => {
                warn!("Redis failed: {}. Using memory cache.", e);
                Arc::new(cache::MemoryCache::new(1000, config.cache_ttl_secs))
            }
        },
        None => Arc::new(cache::MemoryCache::new(1000, config.cache_ttl_secs)),
    };

    let state = AppState {
        config: Arc::new(config.clone()),
        http_client: reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?,
        cache: Arc::new(Cache::new(cache_backend)),
        tera: Arc::new(RwLock::new(tera)),
        rate_limiter: Arc::new(RwLock::new(RateLimiter::new(config.rate_limit_per_minute))),
    };

    // Build router with dynamic routes
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Dynamic routes
        .route("/", get(home_page))
        .route("/products/:id", get(product_page))
        .route("/categories/:slug", get(category_page))
        .route("/pages/:slug", get(cms_page))
        .route("/cart", get(cart_page))
        // API proxy
        .route("/api/*path", get(api_proxy))
        // Static files
        .route("/static/*path", get(static_files))
        .with_state(state)
        .layer(axum::middleware::from_fn(security_headers));

    let addr: SocketAddr = config.bind.parse()?;
    info!("Server ready: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

// Handler: Home page
async fn home_page(State(state): State<AppState>) -> Result<Response, StatusCode> {
    // Fetch featured products from API
    let products = fetch_api(&state, "/api/v1/products?featured=true")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut ctx = TeraContext::new();
    ctx.insert("title", "Home");
    ctx.insert("products", &products);
    ctx.insert("api_url", &state.config.api_url);

    render_template(&state, "index.html", ctx).await
}

// Handler: Product detail page
async fn product_page(
    State(state): State<AppState>,
    Path(params): Path<ProductParams>,
) -> Result<Response, StatusCode> {
    // Fetch product from API
    let product = fetch_api(&state, &format!("/api/v1/products/{}", params.id))
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let mut ctx = TeraContext::new();
    ctx.insert("title", &product["name"].as_str().unwrap_or("Product"));
    ctx.insert("product", &product);
    ctx.insert("api_url", &state.config.api_url);

    render_template(&state, "product.html", ctx).await
}

// Handler: Category page
async fn category_page(
    State(state): State<AppState>,
    Path(params): Path<PageParams>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let page = query.get("page").unwrap_or(&"1".to_string()).clone();

    let products = fetch_api(
        &state,
        &format!("/api/v1/products?category={}&page={}", params.slug, page),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut ctx = TeraContext::new();
    ctx.insert("title", &format!("Category: {}", params.slug));
    ctx.insert("category", &params.slug);
    ctx.insert("products", &products);
    ctx.insert("page", &page);

    render_template(&state, "category.html", ctx).await
}

// Handler: CMS page
async fn cms_page(
    State(state): State<AppState>,
    Path(params): Path<PageParams>,
) -> Result<Response, StatusCode> {
    let mut ctx = TeraContext::new();
    ctx.insert("title", &params.slug);
    ctx.insert("slug", &params.slug);

    render_template(&state, "page.html", ctx).await
}

// Handler: Cart page
async fn cart_page(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let mut ctx = TeraContext::new();
    ctx.insert("title", "Shopping Cart");
    ctx.insert("api_url", &state.config.api_url);

    render_template(&state, "cart.html", ctx).await
}

// Helper: Render template
async fn render_template(
    state: &AppState,
    template_name: &str,
    mut ctx: TeraContext,
) -> Result<Response, StatusCode> {
    // Add global context
    ctx.insert("store_name", "My Store");
    ctx.insert("current_year", &Utc::now().year());
    let tera = state.tera.read().await;

    match tera.render(template_name, &ctx) {
        Ok(html) => {
            let mut response = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html; charset=utf-8");

            // Add edge caching headers for HTML
            response = response
                .header(
                    "Cache-Control",
                    "public, max-age=60, stale-while-revalidate=300",
                )
                .header("CDN-Cache-Control", "max-age=60");

            Ok(response.body(Body::from(html)).unwrap())
        }
        Err(e) => {
            error!("Template error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Helper: Fetch from API with caching
async fn fetch_api(state: &AppState, path: &str) -> Result<serde_json::Value, anyhow::Error> {
    let cache_key = format!("api:{}", path);

    // Try cache
    if let Ok(Some(cached)) = state.cache.get(&cache_key).await {
        return Ok(serde_json::from_slice(&cached.body)?);
    }

    // Fetch from API
    let url = format!("{}{}", state.config.api_url, path);
    let response = state
        .http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", state.config.api_key))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("API error: {}", response.status());
    }

    let data: serde_json::Value = response.json().await?;

    // Cache the response
    let cached = cache::CachedResponse {
        body: serde_json::to_vec(&data)?,
        content_type: "application/json".to_string(),
        cached_at: chrono::Utc::now(),
        backend: String::new(),
    };
    let _ = state
        .cache
        .set(&cache_key, &cached, state.config.cache_ttl_secs)
        .await;

    Ok(data)
}

// Handler: API proxy
async fn api_proxy(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
) -> Result<Response, StatusCode> {
    // Rate limiting
    {
        let mut limiter = state.rate_limiter.write().await;
        if !limiter.check(&addr.ip().to_string()) {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    let path = req.uri().path();
    let method = req.method().clone();
    let target_url = format!("{}{}", state.config.api_url, path);

    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let response = state
        .http_client
        .request(method, &target_url)
        .header("Authorization", format!("Bearer {}", state.config.api_key))
        .body(body_bytes)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::OK);
    let body = response
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    Ok(Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap())
}

// Handler: Static files
async fn static_files(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches("/static/");
    let file_path = state.config.static_dir.join(path);

    if !file_path.exists() || !file_path.is_file() {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .unwrap();
    }

    match tokio::fs::read(&file_path).await {
        Ok(content) => {
            let content_type = mime_guess::from_path(&file_path)
                .first_or_octet_stream()
                .to_string();

            let mut response = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", content_type);

            // Long cache for static assets
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(
                ext,
                "css" | "js" | "png" | "jpg" | "jpeg" | "gif" | "svg" | "woff" | "woff2"
            ) {
                response = response
                    .header("Cache-Control", "public, max-age=2592000, immutable")
                    .header("CDN-Cache-Control", "max-age=2592000");
            }

            response.body(Body::from(content)).unwrap()
        }
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Error reading file"))
            .unwrap(),
    }
}

async fn health_check() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "service": "rcommerce-frontend"
    }))
}

async fn security_headers(req: Request, next: axum::middleware::Next) -> impl IntoResponse {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    headers.insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );
    headers.insert(
        "Referrer-Policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    response
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    let terminate = async {
        let mut sig =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok()?;
        sig.recv().await
    };
    tokio::select! {
        _ = ctrl_c => info!("Shutting down..."),
        _ = terminate => info!("Shutting down..."),
    }
}
