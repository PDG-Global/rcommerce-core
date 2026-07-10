//! CSV file importer

use crate::import::{
    error::{ImportError, ImportResult},
    types::{ImportConfig, ImportProgress, ImportStats},
    EntityType, FileImporter,
};
use async_trait::async_trait;
use csv::ReaderBuilder;
use serde_json::Value;
use std::path::Path;

/// CSV file importer
pub struct CsvImporter;

impl CsvImporter {
    pub fn new() -> Self {
        Self
    }

    /// Convert CSV record to JSON Value
    fn record_to_value(
        record: &csv::StringRecord,
        headers: &csv::StringRecord,
    ) -> ImportResult<Value> {
        let mut map = serde_json::Map::new();

        for (i, header) in headers.iter().enumerate() {
            let value = record.get(i).unwrap_or("").to_string();
            map.insert(header.to_string(), Value::String(value));
        }

        Ok(Value::Object(map))
    }
}

impl Default for CsvImporter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileImporter for CsvImporter {
    fn format(&self) -> &'static str {
        "csv"
    }

    async fn import_file(
        &self,
        file_path: &Path,
        entity_type: EntityType,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let dry_run = config.options.dry_run;
        let file = std::fs::File::open(file_path)?;
        let mut reader = ReaderBuilder::new().has_headers(true).from_reader(file);

        let headers = reader.headers()?.clone();

        // Count total rows
        let total_rows = reader.records().count();

        progress(ImportProgress {
            stage: entity_type.to_string(),
            current: 0,
            total: total_rows,
            message: if dry_run {
                format!(
                    "Validating {} {} records from CSV (DRY RUN)...",
                    total_rows, entity_type
                )
            } else {
                format!(
                    "Importing {} {} records from CSV...",
                    total_rows, entity_type
                )
            },
        });

        // Re-create reader since we consumed it
        let file = std::fs::File::open(file_path)?;
        let mut reader = ReaderBuilder::new().has_headers(true).from_reader(file);

        // Skip headers
        reader.headers()?;

        let mut stats = ImportStats {
            total: total_rows,
            ..Default::default()
        };

        for (i, result) in reader.records().enumerate() {
            let record = result?;

            progress(ImportProgress {
                stage: entity_type.to_string(),
                current: i + 1,
                total: total_rows,
                message: if dry_run {
                    format!("Validating row {}/{}", i + 1, total_rows)
                } else {
                    format!("Processing row {}/{}", i + 1, total_rows)
                },
            });

            // Convert to JSON and process
            match Self::record_to_value(&record, &headers) {
                Ok(_value) => {
                    // Validate the data
                    if let Err(e) = validate_csv_record(&_value, &entity_type) {
                        stats.errors += 1;
                        stats.error_details.push(format!("Row {}: {}", i + 1, e));
                        continue;
                    }

                    if dry_run {
                        stats.created += 1;
                    } else {
                        // In real implementation, insert into database
                        stats.created += 1;
                    }
                }
                Err(e) => {
                    stats.errors += 1;
                    stats.error_details.push(format!("Row {}: {}", i + 1, e));
                }
            }
        }

        Ok(stats)
    }
}

/// Validate a CSV record based on entity type
fn validate_csv_record(value: &Value, entity_type: &EntityType) -> ImportResult<()> {
    match entity_type {
        EntityType::Products => {
            if let Some(obj) = value.as_object() {
                if let Some(title) = obj.get("title") {
                    let title_str = title.as_str().unwrap_or("");
                    if title_str.is_empty() {
                        return Err(ImportError::Validation(
                            "Product title is required".to_string(),
                        ));
                    }
                }
                if let Some(price) = obj.get("price") {
                    let price_str = price.as_str().unwrap_or("0");
                    if price_str.parse::<f64>().unwrap_or(-1.0) < 0.0 {
                        return Err(ImportError::Validation(
                            "Product price must be a non-negative number".to_string(),
                        ));
                    }
                }
            }
        }
        EntityType::Customers => {
            if let Some(obj) = value.as_object() {
                if let Some(email) = obj.get("email") {
                    let email_str = email.as_str().unwrap_or("");
                    if email_str.is_empty() || !email_str.contains('@') {
                        return Err(ImportError::Validation(
                            "Valid customer email is required".to_string(),
                        ));
                    }
                }
            }
        }
        EntityType::Orders => {
            if let Some(obj) = value.as_object() {
                if let Some(order_num) = obj.get("order_number") {
                    let order_str = order_num.as_str().unwrap_or("");
                    if order_str.is_empty() {
                        return Err(ImportError::Validation(
                            "Order number is required".to_string(),
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Expected CSV columns for each entity type
pub mod columns {
    /// Product CSV columns
    pub const PRODUCTS: &[&str] = &[
        "id",
        "title",
        "slug",
        "description",
        "price",
        "compare_at_price",
        "sku",
        "inventory_quantity",
        "status",
        "product_type",
    ];

    /// Customer CSV columns
    pub const CUSTOMERS: &[&str] = &[
        "id",
        "email",
        "first_name",
        "last_name",
        "phone",
        "address1",
        "address2",
        "city",
        "state",
        "postal_code",
        "country",
    ];

    /// Order CSV columns
    pub const ORDERS: &[&str] = &[
        "id",
        "order_number",
        "customer_id",
        "email",
        "status",
        "total",
        "subtotal",
        "tax_total",
        "shipping_total",
        "created_at",
        "line_items",
    ];
}
