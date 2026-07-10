impl crate::payment::PaymentStatus {
    pub fn can_transition_to(&self, new_status: crate::payment::PaymentStatus) -> bool {
        use crate::payment::PaymentStatus::*;

        match (self, new_status) {
            (Pending, Processing) => true,
            (Processing, Succeeded) => true,
            (Processing, Failed) => true,
            (Succeeded, Refunded) => true,
            (Failed, Processing) => true, // Retry
            _ => false,
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::payment::gateways::MockPaymentGateway;
    use crate::payment::PaymentGateway;
    use rust_decimal_macros::dec;

    #[tokio::test]
    async fn test_mock_payment_gateway_creation() {
        let gateway = MockPaymentGateway::new();

        assert_eq!(gateway.id(), "mock");
        assert_eq!(gateway.name(), "Mock Payment Gateway");
    }

    #[test]
    fn test_create_payment_request_validation() {
        use crate::models::Address;
        use crate::payment::{CardDetails, CreatePaymentRequest, PaymentMethod};

        let request = CreatePaymentRequest {
            amount: dec!(99.99),
            currency: "USD".to_string(),
            order_id: uuid::Uuid::new_v4(),
            customer_id: Some(uuid::Uuid::new_v4()),
            customer_email: "customer@example.com".to_string(),
            payment_method: PaymentMethod::Card(CardDetails {
                number: "4242424242424242".to_string(),
                exp_month: 12,
                exp_year: 2025,
                cvc: "123".to_string(),
                name: "John Doe".to_string(),
            }),
            billing_address: Some(Address {
                id: uuid::Uuid::new_v4(),
                customer_id: uuid::Uuid::new_v4(),
                first_name: "John".to_string(),
                last_name: "Doe".to_string(),
                company: None,
                phone: None,
                address1: "123 Main St".to_string(),
                address2: None,
                city: "San Francisco".to_string(),
                state: Some("CA".to_string()),
                country: "US".to_string(),
                zip: "94105".to_string(),
                is_default_shipping: false,
                is_default_billing: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }),
            metadata: serde_json::json!({"test": true}),
        };

        assert_eq!(request.amount, dec!(99.99));
        assert_eq!(request.currency, "USD");
        assert!(request.customer_id.is_some());
    }

    #[test]
    fn test_payment_status_transitions() {
        use crate::payment::PaymentStatus::*;

        // Test valid transitions
        assert!(Pending.can_transition_to(Processing));
        assert!(Processing.can_transition_to(Succeeded));
        assert!(Processing.can_transition_to(Failed));
        assert!(Succeeded.can_transition_to(Refunded));

        // Test invalid transitions
        assert!(!Succeeded.can_transition_to(Processing));
        assert!(!Failed.can_transition_to(Succeeded));
    }
}
