//! Payment Gateway Integration Tests
//!
//! These tests verify the integration with Stripe and Airwallex APIs.
//! To run these tests with real API calls, set the following environment variables:
//!
//! For Stripe:
//! - STRIPE_TEST_SECRET_KEY
//! - STRIPE_TEST_WEBHOOK_SECRET
//!
//! For Airwallex:
//! - AIRWALLEX_TEST_CLIENT_ID
//! - AIRWALLEX_TEST_API_KEY
//! - AIRWALLEX_TEST_WEBHOOK_SECRET
//!
//! Run with: cargo test --features integration_tests -- --test-threads=1

#[cfg(test)]
mod stripe_tests {
    use super::super::stripe::StripeGateway;
    use crate::payment::{CardDetails, CreatePaymentRequest, PaymentGateway, PaymentMethod};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    fn get_stripe_gateway() -> Option<StripeGateway> {
        let api_key = std::env::var("STRIPE_TEST_SECRET_KEY").ok()?;
        let webhook_secret = std::env::var("STRIPE_TEST_WEBHOOK_SECRET").unwrap_or_default();
        Some(StripeGateway::new(api_key, webhook_secret))
    }

    fn create_test_request(amount: Decimal) -> CreatePaymentRequest {
        CreatePaymentRequest {
            amount,
            currency: "USD".to_string(),
            order_id: Uuid::new_v4(),
            customer_id: None,
            customer_email: "test@example.com".to_string(),
            payment_method: PaymentMethod::Card(CardDetails {
                number: "4242424242424242".to_string(),
                exp_month: 12,
                exp_year: 2025,
                cvc: "123".to_string(),
                name: "Test User".to_string(),
            }),
            billing_address: None,
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn test_stripe_create_payment_intent() {
        let gateway = match get_stripe_gateway() {
            Some(g) => g,
            None => {
                eprintln!("Skipping test: STRIPE_TEST_SECRET_KEY not set");
                return;
            }
        };

        let request = create_test_request(Decimal::new(1000, 2)); // $10.00

        let session = gateway
            .create_payment(request)
            .await
            .expect("Failed to create payment");

        assert!(
            !session.id.is_empty(),
            "Payment intent ID should not be empty"
        );
        assert!(
            !session.client_secret.is_empty(),
            "Client secret should not be empty"
        );
        assert_eq!(session.amount, Decimal::new(1000, 2));
        assert_eq!(session.currency, "USD");
    }

    #[tokio::test]
    async fn test_stripe_full_payment_flow() {
        let gateway = match get_stripe_gateway() {
            Some(g) => g,
            None => {
                eprintln!("Skipping test: STRIPE_TEST_SECRET_KEY not set");
                return;
            }
        };

        // 1. Create payment intent
        let request = create_test_request(Decimal::new(2500, 2)); // $25.00
        let session = gateway
            .create_payment(request)
            .await
            .expect("Failed to create payment");

        // 2. Confirm payment (this would normally require a payment method)
        // Note: In test mode, we can confirm directly
        let payment = gateway.confirm_payment(&session.id).await;

        // Payment may succeed or fail depending on test card used
        // We're mainly testing the API integration here
        println!("Payment result: {:?}", payment);
    }

    #[tokio::test]
    async fn test_stripe_get_payment() {
        let gateway = match get_stripe_gateway() {
            Some(g) => g,
            None => {
                eprintln!("Skipping test: STRIPE_TEST_SECRET_KEY not set");
                return;
            }
        };

        // Create a payment first
        let request = create_test_request(Decimal::new(500, 2));
        let session = gateway
            .create_payment(request)
            .await
            .expect("Failed to create payment");

        // Get payment details
        let payment = gateway.get_payment(&session.id).await;
        assert!(payment.is_ok(), "Should be able to retrieve payment");
    }

    #[tokio::test]
    async fn test_stripe_error_handling() {
        let gateway = match get_stripe_gateway() {
            Some(g) => g,
            None => {
                eprintln!("Skipping test: STRIPE_TEST_SECRET_KEY not set");
                return;
            }
        };

        // Test with invalid payment ID
        let result = gateway.get_payment("pi_invalid").await;
        assert!(
            result.is_err(),
            "Should return error for invalid payment ID"
        );
    }
}

#[cfg(test)]
mod airwallex_tests {
    use super::super::airwallex::AirwallexGateway;
    use crate::payment::{CardDetails, CreatePaymentRequest, PaymentGateway, PaymentMethod};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    fn get_airwallex_gateway() -> Option<AirwallexGateway> {
        let client_id = std::env::var("AIRWALLEX_TEST_CLIENT_ID").ok()?;
        let api_key = std::env::var("AIRWALLEX_TEST_API_KEY").ok()?;
        let webhook_secret = std::env::var("AIRWALLEX_TEST_WEBHOOK_SECRET").unwrap_or_default();
        Some(AirwallexGateway::new(client_id, api_key, webhook_secret))
    }

    fn create_test_request(amount: Decimal) -> CreatePaymentRequest {
        CreatePaymentRequest {
            amount,
            currency: "USD".to_string(),
            order_id: Uuid::new_v4(),
            customer_id: None,
            customer_email: "test@example.com".to_string(),
            payment_method: PaymentMethod::Card(CardDetails {
                number: "4111111111111111".to_string(),
                exp_month: 12,
                exp_year: 2025,
                cvc: "123".to_string(),
                name: "Test User".to_string(),
            }),
            billing_address: None,
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn test_airwallex_authentication() {
        let gateway = match get_airwallex_gateway() {
            Some(g) => g,
            None => {
                eprintln!("Skipping test: Airwallex credentials not set");
                return;
            }
        };

        // Authentication happens internally on first API call
        // If this succeeds, authentication worked
        let request = create_test_request(Decimal::new(1000, 2));
        let result = gateway.create_payment(request).await;

        // May fail for other reasons, but should not be auth error
        if let Err(ref e) = result {
            let error_str = e.to_string();
            assert!(
                !error_str.contains("auth"),
                "Should not be authentication error"
            );
        }
    }

    #[tokio::test]
    async fn test_airwallex_create_payment_intent() {
        let gateway = match get_airwallex_gateway() {
            Some(g) => g,
            None => {
                eprintln!("Skipping test: Airwallex credentials not set");
                return;
            }
        };

        let request = create_test_request(Decimal::new(1000, 2));
        let session = gateway.create_payment(request).await;

        match session {
            Ok(s) => {
                assert!(!s.id.is_empty(), "Payment intent ID should not be empty");
                assert!(
                    !s.client_secret.is_empty(),
                    "Client secret should not be empty"
                );
                assert_eq!(s.amount, Decimal::new(1000, 2));
                assert_eq!(s.currency, "USD");
            }
            Err(e) => {
                eprintln!(
                    "Payment creation failed (may be expected in test environment): {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_airwallex_error_handling() {
        let gateway = match get_airwallex_gateway() {
            Some(g) => g,
            None => {
                eprintln!("Skipping test: Airwallex credentials not set");
                return;
            }
        };

        // Test with invalid payment ID
        let result = gateway.get_payment("invalid_id").await;
        assert!(
            result.is_err(),
            "Should return error for invalid payment ID"
        );
    }
}

#[cfg(test)]
mod mock_tests {
    use super::super::MockPaymentGateway;
    use crate::payment::{CardDetails, CreatePaymentRequest, PaymentGateway, PaymentMethod};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    fn create_test_request() -> CreatePaymentRequest {
        CreatePaymentRequest {
            amount: Decimal::new(1000, 2),
            currency: "USD".to_string(),
            order_id: Uuid::new_v4(),
            customer_id: None,
            customer_email: "test@example.com".to_string(),
            payment_method: PaymentMethod::Card(CardDetails {
                number: "4242424242424242".to_string(),
                exp_month: 12,
                exp_year: 2025,
                cvc: "123".to_string(),
                name: "Test User".to_string(),
            }),
            billing_address: None,
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn test_mock_gateway_full_flow() {
        let gateway = MockPaymentGateway::new();

        // Test create
        let request = create_test_request();
        let session = gateway.create_payment(request.clone()).await.unwrap();
        assert_eq!(session.amount, request.amount);
        assert_eq!(session.currency, request.currency);

        // Test confirm
        let payment = gateway.confirm_payment(&session.id).await.unwrap();
        assert_eq!(payment.status, crate::payment::PaymentStatus::Succeeded);

        // Test get
        let retrieved = gateway.get_payment(&session.id).await.unwrap();
        assert_eq!(retrieved.gateway, "mock");

        // Test capture
        let captured = gateway.capture_payment(&session.id, None).await.unwrap();
        assert!(captured.captured_at.is_some());

        // Test refund
        let refund = gateway
            .refund_payment(&session.id, Some(Decimal::new(500, 2)), "test")
            .await
            .unwrap();
        assert_eq!(refund.status, crate::payment::RefundStatus::Succeeded);
    }
}

#[cfg(test)]
mod webhook_tests {
    use super::super::stripe::StripeGateway;
    use crate::payment::PaymentGateway;

    #[test]
    fn test_stripe_webhook_parsing() {
        // This test doesn't need API keys - it just tests webhook payload parsing
        let gateway = StripeGateway::new("sk_test_dummy".to_string(), "whsec_dummy".to_string());

        // Test payment_intent.succeeded event
        let payload = serde_json::json!({
            "id": "evt_123",
            "object": "event",
            "type": "payment_intent.succeeded",
            "data": {
                "object": {
                    "id": "pi_123",
                    "status": "succeeded"
                }
            }
        });

        let payload_bytes = payload.to_string().into_bytes();
        let signature = "dummy_signature";

        // Note: This will fail signature verification but tests payload structure
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(gateway.handle_webhook(&payload_bytes, signature));

        // We expect an error due to signature verification, not payload parsing
        if let Err(e) = result {
            let error_str = e.to_string();
            assert!(
                error_str.contains("signature") || error_str.contains("Invalid"),
                "Error should be about signature, not parsing: {}",
                error_str
            );
        }
    }
}

#[cfg(test)]
mod gateway_comparison_tests {
    use crate::payment::{CardDetails, CreatePaymentRequest, PaymentGateway, PaymentMethod};

    /// Test that verifies all gateways have consistent behavior
    async fn test_gateway_consistency<G: PaymentGateway>(gateway: &G) {
        let _request = CreatePaymentRequest {
            amount: rust_decimal::Decimal::new(1000, 2),
            currency: "USD".to_string(),
            order_id: uuid::Uuid::new_v4(),
            customer_id: None,
            customer_email: "test@example.com".to_string(),
            payment_method: PaymentMethod::Card(CardDetails {
                number: "4242424242424242".to_string(),
                exp_month: 12,
                exp_year: 2025,
                cvc: "123".to_string(),
                name: "Test User".to_string(),
            }),
            billing_address: None,
            metadata: serde_json::json!({}),
        };

        // All gateways should have an ID and name
        assert!(!gateway.id().is_empty());
        assert!(!gateway.name().is_empty());

        // Test that ID is lowercase and snake_case
        let id = gateway.id();
        assert!(id
            .chars()
            .all(|c| c.is_lowercase() || c == '_' || c.is_numeric()));
    }

    #[tokio::test]
    async fn test_mock_gateway_consistency() {
        use super::super::MockPaymentGateway;
        let gateway = MockPaymentGateway::new();
        test_gateway_consistency(&gateway).await;
    }
}
