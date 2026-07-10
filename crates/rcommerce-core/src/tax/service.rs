//! Tax Service
//!
//! Main tax service implementation for calculating taxes and managing tax data.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::tax::{
    calculator::TaxCalculator, models::*, vat_validation::*, CountrySummary, OssReport, OssScheme,
    OssSummary, OssTransaction, TaxAddress, TaxCalculation, TaxCategory, TaxContext, TaxRate,
    TaxZone, TaxableItem, VatId,
};
use crate::{Error, Result};

/// Tax service trait
#[async_trait]
pub trait TaxService: Send + Sync {
    /// Calculate tax for items
    async fn calculate_tax(
        &self,
        items: &[TaxableItem],
        context: &TaxContext,
    ) -> Result<TaxCalculation>;

    /// Validate a VAT ID
    async fn validate_vat_id(&self, vat_id: &str) -> Result<VatValidationResult>;

    /// Get applicable tax rates for a location
    async fn get_tax_rates(
        &self,
        country_code: &str,
        region_code: Option<&str>,
        postal_code: Option<&str>,
    ) -> Result<Vec<TaxRateWithDetails>>;

    /// Generate OSS report
    async fn generate_oss_report(
        &self,
        scheme: OssScheme,
        period: &str,
        member_state: &str,
    ) -> Result<OssReport>;

    /// Record tax transaction
    async fn record_tax_transaction(
        &self,
        order_id: Uuid,
        calculation: &TaxCalculation,
    ) -> Result<()>;

    /// Create tax zone
    async fn create_tax_zone(&self, request: CreateTaxZoneRequest) -> Result<TaxZone>;

    /// Create tax category
    async fn create_tax_category(&self, request: CreateTaxCategoryRequest) -> Result<TaxCategory>;

    /// Create tax rate
    async fn create_tax_rate(&self, request: CreateTaxRateRequest) -> Result<TaxRate>;

    /// Get tax zones
    async fn get_tax_zones(&self, query: &TaxZoneQuery) -> Result<Vec<TaxZone>>;

    /// Get tax categories
    async fn get_tax_categories(&self) -> Result<Vec<TaxCategory>>;
}

/// Default tax service implementation
pub struct DefaultTaxService {
    db: PgPool,
    vat_validator: ViesValidator,
}

impl DefaultTaxService {
    /// Create a new tax service
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            vat_validator: ViesValidator::new(),
        }
    }

    /// Create with custom VAT validator
    pub fn with_validator(db: PgPool, validator: ViesValidator) -> Self {
        Self {
            db,
            vat_validator: validator,
        }
    }

    /// Get tax rates for a zone
    async fn get_rates_for_zone(&self, zone_id: Uuid) -> Result<Vec<TaxRate>> {
        let rates = sqlx::query_as::<_, TaxRate>(
            r#"
            SELECT * FROM tax_rates
            WHERE tax_zone_id = $1
            AND valid_from <= CURRENT_DATE
            AND (valid_until IS NULL OR valid_until >= CURRENT_DATE)
            ORDER BY priority DESC, tax_category_id NULLS LAST
            "#,
        )
        .bind(zone_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch tax rates: {}", e)))?;

        Ok(rates)
    }

    /// Find or create tax zone for address
    async fn find_tax_zone(&self, address: &TaxAddress) -> Result<TaxZone> {
        // Try to find most specific zone

        // 1. Try postal code pattern match
        if let Some(postal) = &address.postal_code {
            let zone = sqlx::query_as::<_, TaxZone>(
                r#"
                SELECT * FROM tax_zones
                WHERE country_code = $1
                AND postal_code_pattern IS NOT NULL
                AND $2 ~ postal_code_pattern
                LIMIT 1
                "#,
            )
            .bind(&address.country_code)
            .bind(postal)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch tax zone: {}", e)))?;

            if let Some(zone) = zone {
                return Ok(zone);
            }
        }

        // 2. Try region match
        if let Some(region) = &address.region_code {
            let zone = sqlx::query_as::<_, TaxZone>(
                r#"
                SELECT * FROM tax_zones
                WHERE country_code = $1
                AND region_code = $2
                LIMIT 1
                "#,
            )
            .bind(&address.country_code)
            .bind(region)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch tax zone: {}", e)))?;

            if let Some(zone) = zone {
                return Ok(zone);
            }
        }

        // 3. Fall back to country-level zone
        let zone = sqlx::query_as::<_, TaxZone>(
            r#"
            SELECT * FROM tax_zones
            WHERE country_code = $1
            AND region_code IS NULL
            AND postal_code_pattern IS NULL
            LIMIT 1
            "#,
        )
        .bind(&address.country_code)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch tax zone: {}", e)))?;

        if let Some(zone) = zone {
            return Ok(zone);
        }

        // 4. Create default zone if none exists
        warn!("Creating default tax zone for {}", address.country_code);
        self.create_default_zone(&address.country_code).await
    }

    /// Create default tax zone for a country
    async fn create_default_zone(&self, country_code: &str) -> Result<TaxZone> {
        let zone = sqlx::query_as::<_, TaxZone>(
            r#"
            INSERT INTO tax_zones (name, code, country_code, zone_type)
            VALUES ($1, $2, $3, 'country')
            RETURNING *
            "#,
        )
        .bind(format!("Default {}", country_code))
        .bind(country_code.to_uppercase())
        .bind(country_code.to_uppercase())
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create default zone: {}", e)))?;

        Ok(zone)
    }
}

#[async_trait]
impl TaxService for DefaultTaxService {
    async fn calculate_tax(
        &self,
        items: &[TaxableItem],
        context: &TaxContext,
    ) -> Result<TaxCalculation> {
        info!("Calculating tax for {} items", items.len());

        // Get tax zone for destination
        let tax_zone = self.find_tax_zone(&context.shipping_address).await?;
        debug!("Using tax zone: {} ({})", tax_zone.name, tax_zone.code);

        // Get tax rates for zone
        let rates = self.get_rates_for_zone(tax_zone.id).await?;

        // Get tax categories
        let categories = self.get_tax_categories().await?;

        // Build calculator
        let calculator = TaxCalculator::new(rates, vec![tax_zone], categories);

        // Calculate tax
        let calculation = calculator.calculate(items, context)?;

        info!(
            "Tax calculation complete: total_tax={}",
            calculation.total_tax
        );

        Ok(calculation)
    }

    async fn validate_vat_id(&self, vat_id_str: &str) -> Result<VatValidationResult> {
        info!("Validating VAT ID: {}", vat_id_str);

        // Parse VAT ID
        let vat_id = VatId::parse(vat_id_str)?;

        // Check cache first
        let cached: Option<VatValidationCache> = sqlx::query_as(
            r#"
            SELECT * FROM vat_id_validations
            WHERE vat_id = $1
            AND validated_at > NOW() - INTERVAL '30 days'
            ORDER BY validated_at DESC
            LIMIT 1
            "#,
        )
        .bind(vat_id.full_id())
        .fetch_optional(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to check VAT cache: {}", e)))?;

        if let Some(cache) = cached {
            debug!("Using cached VAT validation for {}", vat_id.full_id());
            return Ok(VatValidationResult {
                is_valid: cache.is_valid,
                country_code: cache.country_code,
                vat_number: vat_id.number.clone(),
                business_name: cache.business_name,
                business_address: cache.business_address,
                validated_at: cache.validated_at,
                error_message: None,
            });
        }

        // Validate via VIES
        let result = self.vat_validator.validate(&vat_id).await?;

        // Cache result
        sqlx::query(
            r#"
            INSERT INTO vat_id_validations
            (vat_id, country_code, business_name, business_address, is_valid, validated_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (vat_id) DO UPDATE SET
            business_name = EXCLUDED.business_name,
            business_address = EXCLUDED.business_address,
            is_valid = EXCLUDED.is_valid,
            validated_at = EXCLUDED.validated_at,
            expires_at = EXCLUDED.expires_at
            "#
        )
        .bind(vat_id.full_id())
        .bind(&result.country_code)
        .bind(&result.business_name)
        .bind(&result.business_address)
        .bind(result.is_valid)
        .bind(result.validated_at)
        .bind(result.validated_at + chrono::Duration::days(30))
        .execute(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to cache VAT validation: {}", e)))?;

        Ok(result)
    }

    async fn get_tax_rates(
        &self,
        country_code: &str,
        region_code: Option<&str>,
        postal_code: Option<&str>,
    ) -> Result<Vec<TaxRateWithDetails>> {
        let address = TaxAddress {
            country_code: country_code.to_string(),
            region_code: region_code.map(|s| s.to_string()),
            postal_code: postal_code.map(|s| s.to_string()),
            city: None,
        };

        let zone = self.find_tax_zone(&address).await?;
        let rates = self.get_rates_for_zone(zone.id).await?;

        // Fetch zone and category info for each rate
        let mut result = Vec::new();
        for rate in rates {
            let category = if let Some(cat_id) = rate.tax_category_id {
                sqlx::query_as::<_, TaxCategory>("SELECT * FROM tax_categories WHERE id = $1")
                    .bind(cat_id)
                    .fetch_optional(&self.db)
                    .await
                    .map_err(|e| Error::Other(format!("Failed to fetch category: {}", e)))?
            } else {
                None
            };

            result.push(TaxRateWithDetails {
                id: rate.id,
                name: rate.name.clone(),
                rate: rate.rate,
                rate_type: rate.rate_type.clone(),
                is_vat: rate.is_vat,
                vat_type: rate.vat_type.clone(),
                b2b_exempt: rate.b2b_exempt,
                reverse_charge: rate.reverse_charge,
                tax_zone: TaxZoneInfo {
                    id: zone.id,
                    name: zone.name.clone(),
                    code: zone.code.clone(),
                    country_code: zone.country_code.clone(),
                },
                tax_category: category.map(|c| TaxCategoryInfo {
                    id: c.id,
                    name: c.name,
                    code: c.code,
                }),
                valid_from: rate.valid_from,
                valid_until: rate.valid_until,
            });
        }

        Ok(result)
    }

    async fn generate_oss_report(
        &self,
        scheme: OssScheme,
        period: &str,
        member_state: &str,
    ) -> Result<OssReport> {
        info!(
            "Generating OSS report: scheme={:?}, period={}, member_state={}",
            scheme, period, member_state
        );

        // Parse period (YYYY-MM) for validation
        let (_year, _month): (i32, i32) = {
            let parts: Vec<&str> = period.split('-').collect();
            if parts.len() != 2 {
                return Err(Error::validation("Invalid period format, expected YYYY-MM"));
            }
            (
                parts[0]
                    .parse()
                    .map_err(|_| Error::validation("Invalid year"))?,
                parts[1]
                    .parse()
                    .map_err(|_| Error::validation("Invalid month"))?,
            )
        };

        let scheme_str = match scheme {
            OssScheme::Union => "union",
            OssScheme::NonUnion => "non_union",
            OssScheme::Import => "import",
        };

        // Fetch transactions for period
        let transactions: Vec<OssTransactionRow> = sqlx::query_as(
            r#"
            SELECT 
                country_code,
                tax_rate,
                SUM(taxable_amount) as taxable_amount,
                SUM(tax_amount) as tax_amount,
                COUNT(*) as transaction_count
            FROM tax_transactions
            WHERE oss_scheme = $1
            AND oss_period = $2
            GROUP BY country_code, tax_rate
            ORDER BY country_code
            "#,
        )
        .bind(scheme_str)
        .bind(period)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch OSS transactions: {}", e)))?;

        let total_taxable: Decimal = transactions.iter().map(|t| t.taxable_amount).sum();
        let total_tax: Decimal = transactions.iter().map(|t| t.tax_amount).sum();
        let total_count: i64 = transactions.iter().map(|t| t.transaction_count).sum();

        let oss_transactions: Vec<OssTransaction> = transactions
            .into_iter()
            .map(|t| OssTransaction {
                country_code: t.country_code,
                vat_rate: t.tax_rate,
                taxable_amount: t.taxable_amount,
                vat_amount: t.tax_amount,
                transaction_count: t.transaction_count as i32,
            })
            .collect();

        let by_country: Vec<CountrySummary> = oss_transactions
            .iter()
            .map(|t| CountrySummary {
                country_code: t.country_code.clone(),
                country_name: country_name(&t.country_code),
                vat_rate: t.vat_rate,
                taxable_amount: t.taxable_amount,
                vat_amount: t.vat_amount,
                transaction_count: t.transaction_count,
            })
            .collect();

        Ok(OssReport {
            scheme,
            period: period.to_string(),
            member_state: member_state.to_string(),
            transactions: oss_transactions,
            summary: OssSummary {
                total_taxable_amount: total_taxable,
                total_vat_amount: total_tax,
                total_transactions: total_count as i32,
                by_country,
            },
        })
    }

    async fn record_tax_transaction(
        &self,
        order_id: Uuid,
        calculation: &TaxCalculation,
    ) -> Result<()> {
        debug!("Recording tax transactions for order {}", order_id);

        for line_item in &calculation.line_items {
            // Skip lines with no tax recorded (e.g. zero-rated or exempt)
            if line_item.tax_amount.is_zero() || line_item.tax_rate_id.is_nil() {
                continue;
            }

            // Determine OSS scheme
            // TODO: This needs order context to determine properly
            let oss_scheme: Option<&str> = None;

            sqlx::query(
                r#"
                INSERT INTO tax_transactions
                (order_id, order_item_id, tax_rate_id, tax_zone_id, tax_category_id,
                 taxable_amount, tax_amount, tax_rate, country_code, region_code, oss_scheme, oss_period)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                "#
            )
            .bind(order_id)
            .bind(line_item.item_id)
            .bind(line_item.tax_rate_id)
            .bind(line_item.tax_zone_id)
            .bind(None::<Uuid>) // TODO: tax_category_id
            .bind(line_item.taxable_amount)
            .bind(line_item.tax_amount)
            .bind(line_item.tax_rate)
            .bind("") // TODO: country_code
            .bind(None::<String>) // TODO: region_code
            .bind(oss_scheme)
            .bind(None::<String>) // TODO: oss_period
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to record tax transaction: {}", e)))?;
        }

        Ok(())
    }

    async fn create_tax_zone(&self, request: CreateTaxZoneRequest) -> Result<TaxZone> {
        info!("Creating tax zone: {} ({})", request.name, request.code);

        let zone = sqlx::query_as::<_, TaxZone>(
            r#"
            INSERT INTO tax_zones (name, code, country_code, region_code, postal_code_pattern, zone_type, parent_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#
        )
        .bind(request.name)
        .bind(request.code.to_uppercase())
        .bind(request.country_code.to_uppercase())
        .bind(request.region_code.map(|s| s.to_uppercase()))
        .bind(request.postal_code_pattern)
        .bind(request.zone_type.to_string())
        .bind(request.parent_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create tax zone: {}", e)))?;

        Ok(zone)
    }

    async fn create_tax_category(&self, request: CreateTaxCategoryRequest) -> Result<TaxCategory> {
        info!("Creating tax category: {} ({})", request.name, request.code);

        let category = sqlx::query_as::<_, TaxCategory>(
            r#"
            INSERT INTO tax_categories (name, code, description, is_digital, is_food, is_luxury, is_medical, is_educational)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#
        )
        .bind(request.name)
        .bind(request.code.to_uppercase())
        .bind(request.description)
        .bind(request.is_digital)
        .bind(request.is_food)
        .bind(request.is_luxury)
        .bind(request.is_medical)
        .bind(request.is_educational)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create tax category: {}", e)))?;

        Ok(category)
    }

    async fn create_tax_rate(&self, request: CreateTaxRateRequest) -> Result<TaxRate> {
        info!(
            "Creating tax rate: {} = {}%",
            request.name,
            request.rate * Decimal::from(100)
        );

        let vat_type_str = request.vat_type.map(|v| v.to_string());

        let rate = sqlx::query_as::<_, TaxRate>(
            r#"
            INSERT INTO tax_rates (name, tax_zone_id, tax_category_id, rate, rate_type, is_vat, vat_type,
                                   b2b_exempt, reverse_charge, valid_from, valid_until, priority)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING *
            "#
        )
        .bind(request.name)
        .bind(request.tax_zone_id)
        .bind(request.tax_category_id)
        .bind(request.rate)
        .bind(request.rate_type.to_string())
        .bind(request.is_vat)
        .bind(vat_type_str)
        .bind(request.b2b_exempt)
        .bind(request.reverse_charge)
        .bind(request.valid_from)
        .bind(request.valid_until)
        .bind(request.priority)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create tax rate: {}", e)))?;

        Ok(rate)
    }

    async fn get_tax_zones(&self, query: &TaxZoneQuery) -> Result<Vec<TaxZone>> {
        let mut sql = String::from("SELECT * FROM tax_zones WHERE 1=1");

        if query.country_code.is_some() {
            sql.push_str(" AND country_code = $1");
        }
        if query.zone_type.is_some() {
            sql.push_str(&format!(
                " AND zone_type = ${}",
                if query.country_code.is_some() { 2 } else { 1 }
            ));
        }
        if query.search.is_some() {
            sql.push_str(&format!(
                " AND (name ILIKE ${} OR code ILIKE ${})",
                if query.country_code.is_some() { 3 } else { 2 },
                if query.country_code.is_some() { 4 } else { 3 }
            ));
        }

        sql.push_str(" ORDER BY name");

        let mut query_builder = sqlx::query_as::<_, TaxZone>(&sql);

        if let Some(country) = &query.country_code {
            query_builder = query_builder.bind(country.to_uppercase());
        }
        if let Some(zone_type) = &query.zone_type {
            query_builder = query_builder.bind(zone_type.to_lowercase());
        }
        if let Some(search) = &query.search {
            let pattern = format!("%{}%", search);
            query_builder = query_builder.bind(pattern.clone()).bind(pattern);
        }

        let zones = query_builder
            .fetch_all(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch tax zones: {}", e)))?;

        Ok(zones)
    }

    async fn get_tax_categories(&self) -> Result<Vec<TaxCategory>> {
        let categories =
            sqlx::query_as::<_, TaxCategory>("SELECT * FROM tax_categories ORDER BY name")
                .fetch_all(&self.db)
                .await
                .map_err(|e| Error::Other(format!("Failed to fetch tax categories: {}", e)))?;

        Ok(categories)
    }
}

/// Helper struct for OSS query
#[derive(Debug, sqlx::FromRow)]
struct OssTransactionRow {
    country_code: String,
    tax_rate: Decimal,
    taxable_amount: Decimal,
    tax_amount: Decimal,
    transaction_count: i64,
}

/// VAT validation cache row
#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct VatValidationCache {
    vat_id: String,
    country_code: String,
    business_name: Option<String>,
    business_address: Option<String>,
    is_valid: bool,
    validated_at: DateTime<Utc>,
}

/// Get country name from code
fn country_name(code: &str) -> String {
    let countries: std::collections::HashMap<&str, &str> = [
        ("AT", "Austria"),
        ("BE", "Belgium"),
        ("BG", "Bulgaria"),
        ("HR", "Croatia"),
        ("CY", "Cyprus"),
        ("CZ", "Czech Republic"),
        ("DK", "Denmark"),
        ("EE", "Estonia"),
        ("FI", "Finland"),
        ("FR", "France"),
        ("DE", "Germany"),
        ("GR", "Greece"),
        ("HU", "Hungary"),
        ("IE", "Ireland"),
        ("IT", "Italy"),
        ("LV", "Latvia"),
        ("LT", "Lithuania"),
        ("LU", "Luxembourg"),
        ("MT", "Malta"),
        ("NL", "Netherlands"),
        ("PL", "Poland"),
        ("PT", "Portugal"),
        ("RO", "Romania"),
        ("SK", "Slovakia"),
        ("SI", "Slovenia"),
        ("ES", "Spain"),
        ("SE", "Sweden"),
    ]
    .into();

    countries
        .get(code)
        .map(|&s| s.to_string())
        .unwrap_or_else(|| code.to_string())
}
