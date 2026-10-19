//! WooCommerce pagination helpers.
//!
//! WC uses X-WP-Total, X-WP-TotalPages headers and Link headers.

use axum::http::{HeaderMap, HeaderValue};

/// Add WooCommerce pagination headers to a response.
///
/// - `X-WP-Total`: total number of items
/// - `X-WP-TotalPages`: total number of pages
/// - `Link`: rel="next", rel="prev" links
pub fn pagination_headers(total: i64, page: i64, per_page: i64, base_url: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    let total_pages = if per_page > 0 {
        (total as f64 / per_page as f64).ceil() as i64
    } else {
        1
    };

    headers.insert(
        "X-WP-Total",
        HeaderValue::from_str(&total.to_string()).unwrap_or(HeaderValue::from_static("0")),
    );
    headers.insert(
        "X-WP-TotalPages",
        HeaderValue::from_str(&total_pages.to_string()).unwrap_or(HeaderValue::from_static("0")),
    );

    // Build Link header
    let mut links = Vec::new();
    if page > 1 {
        links.push(format!("<{}?page={}>; rel=\"prev\"", base_url, page - 1));
    }
    if page < total_pages {
        links.push(format!("<{}?page={}>; rel=\"next\"", base_url, page + 1));
    }
    links.push(format!("<{}?page=1>; rel=\"first\"", base_url));
    links.push(format!("<{}?page={}>; rel=\"last\"", base_url, total_pages));

    if !links.is_empty() {
        if let Ok(val) = HeaderValue::from_str(&links.join(", ")) {
            headers.insert("Link", val);
        }
    }

    headers
}

/// Parse page and per_page from query params (WC defaults: page=1, per_page=10)
pub fn parse_pagination(page: Option<i64>, per_page: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let per_page = per_page.unwrap_or(10).clamp(1, 100);
    (page, per_page)
}

/// Calculate offset from page and per_page
pub fn offset(page: i64, per_page: i64) -> i64 {
    (page - 1) * per_page
}
