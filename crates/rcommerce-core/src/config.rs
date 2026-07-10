use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Main configuration structure for R commerce
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    #[serde(default)]
    pub cache: CacheConfig,

    #[serde(default)]
    pub security: SecurityConfig,

    #[serde(default)]
    pub media: MediaConfig,

    #[serde(default)]
    pub notifications: NotificationConfig,

    #[serde(default)]
    pub rate_limiting: RateLimitConfig,

    #[serde(default)]
    pub features: FeatureFlags,

    #[serde(default)]
    pub import: ImportConfig,

    #[serde(default)]
    pub tls: TlsConfig,

    #[serde(default)]
    pub dunning: DunningConfig,

    #[serde(default)]
    pub payment: PaymentConfig,

    #[serde(default)]
    pub shipping: ShippingConfig,

    #[serde(default)]
    pub tax: TaxConfig,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &str) -> Result<Self, crate::Error> {
        use crate::Error;

        let contents = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("Failed to read config file: {}", e)))?;

        let config: Config = toml::from_str(&contents)
            .map_err(|e| Error::Config(format!("Failed to parse config: {}", e)))?;

        config.validate()?;

        Ok(config)
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, crate::Error> {
        // Try to load from RCOMMERCE_CONFIG env var first
        if let Ok(config_path) = std::env::var("RCOMMERCE_CONFIG") {
            return Self::load(&config_path);
        }

        // Try default locations
        let default_paths = [
            "./config/default.toml",
            "./config/production.toml",
            "/etc/rcommerce/config.toml",
        ];

        for path in &default_paths {
            if Path::new(path).exists() {
                return Self::load(path);
            }
        }

        // Return default config if no file found
        Ok(Self::default())
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), crate::Error> {
        use crate::Error;

        // Validate server config
        if self.server.port == 0 {
            return Err(Error::Config("Invalid server port".to_string()));
        }

        // Validate database config
        if self.database.pool_size == 0 {
            return Err(Error::Config("Database pool size must be > 0".to_string()));
        }

        // Validate cache config
        if self.cache.max_size_mb > Some(10000) {
            return Err(Error::Config("Cache size too large (max 10GB)".to_string()));
        }

        // Validate JWT secret
        if self.security.jwt.secret.is_empty() {
            return Err(Error::Config(
                "JWT secret is not configured. Please set security.jwt.secret in your config file."
                    .to_string(),
            ));
        }
        if self.security.jwt.secret.len() < 32 {
            return Err(Error::Config(
                "JWT secret must be at least 32 bytes long".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_workers")]
    pub worker_threads: usize,

    #[serde(default = "default_graceful_shutdown")]
    pub graceful_shutdown_timeout_secs: u64,

    #[serde(default)]
    pub cors: CorsConfig,

    #[serde(default)]
    pub limits: LimitsConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            worker_threads: default_workers(),
            graceful_shutdown_timeout_secs: default_graceful_shutdown(),
            cors: CorsConfig::default(),
            limits: LimitsConfig::default(),
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_workers() -> usize {
    0 // 0 means use number of CPU cores
}

fn default_graceful_shutdown() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub allowed_origins: Vec<String>,

    #[serde(default)]
    pub allowed_methods: Vec<String>,

    #[serde(default)]
    pub allowed_headers: Vec<String>,

    #[serde(default = "default_true")]
    pub allow_credentials: bool,

    #[serde(default)]
    pub max_age: Option<u64>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
                .into_iter()
                .map(String::from)
                .collect(),
            allowed_headers: vec!["Content-Type", "Authorization", "X-Requested-With"]
                .into_iter()
                .map(String::from)
                .collect(),
            allow_credentials: true,
            max_age: Some(3600),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    #[serde(default = "default_max_request_size")]
    pub max_request_size_mb: u64,

    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u64,

    #[serde(default = "default_burst")]
    pub rate_limit_burst: u64,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_request_size_mb: default_max_request_size(),
            rate_limit_per_minute: default_rate_limit(),
            rate_limit_burst: default_burst(),
        }
    }
}

fn default_max_request_size() -> u64 {
    10 // 10MB
}

fn default_rate_limit() -> u64 {
    1000 // requests per minute
}

fn default_burst() -> u64 {
    200
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_type")]
    pub db_type: DatabaseType,

    pub host: String,

    #[serde(default = "default_db_port")]
    pub port: u16,

    pub database: String,
    pub username: String,
    pub password: String,

    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    #[serde(default)]
    pub ssl_mode: SslMode,
}

impl DatabaseConfig {
    /// Build database connection URL
    pub fn url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database
        )
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_type: default_db_type(),
            host: "localhost".to_string(),
            port: default_db_port(),
            database: "rcommerce".to_string(),
            username: "rcommerce".to_string(),
            password: "password".to_string(),
            pool_size: default_pool_size(),
            ssl_mode: SslMode::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DatabaseType {
    Postgres,
}

fn default_db_type() -> DatabaseType {
    DatabaseType::Postgres
}

fn default_db_port() -> u16 {
    5432 // PostgreSQL default
}

fn default_pool_size() -> u32 {
    20
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum SslMode {
    Disable,
    #[default]
    Prefer,
    Require,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default = "default_log_format")]
    pub format: LogFormat,

    #[serde(default)]
    pub file: Option<FileLogConfig>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file: None,
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> LogFormat {
    LogFormat::Json
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogFormat {
    Json,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLogConfig {
    pub path: String,
    pub rotation: LogRotation,
    #[serde(default)]
    pub max_size_mb: Option<u64>,
    #[serde(default)]
    pub max_files: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogRotation {
    Daily,
    Hourly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_type")]
    pub cache_type: CacheType,

    #[serde(default = "default_cache_max_size")]
    pub max_size_mb: Option<u64>,

    pub redis_url: Option<String>,

    #[serde(default = "default_redis_pool_size")]
    pub redis_pool_size: u32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            cache_type: default_cache_type(),
            max_size_mb: default_cache_max_size(),
            redis_url: None,
            redis_pool_size: default_redis_pool_size(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CacheType {
    Memory,
    Redis,
}

fn default_cache_type() -> CacheType {
    CacheType::Memory
}

fn default_cache_max_size() -> Option<u64> {
    Some(100) // 100MB
}

fn default_redis_pool_size() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_api_key_prefix_length")]
    pub api_key_prefix_length: usize,

    #[serde(default = "default_api_secret_length")]
    pub api_key_secret_length: usize,

    #[serde(default)]
    pub jwt: JwtConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            api_key_prefix_length: default_api_key_prefix_length(),
            api_key_secret_length: default_api_secret_length(),
            jwt: JwtConfig::default(),
        }
    }
}

fn default_api_key_prefix_length() -> usize {
    8
}

fn default_api_secret_length() -> usize {
    32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    #[serde(default = "default_jwt_secret")]
    pub secret: String,

    #[serde(default = "default_jwt_expiry")]
    pub expiry_hours: u64,

    #[serde(default = "default_jwt_refresh_expiry")]
    pub refresh_expiry_hours: u64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: default_jwt_secret(),
            expiry_hours: default_jwt_expiry(),
            refresh_expiry_hours: default_jwt_refresh_expiry(),
        }
    }
}

fn default_jwt_secret() -> String {
    // JWT secret must be explicitly configured
    // Return empty string to force validation failure if not set
    String::new()
}

fn default_jwt_expiry() -> u64 {
    24 // hours
}

fn default_jwt_refresh_expiry() -> u64 {
    24 * 7 // 7 days
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaConfig {
    #[serde(default = "default_storage_type")]
    pub storage_type: StorageType,

    pub local_path: Option<String>,
    pub local_base_url: Option<String>,

    pub s3_region: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
    pub s3_endpoint: Option<String>,

    #[serde(default)]
    pub image_processing: ImageProcessingConfig,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            storage_type: default_storage_type(),
            local_path: Some("./uploads".to_string()),
            local_base_url: Some("http://localhost:8080/uploads".to_string()),
            s3_region: None,
            s3_bucket: None,
            s3_access_key: None,
            s3_secret_key: None,
            s3_endpoint: None,
            image_processing: ImageProcessingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StorageType {
    Local,
    S3,
    Gcs,
    Azure,
}

fn default_storage_type() -> StorageType {
    StorageType::Local
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageProcessingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_image_quality")]
    pub default_quality: u8,

    #[serde(default = "default_image_sizes")]
    pub sizes: Vec<ImageSize>,
}

impl Default for ImageProcessingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_quality: default_image_quality(),
            sizes: default_image_sizes(),
        }
    }
}

fn default_image_quality() -> u8 {
    85
}

fn default_image_sizes() -> Vec<ImageSize> {
    vec![
        ImageSize {
            name: "thumbnail".to_string(),
            width: 150,
            height: 150,
            crop: true,
        },
        ImageSize {
            name: "small".to_string(),
            width: 300,
            height: 300,
            crop: false,
        },
        ImageSize {
            name: "medium".to_string(),
            width: 600,
            height: 600,
            crop: false,
        },
        ImageSize {
            name: "large".to_string(),
            width: 1200,
            height: 1200,
            crop: false,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSize {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub crop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub email: EmailConfig,

    #[serde(default)]
    pub sms: SmsConfig,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            email: EmailConfig::default(),
            sms: SmsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    #[serde(default)]
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_user: Option<String>,
    pub smtp_pass: Option<String>,
    pub from_name: Option<String>,
    pub from_email: Option<String>,
    #[serde(default = "default_true")]
    pub smtp_tls: bool,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_host: None,
            smtp_port: None,
            smtp_user: None,
            smtp_pass: None,
            from_name: None,
            from_email: None,
            smtp_tls: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SmsConfig {
    #[serde(default)]
    pub provider: Option<String>,
    pub twilio_account_sid: Option<String>,
    pub twilio_auth_token: Option<String>,
    pub twilio_from_number: Option<String>,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum requests per minute from a single IP
    #[serde(default = "default_rate_limit_minute")]
    pub requests_per_minute: u32,

    /// Maximum requests per hour from a single IP
    #[serde(default = "default_rate_limit_hour")]
    pub requests_per_hour: u32,

    /// Maximum requests per day from a single IP
    #[serde(default = "default_rate_limit_day")]
    pub requests_per_day: u32,

    /// Maximum concurrent connections per IP
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_per_ip: u32,

    /// Enable API key rate limiting (more permissive)
    #[serde(default = "default_true")]
    pub api_key_limiting: bool,

    /// Maximum requests per minute with valid API key
    #[serde(default = "default_rate_limit_api_key")]
    pub api_key_requests_per_minute: u32,

    /// Blocklist of IP addresses
    #[serde(default)]
    pub blocklist: Vec<String>,

    /// Allowlist of trusted IP addresses
    #[serde(default)]
    pub allowlist: Vec<String>,

    /// Enable DDoS protection mode (stricter limits under attack)
    #[serde(default = "default_true")]
    pub ddos_protection: bool,

    /// Response headers to include rate limit info
    #[serde(default = "default_true")]
    pub expose_headers: bool,

    /// Store rate limit data in Redis (true) or memory (false)
    #[serde(default = "default_false")]
    pub use_redis: bool,

    /// Redis connection string
    #[serde(default)]
    pub redis_url: Option<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 60,
            requests_per_hour: 1000,
            requests_per_day: 10000,
            max_concurrent_per_ip: 10,
            api_key_limiting: true,
            api_key_requests_per_minute: 1000,
            blocklist: vec![],
            allowlist: vec![],
            ddos_protection: true,
            expose_headers: true,
            use_redis: false,
            redis_url: None,
        }
    }
}

// Default value helper functions
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_rate_limit_minute() -> u32 {
    60
}
fn default_rate_limit_hour() -> u32 {
    1000
}
fn default_rate_limit_day() -> u32 {
    10000
}
fn default_max_concurrent() -> u32 {
    10
}
fn default_rate_limit_api_key() -> u32 {
    1000
}

/// Feature flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    #[serde(default = "default_true")]
    pub debug_api: bool,

    #[serde(default = "default_true")]
    pub metrics: bool,

    #[serde(default = "default_true")]
    pub health_check: bool,

    #[serde(default)]
    pub experimental: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            debug_api: true,
            metrics: true,
            health_check: true,
            experimental: false,
        }
    }
}

/// Import configuration for platform API keys and settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportConfig {
    /// Shopify API configuration
    #[serde(default)]
    pub shopify: Option<PlatformImportConfig>,

    /// WooCommerce API configuration
    #[serde(default)]
    pub woocommerce: Option<PlatformImportConfig>,

    /// Magento API configuration
    #[serde(default)]
    pub magento: Option<PlatformImportConfig>,

    /// Medusa API configuration
    #[serde(default)]
    pub medusa: Option<PlatformImportConfig>,

    /// Default import options
    #[serde(default)]
    pub default_options: DefaultImportOptions,
}

/// Platform-specific import configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformImportConfig {
    /// API base URL
    pub api_url: String,

    /// API key or access token
    pub api_key: String,

    /// API secret (for WooCommerce)
    #[serde(default)]
    pub api_secret: Option<String>,

    /// Additional headers
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,

    /// Default entity types to import
    #[serde(default = "default_import_entities")]
    pub entities: Vec<String>,
}

/// Default import options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultImportOptions {
    /// Default batch size
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Skip existing records by default
    #[serde(default = "default_true")]
    pub skip_existing: bool,

    /// Continue on error by default
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
}

impl Default for DefaultImportOptions {
    fn default() -> Self {
        Self {
            batch_size: 100,
            skip_existing: true,
            continue_on_error: true,
        }
    }
}

fn default_import_entities() -> Vec<String> {
    vec!["all".to_string()]
}

fn default_batch_size() -> usize {
    100
}

/// TLS configuration for secure HTTPS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS/HTTPS
    #[serde(default = "default_false")]
    pub enabled: bool,

    /// Minimum TLS version (1.3 strongly recommended)
    #[serde(default = "default_min_tls_version")]
    pub min_tls_version: TlsVersion,

    /// Maximum TLS version
    #[serde(default = "default_max_tls_version")]
    pub max_tls_version: TlsVersion,

    /// Certificate file path (for manual certs)
    pub cert_file: Option<PathBuf>,

    /// Private key file path (for manual certs)
    pub key_file: Option<PathBuf>,

    /// Let's Encrypt configuration
    #[serde(default)]
    pub lets_encrypt: Option<LetsEncryptConfig>,

    /// HSTS (HTTP Strict Transport Security) configuration
    #[serde(default)]
    pub hsts: Option<HstsConfig>,

    /// Cipher suites (defaults to modern, secure ciphers)
    #[serde(default)]
    pub cipher_suites: Vec<String>,

    /// Enable OCSP stapling
    #[serde(default = "default_true")]
    pub ocsp_stapling: bool,

    /// HTTP port for ACME challenges and redirects (default: 80)
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    /// HTTPS port (default: 443)
    #[serde(default = "default_https_port")]
    pub https_port: u16,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_tls_version: default_min_tls_version(),
            max_tls_version: default_max_tls_version(),
            cert_file: None,
            key_file: None,
            lets_encrypt: None,
            hsts: None,
            cipher_suites: vec![],
            ocsp_stapling: true,
            http_port: 80,
            https_port: 443,
        }
    }
}

impl TlsConfig {
    /// Validate TLS configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled {
            // Check if either manual certs or Let's Encrypt is configured
            let has_manual_certs = self.cert_file.is_some() && self.key_file.is_some();
            let has_lets_encrypt = self
                .lets_encrypt
                .as_ref()
                .map(|le| le.enabled)
                .unwrap_or(false);

            if !has_manual_certs && !has_lets_encrypt {
                return Err(
                    "Either certificate files or Let's Encrypt must be configured when TLS is enabled".to_string(),
                );
            }

            // Verify TLS version is at least 1.2 (1.3 recommended)
            if self.min_tls_version < TlsVersion::Tls1_2 {
                return Err("Minimum TLS version must be 1.2 or higher".to_string());
            }
        }

        Ok(())
    }

    /// Check if this is using Let's Encrypt
    pub fn uses_lets_encrypt(&self) -> bool {
        self.enabled
            && self
                .lets_encrypt
                .as_ref()
                .map(|le| le.enabled)
                .unwrap_or(false)
    }

    /// Check if this is using manual certificates
    pub fn uses_manual_certs(&self) -> bool {
        self.enabled && self.cert_file.is_some() && self.key_file.is_some()
    }
}

/// TLS version enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum TlsVersion {
    #[serde(rename = "1.2")]
    Tls1_2,
    #[serde(rename = "1.3")]
    #[default]
    Tls1_3,
}

fn default_min_tls_version() -> TlsVersion {
    TlsVersion::Tls1_3
}

fn default_max_tls_version() -> TlsVersion {
    TlsVersion::Tls1_3
}

fn default_http_port() -> u16 {
    80
}

fn default_https_port() -> u16 {
    443
}

/// Let's Encrypt configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetsEncryptConfig {
    /// Enable automatic certificate provisioning
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Contact email for Let's Encrypt account
    pub email: String,

    /// Domain(s) to get certificates for
    pub domains: Vec<String>,

    /// ACME directory URL (production or staging)
    #[serde(default = "default_acme_directory")]
    pub acme_directory: String,

    /// Use staging server for testing (default: false)
    #[serde(default)]
    pub use_staging: bool,

    /// Certificate renewal threshold (days before expiry)
    #[serde(default = "default_renewal_days")]
    pub renewal_threshold_days: i32,

    /// Auto-renew certificates
    #[serde(default = "default_true")]
    pub auto_renew: bool,

    /// Certificate cache directory
    #[serde(default = "default_cert_cache_dir")]
    pub cache_dir: PathBuf,
}

impl Default for LetsEncryptConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            email: String::new(),
            domains: vec![],
            acme_directory: default_acme_directory(),
            use_staging: false,
            renewal_threshold_days: 30,
            auto_renew: true,
            cache_dir: default_cert_cache_dir(),
        }
    }
}

impl LetsEncryptConfig {
    /// Validate Let's Encrypt configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled {
            if self.email.is_empty() {
                return Err("Let's Encrypt email is required".to_string());
            }

            if self.domains.is_empty() {
                return Err("At least one domain is required for Let's Encrypt".to_string());
            }

            // Validate domains
            for domain in &self.domains {
                if !domain.contains('.') {
                    return Err(format!("Invalid domain: {}", domain));
                }
            }
        }

        Ok(())
    }
}

/// HSTS (HTTP Strict Transport Security) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HstsConfig {
    /// Enable HSTS
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Max age in seconds (default: 1 year)
    #[serde(default = "default_hsts_max_age")]
    pub max_age: u64,

    /// Include subdomains
    #[serde(default = "default_true")]
    pub include_subdomains: bool,

    /// Preload in browsers
    #[serde(default)]
    pub preload: bool,
}

impl Default for HstsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_age: 31_536_000, // 1 year in seconds
            include_subdomains: true,
            preload: false,
        }
    }
}

impl HstsConfig {
    /// Generate HSTS header value
    pub fn header_value(&self) -> String {
        let mut parts = vec![format!("max-age={}", self.max_age)];

        if self.include_subdomains {
            parts.push("includeSubDomains".to_string());
        }

        if self.preload {
            parts.push("preload".to_string());
        }

        parts.join("; ")
    }
}

fn default_acme_directory() -> String {
    "https://acme-v02.api.letsencrypt.org/directory".to_string()
}

fn default_renewal_days() -> i32 {
    30
}

fn default_cert_cache_dir() -> PathBuf {
    PathBuf::from("/var/lib/rcommerce/certs")
}

fn default_hsts_max_age() -> u64 {
    31_536_000 // 1 year
}

/// Dunning (payment retry) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DunningConfig {
    /// Enable dunning process
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Number of retry attempts before cancellation
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,

    /// Retry intervals in days (e.g., [1, 3, 7] = retry after 1 day, 3 days, 7 days)
    #[serde(default = "default_retry_intervals")]
    pub retry_intervals_days: Vec<i32>,

    /// Grace period in days (subscription remains active during retries)
    #[serde(default = "default_grace_period_days")]
    pub grace_period_days: i32,

    /// Send email on first failure
    #[serde(default = "default_true")]
    pub email_on_first_failure: bool,

    /// Send email on final failure (before cancellation)
    #[serde(default = "default_true")]
    pub email_on_final_failure: bool,

    /// Apply late fees after N retries (None = no late fees)
    #[serde(default)]
    pub late_fee_after_retry: Option<i32>,

    /// Late fee amount
    #[serde(default)]
    pub late_fee_amount: Option<rust_decimal::Decimal>,

    /// Per-gateway dunning configurations
    #[serde(default)]
    pub gateway_configs: std::collections::HashMap<String, GatewayDunningConfig>,

    /// Email template configuration
    #[serde(default)]
    pub email_templates: DunningEmailTemplates,

    /// Background job interval in minutes
    #[serde(default = "default_dunning_job_interval")]
    pub job_interval_minutes: i32,
}

impl Default for DunningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 3,
            retry_intervals_days: vec![1, 3, 7],
            grace_period_days: 14,
            email_on_first_failure: true,
            email_on_final_failure: true,
            late_fee_after_retry: None,
            late_fee_amount: None,
            gateway_configs: std::collections::HashMap::new(),
            email_templates: DunningEmailTemplates::default(),
            job_interval_minutes: 60,
        }
    }
}

/// Per-gateway dunning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayDunningConfig {
    /// Override max retries for this gateway
    #[serde(default)]
    pub max_retries: Option<i32>,

    /// Override retry intervals for this gateway
    #[serde(default)]
    pub retry_intervals_days: Option<Vec<i32>>,

    /// Gateway-specific grace period
    #[serde(default)]
    pub grace_period_days: Option<i32>,

    /// Enable/disable dunning for this gateway
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Dunning email template configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DunningEmailTemplates {
    /// Template for first failure notification
    #[serde(default = "default_first_failure_template")]
    pub first_failure: String,

    /// Template for retry failure notification
    #[serde(default = "default_retry_failure_template")]
    pub retry_failure: String,

    /// Template for final notice before cancellation
    #[serde(default = "default_final_notice_template")]
    pub final_notice: String,

    /// Template for cancellation notification
    #[serde(default = "default_cancellation_template")]
    pub cancellation: String,

    /// Template for payment recovered confirmation
    #[serde(default = "default_recovered_template")]
    pub recovered: String,

    /// Email sender name
    #[serde(default)]
    pub from_name: Option<String>,

    /// Email sender address
    #[serde(default)]
    pub from_email: Option<String>,

    /// Reply-to address
    #[serde(default)]
    pub reply_to: Option<String>,
}

impl Default for DunningEmailTemplates {
    fn default() -> Self {
        Self {
            first_failure: default_first_failure_template(),
            retry_failure: default_retry_failure_template(),
            final_notice: default_final_notice_template(),
            cancellation: default_cancellation_template(),
            recovered: default_recovered_template(),
            from_name: None,
            from_email: None,
            reply_to: None,
        }
    }
}

fn default_max_retries() -> i32 {
    3
}

fn default_retry_intervals() -> Vec<i32> {
    vec![1, 3, 7]
}

fn default_grace_period_days() -> i32 {
    14
}

fn default_dunning_job_interval() -> i32 {
    60 // Run every hour
}

fn default_first_failure_template() -> String {
    "dunning/first_failure".to_string()
}

fn default_retry_failure_template() -> String {
    "dunning/retry_failure".to_string()
}

fn default_final_notice_template() -> String {
    "dunning/final_notice".to_string()
}

fn default_cancellation_template() -> String {
    "dunning/cancellation".to_string()
}

fn default_recovered_template() -> String {
    "dunning/recovered".to_string()
}

/// Payment gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaymentConfig {
    /// Default payment gateway ID
    #[serde(default = "default_payment_gateway")]
    pub default_gateway: String,

    /// Enable test mode for payments
    #[serde(default)]
    pub test_mode: bool,

    /// Stripe configuration
    #[serde(default)]
    pub stripe: StripeConfig,

    /// WeChat Pay configuration
    #[serde(default)]
    pub wechatpay: WeChatPayConfig,

    /// AliPay configuration
    #[serde(default)]
    pub alipay: AliPayConfig,

    /// Airwallex configuration
    #[serde(default)]
    pub airwallex: AirwallexConfig,
}

fn default_payment_gateway() -> String {
    "mock".to_string()
}

/// Stripe payment gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StripeConfig {
    /// Enable Stripe gateway
    #[serde(default)]
    pub enabled: bool,

    /// Stripe API secret key
    pub secret_key: Option<String>,

    /// Stripe webhook secret
    pub webhook_secret: Option<String>,

    /// Stripe publishable key (for frontend)
    pub publishable_key: Option<String>,
}

/// WeChat Pay configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeChatPayConfig {
    /// Enable WeChat Pay gateway
    #[serde(default)]
    pub enabled: bool,

    /// Merchant ID (mchid)
    pub mch_id: Option<String>,

    /// App ID
    pub app_id: Option<String>,

    /// API v3 Key
    pub api_key: Option<String>,

    /// API Client Serial Number
    pub serial_no: Option<String>,

    /// Private key for signing (PEM format)
    pub private_key: Option<String>,

    /// Use sandbox environment
    #[serde(default = "default_true")]
    pub sandbox: bool,
}

/// AliPay configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AliPayConfig {
    /// Enable AliPay gateway
    #[serde(default)]
    pub enabled: bool,

    /// App ID
    pub app_id: Option<String>,

    /// Merchant Private Key (RSA2)
    pub private_key: Option<String>,

    /// AliPay Public Key (for verification)
    pub alipay_public_key: Option<String>,

    /// Use sandbox environment
    #[serde(default = "default_true")]
    pub sandbox: bool,
}

/// Airwallex configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AirwallexConfig {
    /// Enable Airwallex gateway
    #[serde(default)]
    pub enabled: bool,

    /// Client ID
    pub client_id: Option<String>,

    /// API Key
    pub api_key: Option<String>,

    /// Webhook secret
    pub webhook_secret: Option<String>,

    /// Use demo environment
    #[serde(default)]
    pub demo: bool,
}

/// Shipping configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShippingConfig {
    /// Default shipping provider
    #[serde(default = "default_shipping_provider")]
    pub default_provider: String,

    /// Enable test mode for shipping (use sandbox APIs)
    #[serde(default)]
    pub test_mode: bool,

    /// Default shipping origin address
    #[serde(default)]
    pub origin: Option<ShippingOriginConfig>,

    /// DHL Express configuration
    #[serde(default)]
    pub dhl: DhlConfig,

    /// FedEx configuration
    #[serde(default)]
    pub fedex: FedExConfig,

    /// UPS configuration
    #[serde(default)]
    pub ups: UpsConfig,

    /// USPS configuration
    #[serde(default)]
    pub usps: UspsConfig,
}

fn default_shipping_provider() -> String {
    "manual".to_string()
}

/// Shipping origin address configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingOriginConfig {
    pub name: String,
    pub address1: String,
    pub address2: Option<String>,
    pub city: String,
    #[serde(alias = "province")]
    pub state: String,
    pub country: String,
    pub zip: String,
    pub phone: Option<String>,
}

impl Default for ShippingOriginConfig {
    fn default() -> Self {
        Self {
            name: "Your Store".to_string(),
            address1: "123 Commerce St".to_string(),
            address2: None,
            city: "San Francisco".to_string(),
            state: "CA".to_string(),
            country: "US".to_string(),
            zip: "94102".to_string(),
            phone: None,
        }
    }
}

/// DHL Express configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DhlConfig {
    /// Enable DHL Express
    #[serde(default)]
    pub enabled: bool,

    /// DHL API Key
    pub api_key: Option<String>,

    /// DHL API Secret
    pub api_secret: Option<String>,

    /// DHL Account Number
    pub account_number: Option<String>,

    /// Use sandbox environment
    #[serde(default)]
    pub sandbox: bool,
}

/// FedEx configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FedExConfig {
    /// Enable FedEx
    #[serde(default)]
    pub enabled: bool,

    /// FedEx API Key (Client ID)
    pub api_key: Option<String>,

    /// FedEx API Secret (Client Secret)
    pub api_secret: Option<String>,

    /// FedEx Account Number
    pub account_number: Option<String>,

    /// Use sandbox environment
    #[serde(default)]
    pub sandbox: bool,
}

/// UPS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsConfig {
    /// Enable UPS
    #[serde(default)]
    pub enabled: bool,

    /// UPS API Key (Client ID)
    pub api_key: Option<String>,

    /// UPS Username
    pub username: Option<String>,

    /// UPS Password
    pub password: Option<String>,

    /// UPS Account Number
    pub account_number: Option<String>,

    /// Use sandbox environment (Customer Integration Environment)
    #[serde(default)]
    pub sandbox: bool,
}

/// USPS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UspsConfig {
    /// Enable USPS
    #[serde(default)]
    pub enabled: bool,

    /// USPS API Key (Consumer Key)
    pub api_key: Option<String>,

    /// Use sandbox environment
    #[serde(default)]
    pub sandbox: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.database.pool_size, 20);
        // Default JWT secret should be empty (forcing explicit configuration)
        assert!(config.security.jwt.secret.is_empty());
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        config.server.port = 0; // 0 is an invalid port for binding
        config.security.jwt.secret =
            "this_is_a_test_secret_that_is_at_least_32_bytes_long".to_string();
        assert!(config.validate().is_err());

        config.server.port = 8080;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_jwt_secret_validation() {
        let mut config = Config::default();

        // Empty secret should fail
        config.security.jwt.secret = String::new();
        assert!(config.validate().is_err());

        // Short secret should fail
        config.security.jwt.secret = "short_secret".to_string();
        assert!(config.validate().is_err());

        // Secret with exactly 31 bytes should fail
        config.security.jwt.secret = "a".repeat(31);
        assert!(config.validate().is_err());

        // Secret with 32 bytes should pass
        config.security.jwt.secret = "a".repeat(32);
        assert!(config.validate().is_ok());

        // Long secret should pass
        config.security.jwt.secret = "this_is_a_very_secure_random_key_for_testing".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tls_config_defaults() {
        let tls_config = TlsConfig::default();
        assert!(!tls_config.enabled); // TLS disabled by default
        assert_eq!(tls_config.min_tls_version, TlsVersion::Tls1_3);
        assert_eq!(tls_config.max_tls_version, TlsVersion::Tls1_3);
        assert!(tls_config.ocsp_stapling);
        assert!(tls_config.hsts.is_none()); // No HSTS by default when disabled
        assert_eq!(tls_config.http_port, 80);
        assert_eq!(tls_config.https_port, 443);
    }

    #[test]
    fn test_tls_config_validation() {
        // When TLS is disabled, validation should pass regardless of cert config
        let tls_config = TlsConfig::default();
        assert!(!tls_config.enabled);
        assert!(tls_config.validate().is_ok());

        // When TLS is enabled, need certificate source
        let mut tls_config = TlsConfig {
            enabled: true,
            ..Default::default()
        };

        // Invalid: no cert source
        assert!(tls_config.validate().is_err());

        // Valid: Let's Encrypt
        tls_config.lets_encrypt = Some(LetsEncryptConfig::default());
        assert!(tls_config.validate().is_ok());

        // Valid: manual certificates
        tls_config = TlsConfig {
            enabled: true,
            cert_file: Some(PathBuf::from("/path/to/cert.pem")),
            key_file: Some(PathBuf::from("/path/to/key.pem")),
            ..Default::default()
        };
        assert!(tls_config.validate().is_ok());
    }

    #[test]
    fn test_lets_encrypt_config_validation() {
        let mut le_config = LetsEncryptConfig {
            email: "admin@example.com".to_string(),
            domains: vec!["example.com".to_string()],
            ..Default::default()
        };

        assert!(le_config.validate().is_ok());

        // Invalid: empty email
        le_config.email = String::new();
        assert!(le_config.validate().is_err());

        // Invalid: no domains
        le_config.email = "admin@example.com".to_string();
        le_config.domains = vec![];
        assert!(le_config.validate().is_err());
    }

    #[test]
    fn test_tls_version_ordering() {
        assert!(TlsVersion::Tls1_3 > TlsVersion::Tls1_2);
        assert_eq!(TlsVersion::Tls1_3, TlsVersion::Tls1_3);
    }

    #[test]
    fn test_hsts_header_generation() {
        let hsts = HstsConfig::default();
        let header = hsts.header_value();

        assert!(header.contains("max-age=31536000"));
        assert!(header.contains("includeSubDomains"));
        assert!(!header.contains("preload"));

        let hsts_preload = HstsConfig {
            preload: true,
            ..Default::default()
        };
        let header_preload = hsts_preload.header_value();
        assert!(header_preload.contains("preload"));
    }
}

// Tax configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxConfig {
    /// Tax provider: 'builtin', 'avalara', 'taxjar'
    #[serde(default = "default_tax_provider")]
    pub provider: String,

    /// Enable OSS (One Stop Shop) reporting
    #[serde(default)]
    pub enable_oss: bool,

    /// EU member state for OSS registration (e.g., 'DE', 'FR')
    #[serde(default)]
    pub oss_member_state: Option<String>,

    /// Default tax behavior
    #[serde(default)]
    pub default_tax_included: bool,

    /// Default tax zone code
    #[serde(default)]
    pub default_tax_zone: Option<String>,

    /// Validate VAT IDs using VIES
    #[serde(default = "default_validate_vat")]
    pub validate_vat_ids: bool,

    /// VAT validation cache duration in days
    #[serde(default = "default_vat_cache_days")]
    pub vat_cache_days: i64,

    /// Avalara configuration
    #[serde(default)]
    pub avalara: Option<AvalaraConfig>,

    /// TaxJar configuration
    #[serde(default)]
    pub taxjar: Option<TaxJarConfig>,
}

impl Default for TaxConfig {
    fn default() -> Self {
        Self {
            provider: default_tax_provider(),
            enable_oss: false,
            oss_member_state: None,
            default_tax_included: false,
            default_tax_zone: None,
            validate_vat_ids: default_validate_vat(),
            vat_cache_days: default_vat_cache_days(),
            avalara: None,
            taxjar: None,
        }
    }
}

fn default_tax_provider() -> String {
    "builtin".to_string()
}

fn default_validate_vat() -> bool {
    true
}

fn default_vat_cache_days() -> i64 {
    30
}

/// Avalara AvaTax configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvalaraConfig {
    pub api_key: Option<String>,
    pub account_id: Option<String>,
    #[serde(default = "default_avalara_env")]
    pub environment: String, // 'sandbox' or 'production'
}

fn default_avalara_env() -> String {
    "sandbox".to_string()
}

/// TaxJar configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxJarConfig {
    pub api_token: Option<String>,
    #[serde(default)]
    pub sandbox: bool,
}
