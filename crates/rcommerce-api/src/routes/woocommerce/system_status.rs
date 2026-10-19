//! WooCommerce-compatible system status endpoint.
//!
//! GET /wp-json/wc/v3/system_status

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};

use crate::middleware::api_key_auth::ApiKeyAuth;
use crate::state::AppState;

/// GET /wp-json/wc/v3/system_status
pub async fn system_status(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
) -> Response {
    let site_url = state.branding.site_url.clone();
    let status = serde_json::json!({
        "environment": {
            "home_url": site_url,
            "site_url": site_url,
            "version": env!("CARGO_PKG_VERSION"),
            "wp_version": "rcommerce-compat",
            "wp_multisite": false,
            "wp_memory_limit": -1,
            "wp_debug_mode": false,
            "wp_cron": true,
            "language": "en_US",
            "external_object_cache": false,
            "php_version": "N/A",
            "server_info": "rCommerce Rust API",
            "max_upload_size": 52428800,
            "mysql_version": "PostgreSQL",
            "permalink_structure": "/%postname%/",
            "woocommerce_version": "9.9.0",
            "log_directory": "/var/log/rcommerce",
            "log_file": "rcommerce.log",
        },
        "settings": {
            "api_enabled": true,
            "force_ssl": true,
            "currency": "USD",
            "currency_position": "left",
            "thousand_separator": ",",
            "decimal_separator": ".",
            "number_of_decimals": 2,
            "tax_enabled": true,
            "weight_unit": "lbs",
            "dimension_unit": "in",
            "coupons_enabled": true,
            "stock_management": true,
            "woocommerce_cart_page_id": 0,
            "woocommerce_checkout_page_id": 0,
            "woocommerce_myaccount_page_id": 0,
            "woocommerce_shop_page_id": 0,
        },
        "pages": [],
        "post_type_counts": [],
        "active_plugins": [
            "rcommerce-woocommerce-compat/1.0.0",
        ],
        "theme": {
            "name": "rCommerce Storefront",
            "version": "1.0.0",
            "author_url": "https://rcommerce.app",
        },
    });

    Json(status).into_response()
}
