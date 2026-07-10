//! Middleware components for request processing
//!
//! This module contains Axum middleware for cross-cutting concerns
//! like rate limiting, authentication, logging, and security.

pub mod rate_limit;

// Re-export commonly used middleware types
pub use rate_limit::{
    check_for_api_key, rate_limit_middleware, RateLimitConfig, RateLimitError, RateLimitStats,
    RateLimiter,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_middleware_modules_exist() {
        // This test ensures middleware modules compile
        let _config = RateLimitConfig::default();
        assert!(_config.enabled);
    }
}
