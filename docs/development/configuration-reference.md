# Configuration Reference

This document provides a complete reference for all configuration options in R commerce.

## Configuration File Format

R commerce uses TOML format for configuration files, with support for environment variable overrides.

## Top-Level Configuration Structure

```toml
[server]
[database]
[payments]
[shipping]
[notifications]
[logging]
[cache]
[security]
[compatibility]
[migration]
[features]
```

## Server Configuration

```toml
[server]
# HTTP server settings
host = "0.0.0.0"           # Bind address (0.0.0.0 for all interfaces)
port = 8080                # HTTP port
graceful_shutdown_timeout = "30s"  # How long to wait for connections to close

# Request settings
max_request_size = "10MB"  # Maximum request body size
keep_alive = true          # Enable HTTP keep-alive
keep_alive_timeout = "75s" # Keep-alive timeout

# Worker threads (0 = automatic based on CPU count)
worker_threads = 0

# Rate limiting (per IP)
rate_limit_per_minute = 1000
rate_limit_burst = 200

# CORS settings
[cors]
enabled = true
allowed_origins = ["*"]    # or specific origins: ["https://store.com"]
allowed_methods = ["GET", "POST", "PUT", "PATCH", "DELETE"]
allowed_headers = ["Content-Type", "Authorization"]
expose_headers = ["X-Request-ID", "X-Rate-Limit-Remaining"]
allow_credentials = true
max_age = "1h"
```

**Environment Variables:**
```bash
RCOMMERCE_SERVER_HOST=0.0.0.0
RCOMMERCE_SERVER_PORT=3000
RCOMMERCE_SERVER_WORKER_THREADS=4
RCOMMERCE_SERVER_RATE_LIMIT_PER_MINUTE=5000
```

## Database Configuration

### PostgreSQL Configuration

```toml
[database]
type = "postgres"          # Database type: "postgres"

# Connection settings
host = "localhost"
port = 5432
username = "rcommerce"
password = "secure_password"
database = "rcommerce_prod"

# Connection pooling
pool_size = 20             # Maximum connections in pool
max_lifetime = "30min"     # Maximum lifetime of a connection
idle_timeout = "10min"     # Idle timeout before closing connection
connection_timeout = "30s" # Timeout for new connection

# SSL/TLS settings (PostgreSQL)
ssl_mode = "prefer"        # Options: "disable", "prefer", "require"
ssl_cert = "/path/to/client-cert.pem"    # Optional
ssl_key = "/path/to/client-key.pem"      # Optional
ssl_root_cert = "/path/to/ca-cert.pem"   # Optional

# Connection URI (alternative to individual fields)
# url = "postgres://user:pass@host:5432/database?sslmode=require"

# Schema (PostgreSQL)
schema = "public"

# Application name (PostgreSQL)
application_name = "rcommerce"
```


page_size = "4096"         # Page size in bytes

temp_store = "MEMORY"      # Temp store: "MEMORY", "FILE"
```

**Environment Variables:**
```bash
RCOMMERCE_DATABASE_TYPE=postgres
RCOMMERCE_DATABASE_HOST=prod-db.example.com
RCOMMERCE_DATABASE_USERNAME=rcommerce
RCOMMERCE_DATABASE_PASSWORD=secure_password
RCOMMERCE_DATABASE_POOL_SIZE=50
```

## Payment Configuration

```toml
[payments]
# Default payment gateway
default_gateway = "stripe"

# Auto-capture settings
auto_capture = true              # Automatically capture payments
capture_delay_seconds = 0        # Delay before capture (0 = immediate)

# 3D Secure / Strong Customer Authentication
sca_threshold = 3000             # Amount (in minor units) requiring 3DS

# Supported currencies
supported_currencies = ["USD", "EUR", "GBP", "JPY", "CAD", "AUD"]

# Fraud detection
enable_fraud_check = true        # Enable fraud detection
risk_threshold = 75              # Score > 75 blocks payment

# Stripe Configuration
[payments.stripe]
enabled = true
secret_key = "sk_live_your_secret_key_here"
publishable_key = "pk_live_your_publishable_key_here"
webhook_secret = "whsec_your_webhook_secret_here"

# Stripe Connect (for marketplace features)
connect_enabled = false
connect_client_id = "ca_your_client_id_here"

# Stripe settings
statement_descriptor = "RCOMMERCE"
statement_descriptor_suffix = "STORE"
receipt_email = true             # Send automatic receipt emails

# Airwallex Configuration
[payments.airwallex]
enabled = false                  # Enable/disable Airwallex
client_id = "your_client_id"
api_key = "your_api_key"
webhook_secret = "your_webhook_secret"
account_id = "your_account_id"
request_currency = true          # Request payment in specific currency

# PayPal Configuration
[payments.paypal]
enabled = false
client_id = "your_client_id"
client_secret = "your_client_secret"
environment = "live"             # "live" or "sandbox"

# Braintree Configuration
[payments.braintree]
enabled = false
merchant_id = "your_merchant_id"
public_key = "your_public_key"
private_key = "your_private_key"
environment = "production"       # "production" or "sandbox"

# Manual/Bank Transfer Configuration
[payments.manual]
enabled = true                   # Always enable manual payments
instructions = "Please transfer to: Bank: ... Account: ..."
confirm_delay_hours = 24         # How long to wait for confirmation
```

**Environment Variables:**
```bash
RCOMMERCE_PAYMENTS_DEFAULT_GATEWAY=stripe
RCOMMERCE_PAYMENTS_AUTO_CAPTURE=true
RCOMMERCE_PAYMENTS_STRIPE_SECRET_KEY=sk_live_xxx
RCOMMERCE_PAYMENTS_RISK_THRESHOLD=80
```

## Shipping Configuration

```toml
[shipping]
# Default shipping provider
default_provider = "shipstation"

# Calculation settings
default_weight_unit = "lb"      # "lb", "oz", "kg", "g", "lb"
default_dimension_unit = "in"   # "in", "cm", "mm"

# Shipping zones
[[shipping.zones]]
name = "Domestic US"
countries = ["US"]
default_service = "ground"

[[shipping.zones]]
name = "International"
countries = ["CA", "MX", "GB", "DE", "FR", "AU", "JP"]
default_service = "standard"

# Free shipping rule
[shipping.free_shipping]
enabled = true
min_order_value = 75.00           # Free shipping on orders $75+
countries = ["US", "CA"]

# ShipStation Configuration
[shipping.shipstation]
enabled = true
api_key = "your_api_key"
api_secret = "your_api_secret"
webhook_secret = "your_webhook_secret"

# EasyPost Configuration
[shipping.easypost]
enabled = false
api_key = "your_api_key"

# Dianxiaomi Configuration (Chinese market)
[shipping.dianxiaomi]
enabled = false
app_key = "your_app_key"
app_secret = "your_app_secret"
base_url = "https://erp.dianxiaomi.com"
```

## Notification Configuration

```toml
[notifications]
# Email sender
from_name = "Your Store"
from_email = "orders@yourstore.com"

# Queue settings
queue_size = 1000              # Maximum queued notifications
worker_count = 2               # Number of worker threads
retry_attempts = 3             # Retry failed notifications
retry_delay_seconds = 300      # 5 minutes between retries

[notifications.email]
provider = "smtp"              # or "sendgrid", "ses", "mailgun"
template_dir = "/etc/rcommerce/templates/email"

# SMTP Configuration
[notifications.email.smtp]
host = "smtp.sendgrid.net"
port = 587
username = "apikey"
password = "SG.your_api_key"
from_address = "orders@yourstore.com"
use_tls = true

# SendGrid API Configuration (alternative to SMTP)
[notifications.email.sendgrid]
api_key = "SG.your_api_key"
from_address = "orders@yourstore.com"

# SMS Configuration
[notifications.sms]
enabled = false
provider = "twilio"            # or "aws-sms"

[notifications.sms.twilio]
account_sid = "your_account_sid"
auth_token = "your_auth_token"
from_number = "+1234567890"

# Webhook Configuration
[notifications.webhooks]
enabled = true
timeout_seconds = 30
max_concurrent = 50
retry_policy = "exponential"   # "linear" or "exponential"
```

## Logging Configuration

```toml
[logging]
# Log level: "error", "warn", "info", "debug", "trace"
level = "info"

# Log format: "json" or "text"
format = "json"

# Output destinations
[logging.outputs]
# Console output
console = { enabled = true, format = "text" }

# File output
file = {
    enabled = true,
    path = "/var/log/rcommerce/rcommerce.log",
    rotation = "daily",      # "daily", "hourly", "size"
    max_size = "100MB",      # For size-based rotation
    max_files = 10,          # Keep last N rotated files
    compress = true
}

# Syslog output (Unix systems only)
syslog = {
    enabled = false,
    facility = "local0",
    tag = "rcommerce"
}

# Performance logging
[logging.performance]
enable_slow_query_log = true
slow_query_threshold_ms = 1000
enable_http_timing = true

# Security logging
[logging.security]
log_auth_attempts = true
log_failed_auth = true
log_failed_payment = true
mask_sensitive_data = true
```

**Environment Variables:**
```bash
RCOMMERCE_LOGGING_LEVEL=debug
RCOMMERCE_LOGGING_FORMAT=json
RCOMMERCE_RUST_LOG=rcommerce=debug,sqlx=warn
```

## Cache Configuration

```toml
[cache]
# Cache provider: "redis", "memory", "none"
provider = "redis"

# In-memory cache (when provider = "memory")
[cache.memory]
max_size_mb = 100           # Maximum cache size
ttl_seconds = 300           # Default TTL

timeout_ms = 1000
user = "default"
password = "your_redis_password"
database = 0

# Connection pooling
pool_size = 10
max_lifetime = "30min"
idle_timeout = "10min"
connection_timeout = "5s"

# Key prefixes
key_prefix = "rcommerce:"

# Cache namespaces
[cache.namespaces]
products = { ttl_seconds = 3600 }       # 1 hour
orders = { ttl_seconds = 1800 }         # 30 minutes
customers = { ttl_seconds = 3600 }
sessions = { ttl_seconds = 7200 }       # 2 hours
rate_limits = { ttl_seconds = 60 }      # 1 minute

# Redis Cluster (optional)
[cache.cluster]
enabled = false
nodes = ["redis://node1:6379", "redis://node2:6379", "redis://node3:6379"]
```

## Security Configuration

```toml
[security]
# API Key settings
api_key_prefix_length = 8    # Length of prefix in API keys (for identification)
api_key_secret_length = 32   # Length of secret portion

# Rate limiting per key type
[security.rate_limits]
publishable = { requests_per_minute = 100, burst = 20 }
secret = { requests_per_minute = 1000, burst = 200 }
restricted = { requests_per_minute = 500, burst = 100 }
admin = { requests_per_minute = 5000, burst = 1000 }

# Session settings
[security.sessions]
enabled = true
ttl_hours = 24               # Session TTL
renewal_threshold = 0.8      # Renew when 80% expired
store = "cache"              # "cache" or "database"

# JWT settings (for admin sessions)
[security.jwt]
secret = "your_jwt_secret_key_here_32_chars_min"
expiry_hours = 24
issuer = "rcommerce"
audience = "rcommerce-api"

# Password settings
[security.passwords]
min_length = 8
require_uppercase = true
require_lowercase = true
require_numbers = true
require_special_chars = true
max_age_days = 90            # Password expiry
prevent_password_reuse = 5   # Can't reuse last N passwords

# 2FA settings
[security.two_factor]
enabled = false
required_for_admin = false
issuer = "RCommerce"
ttl_seconds = 300           # OTP expiry

# CORS settings (duplicate of server.cors for clarity)
[security.cors]
enabled = true
allowed_origins = ["*"]
max_age = "1h"

# Webhook security
[security.webhooks]
require_https = true         # Webhook URLs must use HTTPS
verify_signatures = true     # Verify webhook signatures
hmac_algorithm = "sha256"    # HMAC algorithm

# Allowed IP ranges (optional)
[security.ip_whitelist]
enabled = false
allowed_ranges = ["10.0.0.0/8", "172.16.0.0/12"]
```

**Environment Variables:**
```bash
RCOMMERCE_SECURITY_JWT_SECRET=your_secret_here
RCOMMERCE_SECURITY_API_KEY_SECRET_LENGTH=40
RCOMMERCE_SECURITY_WEBHOOK_VERIFY_SIGNATURES=true
```

## Compatibility Configuration

```toml
[compatibility]
# Enable compatibility layer
enabled = true

# Default platform when can't be detected
default_platform = "woocommerce"

# WooCommerce compatibility
[compatibility.woocommerce]
enabled = true
# Legacy WooCommerce API support (older versions)
legacy_api = false
# Translate WordPress user IDs to R commerce customer IDs
translate_user_ids = true
# Supported endpoints
# endpoints = ["/wc-api/v3/products", "/wc-api/v3/orders", ...]

# Medusa.js compatibility
[compatibility.medusa]
enabled = true
# API endpoints
store_endpoint = "/store"
admin_endpoint = "/admin"
# Custom field mappings
# field_mappings = { medusa_field: "rcommerce_field" }

# Shopify compatibility (future)
[compatibility.shopify]
enabled = false
api_version = "2024-01"
```

## Feature Flags

```toml
[features]
# Enable/disable features at runtime

# Core features
products = true
orders = true
customers = true
payments = true
shipping = true

discounts = true
taxes = true
inventory = true

# Advanced features
returns = true
subscriptions = false           # Future feature
multi_vendor = false            # Marketplace features (future)
gift_cards = true

# API features
graphql_api = true
webhooks = true
realtime_updates = true        # WebSocket/subscription support

# Security features
two_factor_auth = false
ip_whitelist = false
api_key_scopes = true

# Performance features
caching = true
background_jobs = true
read_replicas = false
```

## Environment Overrides

All configuration options can be overridden via environment variables using the pattern:

```bash
RCOMMERCE_<SECTION>_<SUBSECTION>_<KEY>=value
```

Examples:
```bash
# Server
RCOMMERCE_SERVER_HOST=0.0.0.0
RCOMMERCE_SERVER_PORT=3000
RCOMMERCE_SERVER_WORKER_THREADS=8

# Database
RCOMMERCE_DATABASE_TYPE=postgres
RCOMMERCE_DATABASE_HOST=db.example.com
RCOMMERCE_DATABASE_POOL_SIZE=50

# Payments
RCOMMERCE_PAYMENTS_DEFAULT_GATEWAY=stripe
RCOMMERCE_PAYMENTS_STRIPE_SECRET_KEY=sk_live_xxx

# Redis/Cache
RCOMMERCE_CACHE_REDIS_URL=redis://redis:6379
RCOMMERCE_CACHE_REDIS_POOL_SIZE=20

# Logging
RCOMMERCE_LOGGING_LEVEL=info
RCOMMERCE_LOGGING_FORMAT=json

# Security
RCOMMERCE_SECURITY_JWT_SECRET=your_secret_here
RCOMMERCE_SECURITY_API_KEY_SECRET_LENGTH=40

# Features
RCOMMERCE_FEATURES_SUBSCRIPTIONS=true
RCOMMERCE_FEATURES_MULTI_VENDOR=true
```

**Special Notes:**
- Environment variables override file configuration
- Arrays can be specified as comma-separated: `RCOMMERCE_SHIPPING_SUPPORTED_CURRENCIES=USD,EUR,GBP`
- Booleans: `true`, `false`, `1`, `0`
- Durations: `"30s"`, `"5m"`, `"1h"`, `"24h"`

## Complete Example Configuration

```toml
# Production configuration example
[server]
host = "0.0.0.0"
port = 8080
graceful_shutdown_timeout = "30s"
max_request_size = "10MB"
worker_threads = 0
rate_limit_per_minute = 1000

[database]
type = "postgres"
host = "prod-db.internal.example.com"
port = 5432
username = "rcommerce_prod"
password = "${DB_PASSWORD}"  # Use environment variable
ssl_mode = "require"
pool_size = 50
max_lifetime = "30min"

[payments]
default_gateway = "stripe"
auto_capture = true
sca_threshold = 3000
enable_fraud_check = true
risk_threshold = 75

[payments.stripe]
secret_key = "${STRIPE_SECRET_KEY}"
webhook_secret = "${STRIPE_WEBHOOK_SECRET}"

[shipping]
default_provider = "shipstation"

[shipping.shipstation]
api_key = "${SHIPSTATION_API_KEY}"
api_secret = "${SHIPSTATION_API_SECRET}"

[cache]
provider = "redis"
url = "redis://cache.internal.example.com:6379"
password = "${REDIS_PASSWORD}"
pool_size = 20

[notifications.email]
provider = "sendgrid"

[notifications.email.sendgrid]
api_key = "${SENDGRID_API_KEY}"

[logging]
level = "info"
format = "json"

[logging.file]
path = "/var/log/rcommerce/rcommerce.log"
rotation = "daily"
max_size = "100MB"
max_files = 10

[security]
api_key_prefix_length = 8
api_key_secret_length = 32

[security.jwt]
secret = "${JWT_SECRET}"

[cors]
enabled = true
allowed_origins = ["https://store.example.com", "https://admin.example.com"]
allow_credentials = true

[features]
products = true
orders = true
customers = true
payments = true
shipping = true
returns = true
discounts = true
taxes = true
inventory = true
caching = true
background_jobs = true
graphql_api = true
webhooks = true
```

---

This configuration reference covers all major configuration options for R commerce. For additional options, see the specific integration documentation (payment, shipping, etc.).
