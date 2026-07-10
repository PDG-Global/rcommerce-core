//! Tax Providers
//!
//! External tax provider integrations (Avalara, TaxJar, etc.)

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::tax::{TaxAddress, TaxCalculation, TaxContext, TaxableItem};
use crate::{Error, Result};

/// External tax provider trait
#[async_trait]
pub trait TaxProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Calculate tax
    async fn calculate_tax(
        &self,
        items: &[TaxableItem],
        context: &TaxContext,
    ) -> Result<TaxCalculation>;

    /// Validate address for tax purposes
    async fn validate_address(&self, address: &TaxAddress) -> Result<ValidatedAddress>;

    /// Check if provider is available
    async fn health_check(&self) -> Result<bool>;
}

/// Validated address result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedAddress {
    pub country_code: String,
    pub region_code: String,
    pub city: String,
    pub postal_code: String,
    pub street: String,
    pub is_valid: bool,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// Avalara AvaTax provider
pub struct AvalaraProvider {
    client: reqwest::Client,
    api_key: String,
    account_id: String,
    base_url: String,
}

impl AvalaraProvider {
    /// Create new Avalara provider
    pub fn new(api_key: String, account_id: String, sandbox: bool) -> Self {
        let base_url = if sandbox {
            "https://sandbox-rest.avatax.com".to_string()
        } else {
            "https://rest.avatax.com".to_string()
        };

        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            api_key,
            account_id,
            base_url,
        }
    }

    /// Build authorization header
    fn auth_header(&self) -> String {
        use base64::Engine;
        let credentials = format!("{}:{}", self.account_id, self.api_key);
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(credentials)
        )
    }
}

#[async_trait]
impl TaxProvider for AvalaraProvider {
    fn name(&self) -> &str {
        "Avalara AvaTax"
    }

    async fn calculate_tax(
        &self,
        items: &[TaxableItem],
        context: &TaxContext,
    ) -> Result<TaxCalculation> {
        let request = AvalaraTaxRequest {
            lines: items
                .iter()
                .map(|item| AvalaraLineItem {
                    number: item.id.to_string(),
                    quantity: item.quantity,
                    amount: item.total_price,
                    tax_code: item
                        .tax_category_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                    description: item.title.clone(),
                })
                .collect(),
            addresses: AvalaraAddresses {
                ship_to: AvalaraAddress {
                    line1: "".to_string(), // TODO: Full address
                    city: context.shipping_address.city.clone().unwrap_or_default(),
                    region: context
                        .shipping_address
                        .region_code
                        .clone()
                        .unwrap_or_default(),
                    country: context.shipping_address.country_code.clone(),
                    postal_code: context
                        .shipping_address
                        .postal_code
                        .clone()
                        .unwrap_or_default(),
                },
                ship_from: None, // TODO: Business address
            },
            customer_code: context
                .customer
                .customer_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            currency_code: context.currency.to_string(),
        };

        let response = self
            .client
            .post(format!("{}/api/v2/transactions/create", self.base_url))
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Avalara request failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(Error::Network(format!("Avalara error: {}", error)));
        }

        let _result: AvalaraTaxResponse = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Avalara response: {}", e)))?;

        // Convert Avalara response to our TaxCalculation format
        // TODO: Implement full conversion
        Ok(TaxCalculation::new())
    }

    async fn validate_address(&self, address: &TaxAddress) -> Result<ValidatedAddress> {
        let request = AvalaraAddressValidationRequest {
            line1: "".to_string(),
            city: address.city.clone().unwrap_or_default(),
            region: address.region_code.clone().unwrap_or_default(),
            country: address.country_code.clone(),
            postal_code: address.postal_code.clone().unwrap_or_default(),
        };

        let response = self
            .client
            .post(format!("{}/api/v2/addresses/resolve", self.base_url))
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Avalara address validation failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(Error::Network(format!("Avalara error: {}", error)));
        }

        let result: AvalaraAddressValidationResponse = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Avalara response: {}", e)))?;

        Ok(ValidatedAddress {
            country_code: result.address.country,
            region_code: result.address.region,
            city: result.address.city,
            postal_code: result.address.postal_code,
            street: result.address.line1,
            is_valid: result.resolution_quality == "Intersection"
                || result.resolution_quality == "Exact",
            latitude: result.coordinates.as_ref().map(|c| c.latitude),
            longitude: result.coordinates.as_ref().map(|c| c.longitude),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{}/api/v2/utilities/ping", self.base_url))
            .header("Authorization", self.auth_header())
            .send()
            .await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

/// Avalara tax request
#[derive(Debug, Serialize)]
struct AvalaraTaxRequest {
    lines: Vec<AvalaraLineItem>,
    addresses: AvalaraAddresses,
    #[serde(rename = "customerCode")]
    customer_code: String,
    #[serde(rename = "currencyCode")]
    currency_code: String,
}

#[derive(Debug, Serialize)]
struct AvalaraLineItem {
    number: String,
    quantity: i32,
    amount: Decimal,
    #[serde(rename = "taxCode")]
    tax_code: String,
    description: String,
}

#[derive(Debug, Serialize)]
struct AvalaraAddresses {
    #[serde(rename = "shipTo")]
    ship_to: AvalaraAddress,
    #[serde(rename = "shipFrom")]
    ship_from: Option<AvalaraAddress>,
}

#[derive(Debug, Serialize)]
struct AvalaraAddress {
    line1: String,
    city: String,
    region: String,
    country: String,
    #[serde(rename = "postalCode")]
    postal_code: String,
}

/// Avalara tax response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AvalaraTaxResponse {
    #[serde(rename = "totalTax")]
    total_tax: Decimal,
    #[serde(rename = "totalTaxable")]
    total_taxable: Decimal,
    lines: Vec<AvalaraLineResponse>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AvalaraLineResponse {
    #[serde(rename = "lineNumber")]
    line_number: String,
    #[serde(rename = "tax")]
    tax: Decimal,
    #[serde(rename = "taxableAmount")]
    taxable_amount: Decimal,
    details: Vec<AvalaraTaxDetail>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AvalaraTaxDetail {
    #[serde(rename = "taxName")]
    tax_name: String,
    #[serde(rename = "taxAmount")]
    tax_amount: Decimal,
    rate: Decimal,
}

/// Avalara address validation request
#[derive(Debug, Serialize)]
struct AvalaraAddressValidationRequest {
    line1: String,
    city: String,
    region: String,
    country: String,
    #[serde(rename = "postalCode")]
    postal_code: String,
}

/// Avalara address validation response
#[derive(Debug, Deserialize)]
struct AvalaraAddressValidationResponse {
    address: AvalaraValidatedAddress,
    #[serde(rename = "resolutionQuality")]
    resolution_quality: String,
    coordinates: Option<AvalaraCoordinates>,
}

#[derive(Debug, Deserialize)]
struct AvalaraValidatedAddress {
    line1: String,
    city: String,
    region: String,
    country: String,
    #[serde(rename = "postalCode")]
    postal_code: String,
}

#[derive(Debug, Deserialize)]
struct AvalaraCoordinates {
    latitude: f64,
    longitude: f64,
}

/// TaxJar provider
pub struct TaxJarProvider {
    client: reqwest::Client,
    api_token: String,
    base_url: String,
}

impl TaxJarProvider {
    /// Create new TaxJar provider
    pub fn new(api_token: String, sandbox: bool) -> Self {
        let base_url = if sandbox {
            "https://api.sandbox.taxjar.com".to_string()
        } else {
            "https://api.taxjar.com".to_string()
        };

        Self {
            client: reqwest::Client::new(),
            api_token,
            base_url,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_token)
    }
}

#[async_trait]
impl TaxProvider for TaxJarProvider {
    fn name(&self) -> &str {
        "TaxJar"
    }

    async fn calculate_tax(
        &self,
        items: &[TaxableItem],
        context: &TaxContext,
    ) -> Result<TaxCalculation> {
        let request = TaxJarTaxRequest {
            to_country: context.shipping_address.country_code.clone(),
            to_zip: context
                .shipping_address
                .postal_code
                .clone()
                .unwrap_or_default(),
            to_state: context
                .shipping_address
                .region_code
                .clone()
                .unwrap_or_default(),
            to_city: context.shipping_address.city.clone().unwrap_or_default(),
            amount: items.iter().map(|i| i.total_price).sum(),
            shipping: Decimal::ZERO, // TODO
            line_items: items
                .iter()
                .map(|item| TaxJarLineItem {
                    id: item.id.to_string(),
                    quantity: item.quantity,
                    product_tax_code: item.tax_category_id.map(|id| id.to_string()),
                    unit_price: item.unit_price,
                    discount: Decimal::ZERO,
                })
                .collect(),
        };

        let response = self
            .client
            .post(format!("{}/v2/taxes", self.base_url))
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Network(format!("TaxJar request failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(Error::Network(format!("TaxJar error: {}", error)));
        }

        let _result: TaxJarTaxResponse = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse TaxJar response: {}", e)))?;

        // Convert TaxJar response to our TaxCalculation format
        // TODO: Implement full conversion
        Ok(TaxCalculation::new())
    }

    async fn validate_address(&self, address: &TaxAddress) -> Result<ValidatedAddress> {
        // TaxJar doesn't have a direct address validation API
        // Return basic validation
        Ok(ValidatedAddress {
            country_code: address.country_code.clone(),
            region_code: address.region_code.clone().unwrap_or_default(),
            city: address.city.clone().unwrap_or_default(),
            postal_code: address.postal_code.clone().unwrap_or_default(),
            street: "".to_string(),
            is_valid: true,
            latitude: None,
            longitude: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{}/v2/categories", self.base_url))
            .header("Authorization", self.auth_header())
            .send()
            .await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

/// TaxJar tax request
#[derive(Debug, Serialize)]
struct TaxJarTaxRequest {
    #[serde(rename = "to_country")]
    to_country: String,
    #[serde(rename = "to_zip")]
    to_zip: String,
    #[serde(rename = "to_state")]
    to_state: String,
    #[serde(rename = "to_city")]
    to_city: String,
    amount: Decimal,
    shipping: Decimal,
    #[serde(rename = "line_items")]
    line_items: Vec<TaxJarLineItem>,
}

#[derive(Debug, Serialize)]
struct TaxJarLineItem {
    id: String,
    quantity: i32,
    #[serde(rename = "product_tax_code")]
    product_tax_code: Option<String>,
    #[serde(rename = "unit_price")]
    unit_price: Decimal,
    discount: Decimal,
}

/// TaxJar tax response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TaxJarTaxResponse {
    tax: TaxJarTaxAmounts,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TaxJarTaxAmounts {
    #[serde(rename = "amount_to_collect")]
    amount_to_collect: Decimal,
    #[serde(rename = "taxable_amount")]
    taxable_amount: Decimal,
    rate: Decimal,
}

/// Provider factory
pub struct TaxProviderFactory;

impl TaxProviderFactory {
    /// Create provider from configuration
    pub fn create(config: &crate::config::TaxConfig) -> Result<Box<dyn TaxProvider>> {
        match config.provider.as_str() {
            "avalara" => {
                let api_key = config
                    .avalara
                    .as_ref()
                    .and_then(|a| a.api_key.clone())
                    .ok_or_else(|| Error::config("Avalara API key not configured"))?;
                let account_id = config
                    .avalara
                    .as_ref()
                    .and_then(|a| a.account_id.clone())
                    .ok_or_else(|| Error::config("Avalara account ID not configured"))?;
                let sandbox = config
                    .avalara
                    .as_ref()
                    .map(|a| a.environment == "sandbox")
                    .unwrap_or(true);

                Ok(Box::new(AvalaraProvider::new(api_key, account_id, sandbox)))
            }
            "taxjar" => {
                let api_token = config
                    .taxjar
                    .as_ref()
                    .and_then(|t| t.api_token.clone())
                    .ok_or_else(|| Error::config("TaxJar API token not configured"))?;
                let sandbox = config.taxjar.as_ref().map(|t| t.sandbox).unwrap_or(true);

                Ok(Box::new(TaxJarProvider::new(api_token, sandbox)))
            }
            _ => Err(Error::config(format!(
                "Unknown tax provider: {}",
                config.provider
            ))),
        }
    }
}
