use serde::{Deserialize, Serialize};

/// Address models - re-exported from common module
pub use crate::common::Address;

/// Address type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, Default)]
#[sqlx(type_name = "address_type", rename_all = "snake_case")]
pub enum AddressType {
    #[default]
    Shipping,
    Billing,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_address_creation() {
        let addr = Address {
            id: Uuid::new_v4(),
            customer_id: Uuid::new_v4(),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            company: None,
            phone: None,
            address1: "123 Main St".to_string(),
            address2: None,
            city: "Anytown".to_string(),
            state: Some("CA".to_string()),
            zip: "12345".to_string(),
            country: "US".to_string(),
            is_default_shipping: true,
            is_default_billing: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(addr.first_name, "John");
        assert!(addr.is_default_shipping);
    }
}
