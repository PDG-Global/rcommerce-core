//! External catalogue search engines (see `[search]` in the config).
//!
//! PostgreSQL full-text search (`repository::catalog_i18n`) is the default and
//! needs no extra service. Elasticsearch adds typo tolerance, accent-insensitive
//! matching and infix matching for compound words, and takes search load off
//! the database.

pub mod elasticsearch;

pub use elasticsearch::ElasticsearchCatalog;
