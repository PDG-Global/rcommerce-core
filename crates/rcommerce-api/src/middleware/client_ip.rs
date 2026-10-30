//! Trusted-proxy-aware client IP resolution.
//!
//! The per-IP limiters and the API-key audit trail need the real client IP,
//! not whatever the client claims. This module resolves it from the TCP peer
//! address and — only when the peer is a configured trusted proxy — walks
//! `X-Forwarded-For` right-to-left past the trusted proxies to the entry the
//! deployment's own proxy chain appended.
//!
//! Rules:
//! - Forwarding headers from untrusted peers are ignored entirely (a client
//!   can send any `X-Forwarded-For` it likes; trusting it would let it pick a
//!   fresh IP per request and bypass every per-IP limit).
//! - Malformed entries, empty entries, or chains longer than
//!   `max_forwarded_hops` from a trusted peer fail closed: the request is
//!   rejected rather than trusted partially.
//! - Results are canonical [`IpAddr`] values used as the limiter keys, so
//!   different spellings of the same address share one bucket.

use std::net::IpAddr;

use axum::extract::Request;
use ipnet::IpNet;

use rcommerce_core::config::RateLimitConfig;

/// Resolved client IP, inserted into request extensions by
/// [`client_ip_middleware`]. Consumers (rate limiters, login throttling,
/// API-key audit) should read this instead of parsing forwarding headers
/// themselves.
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

impl ClientIp {
    pub fn ip(&self) -> IpAddr {
        self.0
    }
}

/// Why a client IP could not be resolved safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientIpError {
    /// X-Forwarded-For contained an entry that is neither empty nor a
    /// parseable IP (or was not valid text at all).
    MalformedHeader,
    /// X-Forwarded-For had more entries than `max_forwarded_hops`.
    ChainTooLong,
}

/// Trusted-proxy-aware client IP resolver.
#[derive(Debug, Clone)]
pub struct ClientIpResolver {
    trusted: Vec<IpNet>,
    max_hops: usize,
}

impl ClientIpResolver {
    /// Build a resolver from validated CIDRs.
    pub fn new(trusted: Vec<IpNet>, max_hops: usize) -> Self {
        Self {
            trusted,
            max_hops: max_hops.max(1),
        }
    }

    /// Build a resolver from the `[rate_limiting]` configuration.
    pub fn from_config(config: &RateLimitConfig) -> std::result::Result<Self, String> {
        Ok(Self::new(
            config.trusted_proxy_nets()?,
            config.max_forwarded_hops,
        ))
    }

    fn is_trusted(&self, ip: IpAddr) -> bool {
        self.trusted.iter().any(|net| net.contains(&ip))
    }

    /// Resolve the client IP for one request.
    ///
    /// `peer` is the TCP peer address (use [`std::net::IpAddr::UNSPECIFIED`]
    /// when connection information is unavailable; the unspecified address
    /// is never a trusted proxy, so forwarding headers are ignored).
    ///
    /// Returns `Err` when a trusted peer sent a header that cannot be
    /// processed safely; callers must fail closed in that case.
    pub fn resolve(
        &self,
        peer: IpAddr,
        forwarded_for: Option<&str>,
    ) -> std::result::Result<IpAddr, ClientIpError> {
        let Some(header) = forwarded_for else {
            return Ok(peer);
        };

        // Only a trusted proxy's view of the chain is worth anything.
        if !self.is_trusted(peer) {
            return Ok(peer);
        }
        if header.trim().is_empty() {
            return Err(ClientIpError::MalformedHeader);
        }

        let entries: Vec<&str> = header.split(',').map(str::trim).collect();
        if entries.iter().any(|e| e.is_empty()) {
            return Err(ClientIpError::MalformedHeader);
        }
        if entries.len() > self.max_hops {
            return Err(ClientIpError::ChainTooLong);
        }
        let ips: Vec<IpAddr> = entries
            .iter()
            .map(|e| e.parse::<IpAddr>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| ClientIpError::MalformedHeader)?;

        // Walk right-to-left: entries appended by trusted proxies are
        // skipped; the first untrusted entry is the client. If every entry
        // is a trusted proxy (e.g. an internal client), the leftmost —
        // original — entry is the client.
        for ip in ips.iter().rev() {
            if !self.is_trusted(*ip) {
                return Ok(*ip);
            }
        }
        Ok(ips[0])
    }

    /// Resolve the client IP directly from a request's extensions and
    /// headers. Missing connection information maps to the unspecified
    /// address (all such requests share one limiter bucket), matching the
    /// historical fail-closed behavior.
    pub fn resolve_request(&self, request: &Request) -> std::result::Result<IpAddr, ClientIpError> {
        use axum::extract::ConnectInfo;
        use std::net::SocketAddr;

        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip())
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        if !self.is_trusted(peer) {
            return Ok(peer);
        }
        let mut values = request.headers().get_all("x-forwarded-for").iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(ClientIpError::MalformedHeader);
        }
        let forwarded = first
            .map(|value| value.to_str().map_err(|_| ClientIpError::MalformedHeader))
            .transpose()?;
        self.resolve(peer, forwarded)
    }
}

/// Resolve the client IP once per request and store it as a [`ClientIp`]
/// extension. Always mounted (inside `api_routes` and on the outer router),
/// so handlers and middleware can rely on the extension being present.
///
/// Malformed or overlong forwarding headers from a trusted proxy fail
/// closed with 403; headers from untrusted peers are silently ignored.
pub async fn client_ip_middleware(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    mut request: Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    match state.client_ip_resolver.resolve_request(&request) {
        Ok(ip) => {
            request.extensions_mut().insert(ClientIp(ip));
            next.run(request).await
        }
        Err(err) => {
            tracing::warn!("Client IP resolution failed: {:?}", err);
            axum::http::StatusCode::FORBIDDEN.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(cidrs: &[&str]) -> ClientIpResolver {
        ClientIpResolver::new(cidrs.iter().map(|c| c.parse().unwrap()).collect(), 8)
    }

    const PROXY: &str = "10.0.0.2";
    const CLIENT: &str = "203.0.113.9";

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn ignores_forwarding_headers_from_untrusted_peers() {
        let r = resolver(&["10.0.0.0/8"]);
        // Client forges an XFF chain; peer is not a trusted proxy.
        assert_eq!(
            r.resolve(ip("198.51.100.7"), Some("1.2.3.4")).unwrap(),
            ip("198.51.100.7")
        );
        assert_eq!(
            r.resolve(ip("198.51.100.7"), Some("6.6.6.6, 203.0.113.9"))
                .unwrap(),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn uses_peer_when_no_header() {
        let r = resolver(&["10.0.0.0/8"]);
        assert_eq!(r.resolve(ip(PROXY), None).unwrap(), ip(PROXY));
        assert_eq!(
            r.resolve(ip(PROXY), Some("  ")),
            Err(ClientIpError::MalformedHeader)
        );
    }

    #[test]
    fn walks_right_to_left_past_trusted_proxies() {
        let r = resolver(&["10.0.0.0/8"]);
        // Client forged "6.6.6.6"; proxies appended 10.0.0.3 and 10.0.0.2.
        assert_eq!(
            r.resolve(ip(PROXY), Some("6.6.6.6, 10.0.0.3, 10.0.0.2"))
                .unwrap(),
            ip("6.6.6.6")
        );
        // Single entry appended by the trusted proxy.
        assert_eq!(r.resolve(ip(PROXY), Some(CLIENT)).unwrap(), ip(CLIENT));
    }

    #[test]
    fn rejects_malformed_entries_and_overlong_chains() {
        let r = resolver(&["10.0.0.0/8"]);
        assert_eq!(
            r.resolve(ip(PROXY), Some("not-an-ip")),
            Err(ClientIpError::MalformedHeader)
        );
        assert_eq!(
            r.resolve(ip(PROXY), Some("1.1.1.1,,2.2.2.2")),
            Err(ClientIpError::MalformedHeader)
        );
        let long = ["1.1.1.1"; 9].join(", ");
        assert_eq!(
            r.resolve(ip(PROXY), Some(&long)),
            Err(ClientIpError::ChainTooLong)
        );
        // Untrusted peer with garbage header: header ignored, not rejected.
        assert_eq!(
            r.resolve(ip("198.51.100.7"), Some("garbage")).unwrap(),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn all_trusted_chain_falls_back_to_leftmost() {
        let r = resolver(&["10.0.0.0/8"]);
        // Internal client speaking through two proxies: every entry is
        // trusted; the leftmost (original) entry is the client.
        assert_eq!(
            r.resolve(ip(PROXY), Some("10.1.2.3, 10.0.0.3, 10.0.0.2"))
                .unwrap(),
            ip("10.1.2.3")
        );
    }

    #[test]
    fn ipv6_proxies_match_cidr() {
        let r = resolver(&["2001:db8::/32"]);
        assert_eq!(
            r.resolve(ip("2001:db8::1"), Some("2001:db8::2, ::1"))
                .unwrap(),
            ip("::1")
        );
    }

    #[test]
    fn no_trusted_proxies_means_never_trust_headers() {
        let r = resolver(&[]);
        assert_eq!(r.resolve(ip(PROXY), Some(CLIENT)).unwrap(), ip(PROXY));
    }

    #[test]
    fn rejects_invalid_or_repeated_request_headers() {
        use axum::{body::Body, extract::ConnectInfo, http::HeaderValue};
        use std::net::SocketAddr;

        let r = resolver(&["10.0.0.0/8"]);
        let peer: SocketAddr = "10.0.0.2:4321".parse().unwrap();
        let mut invalid = Request::builder().body(Body::empty()).unwrap();
        invalid.extensions_mut().insert(ConnectInfo(peer));
        invalid
            .headers_mut()
            .insert("x-forwarded-for", HeaderValue::from_bytes(&[0xff]).unwrap());
        assert_eq!(
            r.resolve_request(&invalid),
            Err(ClientIpError::MalformedHeader)
        );

        let mut repeated = Request::builder().body(Body::empty()).unwrap();
        repeated.extensions_mut().insert(ConnectInfo(peer));
        repeated
            .headers_mut()
            .append("x-forwarded-for", HeaderValue::from_static("203.0.113.9"));
        repeated
            .headers_mut()
            .append("x-forwarded-for", HeaderValue::from_static("198.51.100.7"));
        assert_eq!(
            r.resolve_request(&repeated),
            Err(ClientIpError::MalformedHeader)
        );
    }
}
