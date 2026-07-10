use serde::{Deserialize, Serialize};
use std::fmt;

/// Main error type for rcommerce
#[derive(Debug)]
pub enum Error {
    /// Configuration errors
    Config(String),

    /// Database errors (SQLx wrapped)
    Database(sqlx::Error),

    /// HTTP/Network errors
    Network(String),

    /// Authentication/Authorization errors
    Unauthorized(String),

    /// Forbidden error (e.g. imported account requires password reset)
    Forbidden(String),

    /// Validation errors
    Validation(String),

    /// Not found errors
    NotFound(String),

    /// Payment processing errors
    Payment(String),

    /// Shipping errors
    Shipping(String),

    /// Storage/media errors
    Storage(String),

    /// Cache errors
    Cache(String),

    /// Notification errors
    Notification(String),

    /// Rate limiting errors
    RateLimit(crate::middleware::rate_limit::RateLimitError),

    /// HTTP errors (status code + message)
    HttpError(http::StatusCode, String),

    /// Serialization/serialization errors
    Serialization(serde_json::Error),

    /// IO errors
    Io(std::io::Error),

    /// Generic errors with description
    Other(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(msg) => write!(f, "Configuration error: {}", msg),
            Error::Database(e) => write!(f, "Database error: {}", e),
            Error::Network(msg) => write!(f, "Network error: {}", msg),
            Error::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            Error::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            Error::Validation(msg) => write!(f, "Validation error: {}", msg),
            Error::NotFound(msg) => write!(f, "Not found: {}", msg),
            Error::Payment(msg) => write!(f, "Payment error: {}", msg),
            Error::Shipping(msg) => write!(f, "Shipping error: {}", msg),
            Error::Storage(msg) => write!(f, "Storage error: {}", msg),
            Error::Cache(msg) => write!(f, "Cache error: {}", msg),
            Error::Notification(msg) => write!(f, "Notification error: {}", msg),
            Error::Serialization(e) => write!(f, "Serialization error: {}", e),
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::RateLimit(e) => write!(f, "Rate limit error: {}", e),
            Error::HttpError(code, msg) => write!(f, "HTTP {} error: {}", code, msg),
            Error::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Database(e) => Some(e),
            Error::Serialization(e) => Some(e),
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

// Conversions from external error types
impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Error::Database(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Serialization(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<toml::de::Error> for Error {
    fn from(error: toml::de::Error) -> Self {
        Error::Config(error.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Error::Network(error.to_string())
    }
}

impl From<uuid::Error> for Error {
    fn from(error: uuid::Error) -> Self {
        Error::Validation(format!("Invalid UUID: {}", error))
    }
}

// Common error constructors
impl Error {
    /// Create a new configuration error
    pub fn config<T: Into<String>>(msg: T) -> Self {
        Error::Config(msg.into())
    }

    /// Create a new forbidden error
    pub fn forbidden<T: Into<String>>(msg: T) -> Self {
        Error::Forbidden(msg.into())
    }

    /// Create a new validation error
    pub fn validation<T: Into<String>>(msg: T) -> Self {
        Error::Validation(msg.into())
    }

    /// Create a new not found error
    pub fn not_found<T: Into<String>>(msg: T) -> Self {
        Error::NotFound(msg.into())
    }

    /// Create a new unauthorized error
    pub fn unauthorized<T: Into<String>>(msg: T) -> Self {
        Error::Unauthorized(msg.into())
    }

    /// Create a new payment error
    pub fn payment<T: Into<String>>(msg: T) -> Self {
        Error::Payment(msg.into())
    }

    /// Create a new shipping error
    pub fn shipping<T: Into<String>>(msg: T) -> Self {
        Error::Shipping(msg.into())
    }

    /// Create a new storage error
    pub fn storage<T: Into<String>>(msg: T) -> Self {
        Error::Storage(msg.into())
    }

    /// Create a new cache error
    pub fn cache<T: Into<String>>(msg: T) -> Self {
        Error::Cache(msg.into())
    }

    /// Create a new notification error
    pub fn notification<T: Into<String>>(msg: T) -> Self {
        Error::Notification(msg.into())
    }

    /// Create a new network error
    pub fn network<T: Into<String>>(msg: T) -> Self {
        Error::Network(msg.into())
    }

    /// Create a new internal error
    pub fn internal<T: Into<String>>(msg: T) -> Self {
        Error::Other(msg.into())
    }

    /// Create a notification error (convenience method matching naming convention)
    pub fn notification_error<T: Into<String>>(msg: T) -> Self {
        Error::Notification(msg.into())
    }

    /// Create a payment error (convenience method matching naming convention)
    pub fn payment_error<T: Into<String>>(msg: T) -> Self {
        Error::Payment(msg.into())
    }
}

impl Error {
    /// Get HTTP status code for error
    pub fn status_code(&self) -> u16 {
        match self {
            Error::Unauthorized(_) => 401,
            Error::Forbidden(_) => 403,
            Error::Validation(_) => 400,
            Error::NotFound(_) => 404,
            Error::Config(_) => 500,
            Error::Database(_) => 500,
            Error::Payment(_) => 402,
            Error::Shipping(_) => 500,
            Error::Storage(_) => 500,
            Error::Cache(_) => 500,
            Error::Notification(_) => 500,
            Error::Serialization(_) => 500,
            Error::Io(_) => 500,
            Error::Network(_) => 503,
            Error::RateLimit(_) => 429,
            Error::HttpError(code, _) => code.as_u16(),
            Error::Other(_) => 500,
        }
    }

    /// Get error category for monitoring
    pub fn category(&self) -> &'static str {
        match self {
            Error::Config(_) => "config",
            Error::Database(_) => "database",
            Error::Unauthorized(_) => "auth",
            Error::Forbidden(_) => "auth",
            Error::Validation(_) => "validation",
            Error::NotFound(_) => "not_found",
            Error::Payment(_) => "payment",
            Error::Shipping(_) => "shipping",
            Error::Storage(_) => "storage",
            Error::Cache(_) => "cache",
            Error::Notification(_) => "notification",
            Error::Serialization(_) => "serialization",
            Error::Io(_) => "io",
            Error::Network(_) => "network",
            Error::RateLimit(_) => "rate_limit",
            Error::HttpError(_, _) => "http",
            Error::Other(_) => "other",
        }
    }
}

/// Validation error struct for detailed field errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationErrors {
    pub errors: Vec<FieldError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
    pub code: Option<String>,
}

impl ValidationErrors {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn add(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.errors.push(FieldError {
            field: field.into(),
            message: message.into(),
            code: None,
        });
    }

    pub fn add_with_code(
        &mut self,
        field: impl Into<String>,
        message: impl Into<String>,
        code: impl Into<String>,
    ) {
        self.errors.push(FieldError {
            field: field.into(),
            message: message.into(),
            code: Some(code.into()),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn into_error(self) -> Error {
        Error::Validation(
            serde_json::to_string(&self).unwrap_or_else(|_| "Validation failed".to_string()),
        )
    }
}

impl Default for ValidationErrors {
    fn default() -> Self {
        Self::new()
    }
}

// Axum IntoResponse implementation for API error handling
impl axum::response::IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code();
        let body = serde_json::json!({
            "error": {
                "message": self.to_string(),
                "code": status,
                "category": self.category()
            }
        });

        (
            axum::http::StatusCode::from_u16(status)
                .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
            axum::Json(body),
        )
            .into_response()
    }
}

// Pagination helpers
