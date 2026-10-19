//! WooCommerce-compatible Basic Auth middleware.
//!
//! WooCommerce uses HTTP Basic Auth with consumer_key as username and
//! consumer_secret as password. The key format is ck_xxx / cs_xxx.
//!
//! This middleware extracts those credentials, strips the ck_/cs_ prefixes,
//! and validates using the existing rCommerce API key system.

use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::Engine;
use std::sync::Arc;

use rcommerce_core::repository::{ApiKeyRepository, PostgresApiKeyRepository};

use super::api_key_auth::ApiKeyAuth;

/// WooCommerce error response format
#[derive(serde::Serialize)]
struct WcError {
    code: String,
    message: String,
    data: WcErrorData,
}

#[derive(serde::Serialize)]
struct WcErrorData {
    status: u16,
}

fn wc_error_response(status: u16, code: &str, message: &str) -> Response {
    let body = serde_json::to_string(&WcError {
        code: code.to_string(),
        message: message.to_string(),
        data: WcErrorData { status },
    })
    .unwrap_or_default();

    axum::http::Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=UTF-8")
        .body(Body::from(body))
        .unwrap()
}

/// WooCommerce Basic Auth middleware.
///
/// Tries (in order):
/// 1. `Authorization: Basic <base64(ck_xxx:cs_xxx)>`
/// 2. `?consumer_key=ck_xxx&consumer_secret=cs_xxx` query params
///
/// On success, injects `ApiKeyAuth` into request extensions.
pub async fn wc_auth_middleware(
    State(repo): State<Arc<PostgresApiKeyRepository>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, impl IntoResponse> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();

    tracing::info!(
        "[WC] {} {} | User-Agent: {} | Authorization: {}",
        method,
        uri,
        headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(none)"),
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(none)"),
    );

    let credentials =
        extract_basic_auth(request.headers()).or_else(|| extract_query_credentials(request.uri()));

    let (ck, cs) = match credentials {
        Some(creds) => creds,
        None => {
            tracing::warn!("[WC] No credentials found in request to {}", uri);
            return Err(wc_error_response(
                401,
                "woocommerce_rest_cannot_view",
                "Sorry, you cannot view this resource.",
            ));
        }
    };

    tracing::info!(
        "[WC] Extracted consumer_key: {}...{}",
        &ck[..ck.len().min(8)],
        if ck.len() > 8 {
            &ck[ck.len() - 3..]
        } else {
            ""
        }
    );

    // Strip ck_ and cs_ prefixes to get the rCommerce key format
    let prefix = ck.strip_prefix("ck_").unwrap_or(&ck);
    let secret = cs.strip_prefix("cs_").unwrap_or(&cs);
    let full_key = format!("{}.{}", prefix, secret);

    tracing::info!(
        "[WC] Verifying key prefix: {}...",
        &prefix[..prefix.len().min(8)]
    );

    let key_record = match repo.verify_key(&full_key).await {
        Ok(Some(record)) => {
            tracing::info!("[WC] Key '{}' authenticated OK", record.name);
            record
        }
        Ok(None) => {
            tracing::warn!(
                "[WC] Key verification failed — key not found or revoked (prefix: {})",
                prefix
            );
            return Err(wc_error_response(
                401,
                "woocommerce_rest_cannot_view",
                "Consumer key is invalid.",
            ));
        }
        Err(e) => {
            tracing::error!("[WC] Database error during key verification: {}", e);
            return Err(wc_error_response(
                500,
                "internal_server_error",
                "Internal server error.",
            ));
        }
    };

    // Update last used (fire and forget)
    let repo_clone = repo.clone();
    let key_id = key_record.id;
    tokio::spawn(async move {
        let _ = repo_clone.update_last_used(key_id, None).await;
    });

    let auth = ApiKeyAuth {
        key_id: key_record.id,
        customer_id: key_record.customer_id,
        scopes: key_record.scopes,
        name: key_record.name,
    };

    request.extensions_mut().insert(auth);

    let response = next.run(request).await;
    tracing::info!(
        "[WC] Response: {} ({}ms latency)",
        response.status(),
        0 // We don't have timing here, but status is the key info
    );
    Ok(response)
}

/// Extract ck_xxx:cs_xxx from Basic Auth header
fn extract_basic_auth(headers: &axum::http::HeaderMap) -> Option<(String, String)> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;

    tracing::debug!(
        "[WC] Raw Authorization header: {}",
        &header[..header.len().min(40)]
    );

    let encoded = header.strip_prefix("Basic ")?.trim();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let credentials = String::from_utf8(decoded).ok()?;

    tracing::debug!(
        "[WC] Decoded Basic auth: {}...",
        &credentials[..credentials.len().min(20)]
    );

    let (key, secret) = credentials.split_once(':')?;

    Some((key.to_string(), secret.to_string()))
}

/// Extract consumer_key/consumer_secret from query string
fn extract_query_credentials(uri: &axum::http::Uri) -> Option<(String, String)> {
    let query = uri.query()?;
    let params: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

    let key = params.get("consumer_key")?;
    let secret = params.get("consumer_secret")?;

    if key.is_empty() || secret.is_empty() {
        return None;
    }

    Some((key.to_string(), secret.to_string()))
}
