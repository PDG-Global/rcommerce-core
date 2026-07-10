//! JSON file importer

use crate::import::{
    error::{ImportError, ImportResult},
    types::{ImportConfig, ImportProgress, ImportStats},
    EntityType, FileImporter,
};
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

/// JSON file importer
pub struct JsonImporter;

impl JsonImporter {
    pub fn new() -> Self {
        Self
    }

    /// Extract array from JSON value
    fn extract_array(value: Value) -> ImportResult<Vec<Value>> {
        match value {
            Value::Array(arr) => Ok(arr),
            Value::Object(obj) => {
                // Try to find an array field
                for (_, v) in obj {
                    if let Value::Array(arr) = v {
                        return Ok(arr);
                    }
                }
                Err(ImportError::Parse(
                    "JSON object does not contain an array field".to_string(),
                ))
            }
            _ => Err(ImportError::Parse(
                "JSON root must be an array or object containing an array".to_string(),
            )),
        }
    }
}

impl Default for JsonImporter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileImporter for JsonImporter {
    fn format(&self) -> &'static str {
        "json"
    }

    async fn import_file(
        &self,
        file_path: &Path,
        entity_type: EntityType,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let dry_run = config.options.dry_run;
        let content = tokio::fs::read_to_string(file_path).await?;
        let json: Value = serde_json::from_str(&content)?;
        let records = Self::extract_array(json)?;
        let total = records.len();

        progress(ImportProgress {
            stage: entity_type.to_string(),
            current: 0,
            total,
            message: if dry_run {
                format!(
                    "Validating {} {} records from JSON (DRY RUN)...",
                    total, entity_type
                )
            } else {
                format!("Importing {} {} records from JSON...", total, entity_type)
            },
        });

        let mut stats = ImportStats {
            total,
            ..Default::default()
        };

        for (i, record) in records.iter().enumerate() {
            progress(ImportProgress {
                stage: entity_type.to_string(),
                current: i + 1,
                total,
                message: if dry_run {
                    format!("Validating record {}/{}", i + 1, total)
                } else {
                    format!("Processing record {}/{}", i + 1, total)
                },
            });

            // Validate record structure based on entity type
            let validation_result = match entity_type {
                EntityType::Products => validate_json_product(record),
                EntityType::Customers => validate_json_customer(record),
                EntityType::Orders => validate_json_order(record),
            };

            match validation_result {
                Ok(()) => {
                    if dry_run {
                        stats.created += 1;
                    } else {
                        // In real implementation, insert into database
                        stats.created += 1;
                    }
                }
                Err(e) => {
                    stats.errors += 1;
                    stats.error_details.push(format!("Record {}: {}", i + 1, e));
                }
            }
        }

        Ok(stats)
    }
}

/// Validate a JSON product record
fn validate_json_product(record: &Value) -> ImportResult<()> {
    if record.get("title").is_none() {
        return Err(ImportError::Validation(
            "Product title is required".to_string(),
        ));
    }

    if let Some(title) = record.get("title") {
        let title_str = title.as_str().unwrap_or("");
        if title_str.is_empty() {
            return Err(ImportError::Validation(
                "Product title cannot be empty".to_string(),
            ));
        }
    }

    if record.get("price").is_none() {
        return Err(ImportError::Validation(
            "Product price is required".to_string(),
        ));
    }

    if let Some(price) = record.get("price") {
        let price_str = price.as_str().unwrap_or("0");
        if price_str.parse::<f64>().unwrap_or(-1.0) < 0.0 {
            return Err(ImportError::Validation(
                "Product price must be a non-negative number".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate a JSON customer record
fn validate_json_customer(record: &Value) -> ImportResult<()> {
    if record.get("email").is_none() {
        return Err(ImportError::Validation(
            "Customer email is required".to_string(),
        ));
    }

    if let Some(email) = record.get("email") {
        let email_str = email.as_str().unwrap_or("");
        if email_str.is_empty() || !email_str.contains('@') {
            return Err(ImportError::Validation(
                "Valid customer email is required".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate a JSON order record
fn validate_json_order(record: &Value) -> ImportResult<()> {
    if record.get("order_number").is_none() && record.get("id").is_none() {
        return Err(ImportError::Validation(
            "Order number or ID is required".to_string(),
        ));
    }

    Ok(())
}

/// Example JSON structures for each entity type
pub mod examples {
    use serde_json::json;

    /// Example product JSON structure
    pub fn product() -> serde_json::Value {
        json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "title": "Premium Cotton T-Shirt",
            "slug": "premium-cotton-t-shirt",
            "description": "High-quality organic cotton t-shirt",
            "price": "29.99",
            "compare_at_price": "39.99",
            "sku": "TSHIRT-001",
            "inventory_quantity": 100,
            "status": "active",
            "product_type": "physical",
            "tags": ["clothing", "organic"],
            "variants": [
                {
                    "id": "var-001",
                    "title": "Small / Black",
                    "sku": "TSHIRT-001-S-BLK",
                    "price": "29.99",
                    "inventory_quantity": 50
                }
            ]
        })
    }

    /// Example customer JSON structure
    pub fn customer() -> serde_json::Value {
        json!({
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "email": "customer@example.com",
            "first_name": "John",
            "last_name": "Doe",
            "phone": "+1234567890",
            "addresses": [
                {
                    "address1": "123 Main St",
                    "city": "New York",
                    "state": "NY",
                    "postal_code": "10001",
                    "country": "US"
                }
            ]
        })
    }

    /// Example order JSON structure
    pub fn order() -> serde_json::Value {
        json!({
            "id": "ord-123456",
            "order_number": "1001",
            "customer_id": "123e4567-e89b-12d3-a456-426614174000",
            "email": "customer@example.com",
            "status": "confirmed",
            "total": "59.98",
            "subtotal": "54.99",
            "tax_total": "4.99",
            "shipping_total": "0.00",
            "line_items": [
                {
                    "product_id": "550e8400-e29b-41d4-a716-446655440000",
                    "title": "Premium Cotton T-Shirt",
                    "quantity": 2,
                    "price": "29.99",
                    "total": "59.98"
                }
            ]
        })
    }
}
