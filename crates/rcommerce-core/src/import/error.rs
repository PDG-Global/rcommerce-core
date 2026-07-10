//! Import error types

use thiserror::Error;

/// Errors that can occur during import operations
#[derive(Error, Debug)]
pub enum ImportError {
    #[error("Platform not supported: {0}")]
    UnsupportedPlatform(String),

    #[error("File format not supported: {0}")]
    UnsupportedFormat(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Database error: {0}")]
    Database(#[from] crate::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("XML error: {0}")]
    Xml(String),

    #[error("Import cancelled by user")]
    Cancelled,

    #[error("Rate limit exceeded. Retry after: {0} seconds")]
    RateLimit(u64),

    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("{entity_type} with identifier '{identifier}' already exists")]
    Duplicate {
        entity_type: String,
        identifier: String,
    },

    #[error("Import failed: {0}")]
    Other(String),
}

/// Result type for import operations
pub type ImportResult<T> = Result<T, ImportError>;
