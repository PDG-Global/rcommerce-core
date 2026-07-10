//! Carrier implementations for major shipping providers

pub mod dhl;
pub mod fedex;
pub mod manual;
pub mod ups;
pub mod usps;

pub use dhl::DhlProvider;
pub use fedex::FedExProvider;
pub use manual::ManualProvider;
pub use ups::UpsProvider;
pub use usps::UspsProvider;

use crate::common::Address;
use crate::Result;

/// Detect carrier from tracking number
pub fn detect_carrier_from_tracking(tracking: &str) -> Option<&'static str> {
    let tracking = tracking.trim().to_uppercase();

    // UPS
    // UPS tracking numbers are typically 18 characters starting with 1Z
    if tracking.starts_with("1Z") && tracking.len() == 18 {
        return Some("ups");
    }

    // FedEx
    // FedEx tracking numbers are typically 12, 15, 20, or 34 digits
    if (tracking.len() == 12
        || tracking.len() == 15
        || tracking.len() == 20
        || tracking.len() == 34)
        && tracking.chars().all(|c| c.is_ascii_digit())
    {
        return Some("fedex");
    }

    // USPS
    // USPS tracking numbers are typically 20-22 digits, often starting with 9
    if (tracking.len() == 20 || tracking.len() == 22)
        && tracking.starts_with('9')
        && tracking.chars().all(|c| c.is_ascii_digit())
    {
        return Some("usps");
    }

    // DHL
    // DHL tracking numbers are typically 10 or 11 digits
    if (tracking.len() == 10 || tracking.len() == 11)
        && tracking.chars().all(|c| c.is_ascii_digit())
    {
        return Some("dhl");
    }

    // DHL Express (starts with specific prefixes)
    if tracking.len() == 12 {
        let prefixes = ["000", "JJD", "JJD00", "JJD01", "JVGL"];
        for prefix in &prefixes {
            if tracking.starts_with(prefix) {
                return Some("dhl");
            }
        }
    }

    None
}

/// Get tracking URL for carrier
pub fn get_tracking_url(carrier: &str, tracking_number: &str) -> Option<String> {
    match carrier.to_lowercase().as_str() {
        "ups" => Some(format!(
            "https://www.ups.com/track?tracknum={}",
            tracking_number
        )),
        "fedex" => Some(format!(
            "https://www.fedex.com/apps/fedextrack/?tracknumbers={}",
            tracking_number
        )),
        "usps" => Some(format!(
            "https://tools.usps.com/go/TrackConfirmAction?qtc_tLabels1={}",
            tracking_number
        )),
        "dhl" => Some(format!(
            "https://www.dhl.com/en/express/tracking.html?AWB={}",
            tracking_number
        )),
        _ => None,
    }
}

/// Normalize tracking number (remove spaces, dashes, etc.)
pub fn normalize_tracking_number(tracking: &str) -> String {
    tracking.to_uppercase().replace([' ', '-', '_'], "")
}

/// Validate tracking number format
pub fn validate_tracking_number(carrier: &str, tracking: &str) -> bool {
    let normalized = normalize_tracking_number(tracking);

    match carrier.to_lowercase().as_str() {
        "ups" => {
            // UPS: 18 chars starting with 1Z
            normalized.len() == 18 && normalized.starts_with("1Z")
        }
        "fedex" => {
            // FedEx: 12, 15, 20, or 34 digits
            let valid_lengths = [12, 15, 20, 34];
            valid_lengths.contains(&normalized.len())
                && normalized.chars().all(|c| c.is_ascii_digit())
        }
        "usps" => {
            // USPS: 20-22 digits, often starts with 9
            (20..=22).contains(&normalized.len()) && normalized.chars().all(|c| c.is_ascii_digit())
        }
        "dhl" => {
            // DHL: 10-11 digits, or specific formats
            if normalized.len() == 10 || normalized.len() == 11 {
                normalized.chars().all(|c| c.is_ascii_digit())
            } else {
                normalized.len() == 12
            }
        }
        _ => false,
    }
}

/// Address normalization for carriers
pub fn normalize_address_for_carrier(address: &Address, carrier: &str) -> Result<Address> {
    match carrier.to_lowercase().as_str() {
        "ups" | "fedex" | "dhl" | "usps" => {
            // Standard normalization
            let mut normalized = address.clone();

            // Ensure country code is uppercase
            normalized.country = normalized.country.to_uppercase();

            // Normalize state/province
            if let Some(ref mut state) = normalized.state {
                *state = state.to_uppercase();
            }

            // Normalize postal code (remove spaces for US)
            if normalized.country == "US" {
                normalized.zip = normalized.zip.replace([' ', '-'], "");
            }

            // Ensure phone is in E.164 format if present
            // This is a simplified version

            Ok(normalized)
        }
        _ => Ok(address.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_carrier() {
        // UPS
        assert_eq!(
            detect_carrier_from_tracking("1Z999AA10123456784"),
            Some("ups")
        );

        // FedEx
        assert_eq!(detect_carrier_from_tracking("123456789012"), Some("fedex"));

        // USPS
        assert_eq!(
            detect_carrier_from_tracking("9400100000000000000000"),
            Some("usps")
        );

        // DHL
        assert_eq!(detect_carrier_from_tracking("1234567890"), Some("dhl"));
    }

    #[test]
    fn test_tracking_url() {
        assert!(get_tracking_url("ups", "1Z123").is_some());
        assert!(get_tracking_url("fedex", "123").is_some());
        assert!(get_tracking_url("usps", "123").is_some());
        assert!(get_tracking_url("dhl", "123").is_some());
        assert!(get_tracking_url("unknown", "123").is_none());
    }

    #[test]
    fn test_normalize_tracking() {
        assert_eq!(normalize_tracking_number("1z 999-aa10"), "1Z999AA10");
    }

    #[test]
    fn test_validate_tracking() {
        assert!(validate_tracking_number("ups", "1Z999AA10123456784"));
        assert!(!validate_tracking_number("ups", "invalid"));

        assert!(validate_tracking_number("fedex", "123456789012"));
        assert!(!validate_tracking_number("fedex", "123"));
    }
}
