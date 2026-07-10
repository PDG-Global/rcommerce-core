use chrono::{DateTime, Utc};

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

/// Customer role for access control
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "customer_role", rename_all = "snake_case")]
pub enum CustomerRole {
    #[default]
    Customer,
    Manager,
    Admin,
}

impl CustomerRole {
    /// Get permissions for this role
    pub fn permissions(&self) -> Vec<String> {
        match self {
            CustomerRole::Customer => vec!["read".to_string()],
            CustomerRole::Manager => vec!["read".to_string(), "write".to_string()],
            CustomerRole::Admin => {
                vec!["read".to_string(), "write".to_string(), "admin".to_string()]
            }
        }
    }
}

/// Customer entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Customer {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<String>,
    pub accepts_marketing: bool,
    pub tax_exempt: bool,
    pub currency: crate::models::Currency,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub timezone: Option<String>,
    pub marketing_opt_in: bool,
    pub email_notifications: bool,
    pub sms_notifications: bool,
    pub push_notifications: bool,
    // Authentication fields (not serialized to JSON)
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub is_verified: bool,
    pub is_imported: bool,
    pub email_verified: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing)]
    pub role: CustomerRole,
}

/// Create customer request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCustomerRequest {
    #[validate(email)]
    pub email: String,

    #[validate(length(min = 1, max = 100))]
    pub first_name: String,

    #[validate(length(min = 1, max = 100))]
    pub last_name: String,

    pub phone: Option<String>,
    pub accepts_marketing: bool,
    pub currency: crate::models::Currency,
}

/// Update customer request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateCustomerRequest {
    #[validate(email)]
    pub email: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub first_name: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub last_name: Option<String>,

    pub phone: Option<String>,
    pub accepts_marketing: Option<bool>,
    pub tax_exempt: Option<bool>,
}

/// Create address request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateAddressRequest {
    #[validate(length(min = 1, max = 100))]
    pub first_name: String,

    #[validate(length(min = 1, max = 100))]
    pub last_name: String,

    pub company: Option<String>,

    #[validate(length(min = 1, max = 20))]
    pub phone: Option<String>,

    #[validate(length(min = 1, max = 255))]
    pub address1: String,

    #[validate(length(max = 255))]
    pub address2: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub city: String,

    #[validate(length(max = 100))]
    pub state: Option<String>,

    #[validate(length(min = 2, max = 2))]
    pub country: String,

    #[validate(length(min = 1, max = 20))]
    pub zip: String,

    pub is_default_shipping: bool,
    pub is_default_billing: bool,
}
