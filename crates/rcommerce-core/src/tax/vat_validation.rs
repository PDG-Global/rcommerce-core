//! VAT ID Validation
//!
//! Validates VAT IDs using the EU VIES (VAT Information Exchange System) service.
//! Also supports UK VAT ID validation post-Brexit.

use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::{Error, Result};

/// VAT ID structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VatId {
    /// Country code (ISO 3166-1 alpha-2)
    pub country_code: String,
    /// VAT number without country prefix
    pub number: String,
    /// Whether the VAT ID has been validated
    pub is_validated: bool,
    /// When validation was performed
    pub validated_at: Option<DateTime<Utc>>,
    /// Validation result
    pub is_valid: Option<bool>,
    /// Business name from validation
    pub business_name: Option<String>,
    /// Business address from validation
    pub business_address: Option<String>,
}

impl VatId {
    /// Create a new VAT ID from a string
    pub fn parse(vat_id: &str) -> Result<Self> {
        let normalized = vat_id.replace([' ', '.', '-'], "").to_uppercase();

        if normalized.len() < 3 {
            return Err(Error::validation("VAT ID too short"));
        }

        let country_code = &normalized[..2];
        let number = &normalized[2..];

        // Validate country code
        if !is_valid_vat_country(country_code) {
            return Err(Error::validation(format!(
                "Invalid VAT country code: {}",
                country_code
            )));
        }

        // Validate format
        if !validate_vat_format(country_code, number) {
            return Err(Error::validation(format!(
                "Invalid VAT number format for {}",
                country_code
            )));
        }

        Ok(Self {
            country_code: country_code.to_string(),
            number: number.to_string(),
            is_validated: false,
            validated_at: None,
            is_valid: None,
            business_name: None,
            business_address: None,
        })
    }

    /// Get full VAT ID string
    pub fn full_id(&self) -> String {
        format!("{}{}", self.country_code, self.number)
    }

    /// Check if validation is expired (default 30 days)
    pub fn is_expired(&self, max_age_days: i64) -> bool {
        if let Some(validated_at) = self.validated_at {
            Utc::now() - validated_at > Duration::days(max_age_days)
        } else {
            true
        }
    }
}

/// VAT validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VatValidationResult {
    /// Whether the VAT ID is valid
    pub is_valid: bool,
    /// Country code
    pub country_code: String,
    /// VAT number
    pub vat_number: String,
    /// Business name (if available)
    pub business_name: Option<String>,
    /// Business address (if available)
    pub business_address: Option<String>,
    /// Validation timestamp
    pub validated_at: DateTime<Utc>,
    /// Error message (if validation failed)
    pub error_message: Option<String>,
}

/// VIES VAT validation service
pub struct ViesValidator {
    client: reqwest::Client,
    base_url: String,
}

impl ViesValidator {
    /// Create a new VIES validator
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            base_url: "https://ec.europa.eu/taxation_customs/vies/services/checkVatService"
                .to_string(),
        }
    }

    /// Create with custom client
    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            client,
            base_url: "https://ec.europa.eu/taxation_customs/vies/services/checkVatService"
                .to_string(),
        }
    }

    /// Validate a VAT ID using VIES
    pub async fn validate(&self, vat_id: &VatId) -> Result<VatValidationResult> {
        info!("Validating VAT ID: {}", vat_id.full_id());

        // Check if it's an EU country
        if !is_eu_country_for_vat(&vat_id.country_code) {
            return Err(Error::validation(format!(
                "Country {} not supported by VIES",
                vat_id.country_code
            )));
        }

        // Build SOAP request
        let soap_request = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/" xmlns:urn="urn:ec.europa.eu:taxud:vies:services:checkVat:types">
   <soapenv:Header/>
   <soapenv:Body>
      <urn:checkVat>
         <urn:countryCode>{}</urn:countryCode>
         <urn:vatNumber>{}</urn:vatNumber>
      </urn:checkVat>
   </soapenv:Body>
</soapenv:Envelope>"#,
            vat_id.country_code, vat_id.number
        );

        // Send request
        let response = match self
            .client
            .post(&self.base_url)
            .header("Content-Type", "text/xml; charset=utf-8")
            .header("SOAPAction", "")
            .body(soap_request)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                error!("VIES request failed: {}", e);
                return Err(Error::Network(format!("VIES service unavailable: {}", e)));
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("VIES returned error {}: {}", status, body);
            return Err(Error::Network(format!("VIES service error: {}", status)));
        }

        let body = response
            .text()
            .await
            .map_err(|e| Error::Network(format!("Failed to read VIES response: {}", e)))?;

        // Parse response
        self.parse_vies_response(&body, vat_id)
    }

    /// Parse VIES SOAP response
    fn parse_vies_response(&self, body: &str, vat_id: &VatId) -> Result<VatValidationResult> {
        // Extract valid flag
        let valid_regex = Regex::new(r"<valid>(true|false)</valid>").unwrap();
        let is_valid = valid_regex
            .captures(body)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str() == "true")
            .ok_or_else(|| {
                Error::Network("Invalid VIES response: missing valid flag".to_string())
            })?;

        // Extract business name
        let name_regex = Regex::new(r"<name>([^<]*)</name>").unwrap();
        let business_name = name_regex
            .captures(body)
            .and_then(|c| c.get(1))
            .and_then(|m| {
                let name = m.as_str();
                if name == "---" {
                    None
                } else {
                    Some(name.to_string())
                }
            });

        // Extract address
        let address_regex = Regex::new(r"<address>([^<]*)</address>").unwrap();
        let business_address = address_regex
            .captures(body)
            .and_then(|c| c.get(1))
            .and_then(|m| {
                let addr = m.as_str();
                if addr == "---" {
                    None
                } else {
                    Some(addr.replace("\n", ", "))
                }
            });

        info!(
            "VAT ID {} validation result: valid={}",
            vat_id.full_id(),
            is_valid
        );

        Ok(VatValidationResult {
            is_valid,
            country_code: vat_id.country_code.clone(),
            vat_number: vat_id.number.clone(),
            business_name,
            business_address,
            validated_at: Utc::now(),
            error_message: None,
        })
    }

    /// Check if VIES service is available
    pub async fn check_service_status(&self) -> Result<bool> {
        // Use a known valid VAT ID for testing
        let test_vat = VatId::parse("DE123456789")?;

        match self.validate(&test_vat).await {
            Ok(_) => Ok(true),
            Err(e) => {
                warn!("VIES service check failed: {}", e);
                Ok(false)
            }
        }
    }
}

impl Default for ViesValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if country code is valid for VAT
fn is_valid_vat_country(country_code: &str) -> bool {
    let valid_countries = [
        // EU countries
        "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT",
        "LV", "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
        // Non-EU with VAT
        "GB", "XI", // UK and Northern Ireland
        "CH", // Switzerland
        "NO", // Norway
    ];

    valid_countries.contains(&country_code.to_uppercase().as_str())
}

/// Check if country is in EU (for VIES)
fn is_eu_country_for_vat(country_code: &str) -> bool {
    let eu_countries = [
        "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT",
        "LV", "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
    ];

    eu_countries.contains(&country_code.to_uppercase().as_str())
}

/// Validate VAT number format for a country
fn validate_vat_format(country_code: &str, number: &str) -> bool {
    let patterns: std::collections::HashMap<&str, &str> = [
        ("AT", r"^U\d{8}$"),                                    // Austria: U + 8 digits
        ("BE", r"^\d{10}$|^\d{9}$"),                            // Belgium: 9 or 10 digits
        ("BG", r"^\d{9,10}$"),                                  // Bulgaria: 9 or 10 digits
        ("HR", r"^\d{11}$"),                                    // Croatia: 11 digits
        ("CY", r"^\d{8}[A-Z]$"),                                // Cyprus: 8 digits + letter
        ("CZ", r"^\d{8,10}$"),                                  // Czech Republic: 8-10 digits
        ("DK", r"^\d{8}$"),                                     // Denmark: 8 digits
        ("EE", r"^\d{9}$"),                                     // Estonia: 9 digits
        ("FI", r"^\d{8}$"),                                     // Finland: 8 digits
        ("FR", r"^[A-Z0-9]{2}\d{9}$"),                          // France: 2 chars + 9 digits
        ("DE", r"^\d{9}$"),                                     // Germany: 9 digits
        ("GR", r"^\d{9}$"),                                     // Greece: 9 digits
        ("HU", r"^\d{8}$"),                                     // Hungary: 8 digits
        ("IE", r"^\d{7}[A-Z]{1,2}$|^\d{}[A-Z]\d{5}[A-Z]$"),     // Ireland
        ("IT", r"^\d{11}$"),                                    // Italy: 11 digits
        ("LV", r"^\d{11}$"),                                    // Latvia: 11 digits
        ("LT", r"^\d{9}$|^\d{12}$"),                            // Lithuania
        ("LU", r"^\d{8}$"),                                     // Luxembourg: 8 digits
        ("MT", r"^\d{8}$"),                                     // Malta: 8 digits
        ("NL", r"^\d{9}B\d{2}$"),                               // Netherlands
        ("PL", r"^\d{10}$"),                                    // Poland: 10 digits
        ("PT", r"^\d{9}$"),                                     // Portugal: 9 digits
        ("RO", r"^\d{2,10}$"),                                  // Romania: 2-10 digits
        ("SK", r"^\d{10}$"),                                    // Slovakia: 10 digits
        ("SI", r"^\d{8}$"),                                     // Slovenia: 8 digits
        ("ES", r"^[A-Z]\d{7}[A-Z]$|^\d{8}[A-Z]$|^[A-Z]\d{8}$"), // Spain
        ("SE", r"^\d{12}$"),                                    // Sweden: 12 digits
        ("GB", r"^\d{9}$|^\d{12}$|^(GD|HA)\d{3}$"),             // UK
        ("XI", r"^\d{9}$|^\d{12}$"),                            // Northern Ireland
    ]
    .into();

    if let Some(pattern) = patterns.get(country_code.to_uppercase().as_str()) {
        let regex = Regex::new(pattern).unwrap();
        regex.is_match(number)
    } else {
        // Unknown country - accept any format (will fail at validation)
        true
    }
}

/// UK VAT validator (post-Brexit)
pub struct UkVatValidator {
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl UkVatValidator {
    /// Create new UK VAT validator
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Validate UK VAT ID
    /// Note: UK VAT validation requires HMRC API credentials
    pub async fn validate(&self, vat_id: &VatId) -> Result<VatValidationResult> {
        if vat_id.country_code != "GB" && vat_id.country_code != "XI" {
            return Err(Error::validation("Not a UK VAT ID"));
        }

        // UK VAT validation requires HMRC API access
        // This is a placeholder - actual implementation requires OAuth2
        warn!("UK VAT validation not implemented - requires HMRC API credentials");

        Ok(VatValidationResult {
            is_valid: false,
            country_code: vat_id.country_code.clone(),
            vat_number: vat_id.number.clone(),
            business_name: None,
            business_address: None,
            validated_at: Utc::now(),
            error_message: Some("UK VAT validation requires HMRC API credentials".to_string()),
        })
    }
}

impl Default for UkVatValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vat_id_parse() {
        let vat = VatId::parse("DE123456789").unwrap();
        assert_eq!(vat.country_code, "DE");
        assert_eq!(vat.number, "123456789");
        assert_eq!(vat.full_id(), "DE123456789");
    }

    #[test]
    fn test_vat_id_parse_with_spaces() {
        let vat = VatId::parse("DE 123.456.789").unwrap();
        assert_eq!(vat.country_code, "DE");
        assert_eq!(vat.number, "123456789");
    }

    #[test]
    fn test_vat_id_parse_invalid() {
        assert!(VatId::parse("XX123456789").is_err());
        assert!(VatId::parse("D").is_err());
    }

    #[test]
    fn test_validate_vat_format() {
        // Germany - 9 digits
        assert!(validate_vat_format("DE", "123456789"));
        assert!(!validate_vat_format("DE", "12345678")); // Too short
        assert!(!validate_vat_format("DE", "1234567890")); // Too long

        // France - 2 chars + 9 digits
        assert!(validate_vat_format("FR", "AB123456789"));
        assert!(!validate_vat_format("FR", "123456789"));

        // UK - 9 or 12 digits
        assert!(validate_vat_format("GB", "123456789"));
        assert!(validate_vat_format("GB", "123456789012"));
    }

    #[test]
    fn test_is_valid_vat_country() {
        assert!(is_valid_vat_country("DE"));
        assert!(is_valid_vat_country("GB"));
        assert!(!is_valid_vat_country("US"));
    }

    #[test]
    fn test_vat_id_expired() {
        let mut vat = VatId::parse("DE123456789").unwrap();
        assert!(vat.is_expired(30)); // Not validated yet

        vat.validated_at = Some(Utc::now() - Duration::days(31));
        vat.is_validated = true;
        assert!(vat.is_expired(30));

        vat.validated_at = Some(Utc::now() - Duration::days(15));
        assert!(!vat.is_expired(30));
    }
}
