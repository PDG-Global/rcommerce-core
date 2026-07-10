use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Common trait implementations
pub use crate::traits::*;

/// Common types
pub type CountryCode = String;
pub type LanguageCode = String;

/// Address structure used throughout the system
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Address {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub company: Option<String>,
    pub phone: Option<String>,
    pub address1: String,
    pub address2: Option<String>,
    pub city: String,
    pub state: Option<String>,
    pub country: String,
    pub zip: String,
    pub is_default_shipping: bool,
    pub is_default_billing: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Payment method types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "payment_method_type", rename_all = "snake_case")]
pub enum PaymentMethodType {
    CreditCard,
    DebitCard,
    BankTransfer,
    CashOnDelivery,
    DigitalWallet,
    Cryptocurrency,
}

/// Payment gateway types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "payment_gateway_type", rename_all = "snake_case")]
pub enum PaymentGatewayType {
    Stripe,
    Airwallex,
    Paypal,
    Square,
}

/// Shipping provider types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "shipping_provider_type", rename_all = "snake_case")]
pub enum ShippingProviderType {
    Shipstation,
    Dianxiaomi,
    Ups,
    Fedex,
    Dhl,
}

/// Currency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyConfig {
    pub code: String,
    pub name: String,
    pub symbol: String,
    pub decimal_places: u8,
    pub exchange_rate: Decimal,
}

impl Default for CurrencyConfig {
    fn default() -> Self {
        Self {
            code: "USD".to_string(),
            name: "US Dollar".to_string(),
            symbol: "$".to_string(),
            decimal_places: 2,
            exchange_rate: Decimal::new(1, 0),
        }
    }
}

/// Pricing information for products
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pricing {
    pub price: Decimal,
    pub compare_at_price: Option<Decimal>,
    pub cost_price: Option<Decimal>,
    pub currency: String,
}

/// Inventory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryInfo {
    pub available: i32,
    pub reserved: i32,
    pub incoming: i32,
}

/// Weight and dimensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalProperties {
    pub weight: Decimal,
    pub weight_unit: WeightUnit,
    pub dimensions: Option<Dimensions>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "weight_unit", rename_all = "snake_case")]
pub enum WeightUnit {
    G,
    Kg,
    Oz,
    Lb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimensions {
    pub length: Decimal,
    pub width: Decimal,
    pub height: Decimal,
    pub unit: LengthUnit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "length_unit", rename_all = "snake_case")]
pub enum LengthUnit {
    Cm,
    M,
    In,
    Ft,
}

/// SEO metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeoMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<String>,
    pub tags: Vec<String>,
}

/// Audit trail entry
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuditLog {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub user_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub changes: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl AuditLog {
    pub fn new(
        entity_type: impl Into<String>,
        entity_id: Uuid,
        action: impl Into<String>,
        user_id: Option<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            entity_type: entity_type.into(),
            entity_id,
            action: action.into(),
            user_id,
            ip_address: None,
            user_agent: None,
            changes: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    pub fn with_user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = Some(agent.into());
        self
    }

    pub fn with_changes(mut self, changes: serde_json::Value) -> Self {
        self.changes = Some(changes);
        self
    }
}

/// ApiKey for authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub key_prefix: String,
    pub user_id: Uuid,
    pub name: String,
    pub permissions: Vec<Permission>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Permission enum
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    ReadProducts,
    WriteProducts,
    DeleteProducts,
    ReadOrders,
    WriteOrders,
    DeleteOrders,
    ReadCustomers,
    WriteCustomers,
    DeleteCustomers,
    ReadSettings,
    WriteSettings,
}

/// Staged changes for a product or order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedChanges<T> {
    pub target_id: Uuid,
    pub changes: T,
    pub metadata: StagedMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedMetadata {
    pub action_type: String,
    pub user_id: Uuid,
    pub reason: Option<String>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub notes: Option<String>,
}

/// Validation helpers
pub mod validation {

    /// Validate email format
    pub fn validate_email(email: &str) -> bool {
        email.contains('@') && email.len() >= 3 && email.len() <= 254
    }

    /// Validate phone number
    pub fn validate_phone(phone: &str) -> bool {
        let cleaned = phone.replace(|c: char| !c.is_ascii_digit(), "");
        !cleaned.is_empty() && cleaned.len() >= 10
    }

    /// Validate currency code
    pub fn validate_currency(code: &str) -> bool {
        code.len() == 3 && code.chars().all(|c| c.is_ascii_uppercase())
    }

    /// Validate URL
    pub fn validate_url(url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }

    /// Format phone number
    pub fn format_phone(phone: &str) -> String {
        let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();

        match digits.len() {
            10 => format!("{}-{}-{}", &digits[0..3], &digits[3..6], &digits[6..10]),
            11 if digits.starts_with('1') => format!(
                "+{} {}-{}-{}",
                &digits[0..1],
                &digits[1..4],
                &digits[4..7],
                &digits[7..11]
            ),
            _ => phone.to_string(),
        }
    }
}

// Testing utilities
#[cfg(test)]
pub mod test_utils {
    use super::*;
    use uuid::Uuid;

    /// Create a test address
    pub fn create_test_address(customer_id: Uuid) -> Address {
        Address {
            id: Uuid::new_v4(),
            customer_id,
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            company: None,
            phone: Some("123-456-7890".to_string()),
            address1: "123 Test St".to_string(),
            address2: None,
            city: "Test City".to_string(),
            state: Some("TX".to_string()),
            country: "US".to_string(),
            zip: "12345".to_string(),
            is_default_shipping: true,
            is_default_billing: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Create test pricing
    pub fn create_test_pricing() -> Pricing {
        Pricing {
            price: Decimal::new(2999, 2),                  // $29.99
            compare_at_price: Some(Decimal::new(3999, 2)), // $39.99
            cost_price: Some(Decimal::new(1500, 2)),       // $15.00
            currency: "USD".to_string(),
        }
    }
}

/// Regular expressions for validation
pub mod regex {
    use lazy_static::lazy_static;
    use regex::Regex;

    lazy_static! {
        /// Phone number pattern (supports international)
        pub static ref PHONE_PATTERN: Regex = Regex::new(
            r"^(\+?1[-. ]?)?\(?[0-9]{3}\)?[-. ]?[0-9]{3}[-. ]?[0-9]{4}$"
        ).unwrap();

        /// Currency code pattern (3 uppercase letters)
        pub static ref CURRENCY_PATTERN: Regex = Regex::new(r"^[A-Z]{3}$").unwrap();

        /// UUID pattern
        pub static ref UUID_PATTERN: Regex = Regex::new(
            r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
        ).unwrap();

        /// Email pattern (basic validation)
        pub static ref EMAIL_PATTERN: Regex = Regex::new(
            r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
        ).unwrap();

        /// URL pattern (http/https)
        pub static ref URL_PATTERN: Regex = Regex::new(
            r"^https?://[^\s/$.?#].[^\s]*$"
        ).unwrap();

        /// Alphanumeric pattern (for SKU/code validation)
        pub static ref ALPHANUMERIC_PATTERN: Regex = Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    }
}

// Use FromRow from sqlx
use sqlx::FromRow;
