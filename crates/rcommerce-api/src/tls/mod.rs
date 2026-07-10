pub mod config;

#[cfg(feature = "letsencrypt")]
pub mod letsencrypt;

use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

// Re-export only the types that don't conflict with rcommerce_core
pub use config::{HstsConfig, TlsVersion};

#[cfg(feature = "letsencrypt")]
pub use letsencrypt::{CertificateInfo, LetsEncryptManager};

// Use core TlsConfig for consistency
pub use rcommerce_core::config::TlsConfig;

/// Security headers middleware
///
/// This middleware adds security headers including HSTS when TLS is enabled.
/// The TlsConfig should be passed via request extensions.
pub async fn security_headers_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let tls_config = request.extensions().get::<TlsConfig>().cloned();
    let mut response = next.run(request).await;

    // Add security headers
    if let Some(tls_config) = tls_config {
        add_security_headers(&mut response, &tls_config);
    }

    Ok(response)
}

fn add_security_headers(response: &mut Response, tls_config: &TlsConfig) {
    let headers = response.headers_mut();

    // HSTS Header
    if let Some(hsts_config) = &tls_config.hsts {
        if hsts_config.enabled {
            headers.insert(
                "strict-transport-security",
                hsts_config.header_value().parse().unwrap(),
            );
        }
    }

    // Security headers
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert(
        "referrer-policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    headers.insert("x-xss-protection", "1; mode=block".parse().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_version_ordering() {
        assert!(TlsVersion::Tls1_3 > TlsVersion::Tls1_2);
        assert_eq!(TlsVersion::Tls1_3, TlsVersion::Tls1_3);
    }
}
