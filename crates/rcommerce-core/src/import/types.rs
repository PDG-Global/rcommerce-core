//! Import types and configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for import operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportConfig {
    /// Database connection string
    pub database_url: String,

    /// Source configuration (API keys, URLs, etc.)
    #[serde(flatten)]
    pub source: SourceConfig,

    /// Import options
    pub options: ImportOptions,
}

/// Source-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum SourceConfig {
    /// Platform API configuration
    Platform {
        /// Platform name
        platform: String,
        /// API base URL
        api_url: String,
        /// API key or access token
        api_key: String,
        /// Additional headers
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    /// File-based import
    File {
        /// File path
        path: PathBuf,
        /// File format (csv, json, xml)
        format: String,
    },
}

/// Import options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOptions {
    /// Skip existing records
    #[serde(default = "default_true")]
    pub skip_existing: bool,

    /// Update existing records
    #[serde(default)]
    pub update_existing: bool,

    /// Batch size for imports
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Maximum number of records to import (0 = unlimited)
    #[serde(default)]
    pub limit: usize,

    /// Dry run - validate without importing
    #[serde(default)]
    pub dry_run: bool,

    /// Continue on error
    #[serde(default = "default_true")]
    pub continue_on_error: bool,

    /// Field mappings
    #[serde(default)]
    pub field_mappings: HashMap<String, String>,

    /// Default values for missing fields
    #[serde(default)]
    pub default_values: HashMap<String, serde_json::Value>,

    /// Transform rules
    #[serde(default)]
    pub transforms: Vec<TransformRule>,

    /// Default currency for imported records (e.g., "USD", "AUD", "EUR")
    #[serde(default = "default_currency")]
    pub default_currency: String,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            skip_existing: true,
            update_existing: false,
            batch_size: 100,
            limit: 0,
            dry_run: false,
            continue_on_error: true,
            field_mappings: HashMap::new(),
            default_values: HashMap::new(),
            transforms: Vec::new(),
            default_currency: default_currency(),
        }
    }
}

/// Transform rule for data transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformRule {
    /// Field to transform
    pub field: String,
    /// Transform operation
    pub operation: TransformOperation,
}

/// Transform operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformOperation {
    /// Replace values
    Replace { from: String, to: String },
    /// Prefix addition
    Prefix { value: String },
    /// Suffix addition
    Suffix { value: String },
    /// Custom mapping
    Map { values: HashMap<String, String> },
    /// Uppercase
    Uppercase,
    /// Lowercase
    Lowercase,
    /// Trim whitespace
    Trim,
}

/// Import progress information
#[derive(Debug, Clone)]
pub struct ImportProgress {
    /// Current stage (e.g., "products", "customers", "orders")
    pub stage: String,
    /// Current item number
    pub current: usize,
    /// Total items
    pub total: usize,
    /// Progress message
    pub message: String,
}

impl ImportProgress {
    /// Calculate percentage complete
    pub fn percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.current as f64 / self.total as f64) * 100.0
        }
    }
}

/// Import statistics
#[derive(Debug, Clone, Default)]
pub struct ImportStats {
    /// Number of records created
    pub created: usize,
    /// Number of records updated
    pub updated: usize,
    /// Number of records skipped (duplicates)
    pub skipped: usize,
    /// Number of records with errors
    pub errors: usize,
    /// Total records processed
    pub total: usize,
    /// Error details
    pub error_details: Vec<String>,
}

impl ImportStats {
    /// Add another stats to this one
    pub fn add(&mut self, other: &ImportStats) {
        self.created += other.created;
        self.updated += other.updated;
        self.skipped += other.skipped;
        self.errors += other.errors;
        self.total += other.total;
        self.error_details.extend(other.error_details.clone());
    }

    /// Check if import was successful (no errors)
    pub fn is_success(&self) -> bool {
        self.errors == 0
    }

    /// Get summary string
    pub fn summary(&self) -> String {
        format!(
            "Created: {}, Updated: {}, Skipped: {}, Errors: {}, Total: {}",
            self.created, self.updated, self.skipped, self.errors, self.total
        )
    }
}

fn default_true() -> bool {
    true
}

fn default_batch_size() -> usize {
    100
}

fn default_currency() -> String {
    "USD".to_string()
}
