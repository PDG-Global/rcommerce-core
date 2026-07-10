//! Medusa.js importer implementation (stub)

use crate::import::{
    error::{ImportError, ImportResult},
    types::{ImportConfig, ImportProgress, ImportStats},
    PlatformImporter,
};
use async_trait::async_trait;

/// Medusa.js API client and importer
pub struct MedusaImporter;

impl MedusaImporter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MedusaImporter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformImporter for MedusaImporter {
    fn platform(&self) -> &'static str {
        "medusa"
    }

    async fn import_products(
        &self,
        _config: &ImportConfig,
        _progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        Err(ImportError::Other(
            "Medusa importer not yet implemented. Use CSV/JSON export instead.".to_string(),
        ))
    }

    async fn import_customers(
        &self,
        _config: &ImportConfig,
        _progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        Err(ImportError::Other(
            "Medusa importer not yet implemented. Use CSV/JSON export instead.".to_string(),
        ))
    }

    async fn import_orders(
        &self,
        _config: &ImportConfig,
        _progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        Err(ImportError::Other(
            "Medusa importer not yet implemented. Use CSV/JSON export instead.".to_string(),
        ))
    }
}
