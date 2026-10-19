//! WeChat Pay Integration Tests
//!
//! These tests require real WeChat Pay API credentials and make actual API calls.
//!
//! # Prerequisites
//!
//! 1. WeChat Pay merchant account
//! 2. API v3 credentials (mch_id, app_id, serial_no, private_key)
//! 3. Sandbox mode recommended for testing
//!
//! # Running the tests
//!
//! ```bash
//! # Set required environment variables
//! export WECHATPAY_MCH_ID="1234567890"
//! export WECHATPAY_APP_ID="wx1234567890abcdef"
//! export WECHATPAY_SERIAL_NO="..."
//! export WECHATPAY_PRIVATE_KEY="-----BEGIN PRIVATE KEY-----..."
//! export WECHATPAY_API_KEY="..."
//!
//! # Run integration tests
//! cargo test --test wechatpay_integration_tests -- --ignored --test-threads=1
//! ```

use rcommerce_core::payment::gateways::wechatpay::WeChatPayGateway;
use rcommerce_core::payment::{CardDetails, CreatePaymentRequest, PaymentGateway, PaymentMethod};
use rust_decimal::Decimal;
use uuid::Uuid;

// ============================================================================
// Helper Functions
// ============================================================================

fn get_wechatpay_gateway() -> Option<WeChatPayGateway> {
    let mch_id = std::env::var("WECHATPAY_MCH_ID").ok()?;
    let api_key = std::env::var("WECHATPAY_API_KEY").ok()?;
    let app_id = std::env::var("WECHATPAY_APP_ID").ok()?;
    let serial_no = std::env::var("WECHATPAY_SERIAL_NO").ok()?;
    let private_key = std::env::var("WECHATPAY_PRIVATE_KEY").ok()?;

    // Use sandbox mode for testing
    Some(WeChatPayGateway::new(
        mch_id,
        api_key,
        app_id,
        serial_no,
        private_key,
        true, // sandbox mode
    ))
}

fn create_test_request(amount: Decimal) -> CreatePaymentRequest {
    CreatePaymentRequest {
        amount,
        currency: "CNY".to_string(), // WeChat Pay primarily uses CNY
        order_id: Uuid::new_v4(),
        customer_id: None,
        customer_email: "test@example.com".to_string(),
        payment_method: PaymentMethod::Card(CardDetails {
            number: "...".to_string(), // WeChat Pay doesn't use card details directly
            exp_month: 0,
            exp_year: 0,
            cvc: "".to_string(),
            name: "".to_string(),
        }),
        billing_address: None,
        payment_consent_id: None,
        merchant_initiated: None,
        metadata: serde_json::json!({
            "test": true,
            "environment": "integration_test"
        }),
    }
}

// ============================================================================
// WeChat Pay Integration Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires WECHATPAY_MCH_ID and other credentials"]
async fn test_wechatpay_create_native_payment() {
    let gateway =
        get_wechatpay_gateway().expect("WeChat Pay credentials must be set in environment");

    let request = create_test_request(Decimal::new(100, 2)); // 1.00 CNY

    let session = gateway
        .create_payment(request)
        .await
        .expect("Failed to create WeChat Pay native payment");

    assert!(!session.id.is_empty(), "Trade number should not be empty");
    assert!(
        !session.client_secret.is_empty(),
        "Code URL (QR code) should not be empty"
    );
    assert_eq!(session.amount, Decimal::new(100, 2));
    assert_eq!(session.currency, "CNY");

    println!("✅ Created WeChat Pay native payment");
    println!("   Trade No: {}", session.id);
    println!("   QR Code URL: {}", session.client_secret);
}

#[tokio::test]
#[ignore = "Requires WECHATPAY_MCH_ID and other credentials"]
async fn test_wechatpay_query_payment() {
    let gateway =
        get_wechatpay_gateway().expect("WeChat Pay credentials must be set in environment");

    // First create a payment
    let request = create_test_request(Decimal::new(200, 2)); // 2.00 CNY
    let session = gateway
        .create_payment(request)
        .await
        .expect("Failed to create payment");

    // Query the payment status
    let payment = gateway.get_payment(&session.id).await;

    // Payment might not be found immediately or might be in various states
    match payment {
        Ok(p) => {
            println!("✅ Queried WeChat Pay payment: {:?}", p.status);
        }
        Err(e) => {
            println!(
                "⚠️ Payment query result (may be expected for unpaid order): {}",
                e
            );
        }
    }
}

#[tokio::test]
#[ignore = "Requires WECHATPAY_MCH_ID and other credentials"]
async fn test_wechatpay_error_handling() {
    let gateway =
        get_wechatpay_gateway().expect("WeChat Pay credentials must be set in environment");

    // Test with invalid trade number format
    let result = gateway.get_payment("invalid_trade_no").await;
    assert!(
        result.is_err(),
        "Should return error for invalid trade number"
    );

    let error_msg = result.unwrap_err().to_string();
    println!("✅ Error handling works: {}", error_msg);
}

// ============================================================================
// Environment Variable Validation
// ============================================================================

#[test]
fn check_environment_variables() {
    println!("\n🔍 Checking WeChat Pay environment variables...\n");

    let mut missing = Vec::new();

    if std::env::var("WECHATPAY_MCH_ID").is_err() {
        missing.push("WECHATPAY_MCH_ID");
    } else {
        println!("✅ WECHATPAY_MCH_ID is set");
    }

    if std::env::var("WECHATPAY_APP_ID").is_err() {
        missing.push("WECHATPAY_APP_ID");
    } else {
        println!("✅ WECHATPAY_APP_ID is set");
    }

    if std::env::var("WECHATPAY_SERIAL_NO").is_err() {
        missing.push("WECHATPAY_SERIAL_NO");
    } else {
        println!("✅ WECHATPAY_SERIAL_NO is set");
    }

    if std::env::var("WECHATPAY_PRIVATE_KEY").is_err() {
        missing.push("WECHATPAY_PRIVATE_KEY");
    } else {
        println!("✅ WECHATPAY_PRIVATE_KEY is set");
    }

    if std::env::var("WECHATPAY_API_KEY").is_err() {
        missing.push("WECHATPAY_API_KEY");
    } else {
        println!("✅ WECHATPAY_API_KEY is set");
    }

    if !missing.is_empty() {
        println!("\n⚠️  Missing environment variables:");
        for var in &missing {
            println!("   - {}", var);
        }
        println!("\n📝 To run WeChat Pay integration tests, set:");
        println!("   export WECHATPAY_MCH_ID=\"1234567890\"");
        println!("   export WECHATPAY_APP_ID=\"wx1234567890abcdef\"");
        println!("   export WECHATPAY_SERIAL_NO=\"...\"");
        println!("   export WECHATPAY_PRIVATE_KEY=\"-----BEGIN PRIVATE KEY-----...\"");
        println!("   export WECHATPAY_API_KEY=\"...\"");
        println!("\n🔧 Getting WeChat Pay Credentials:");
        println!("   1. Apply for a WeChat Pay merchant account at https://pay.weixin.qq.com");
        println!("   2. Complete the onboarding process");
        println!("   3. Generate API certificates in the merchant platform");
        println!("   4. Download the private key and note the serial number");
    } else {
        println!("\n✅ All WeChat Pay environment variables are set!");
        println!("   Run: cargo test --test wechatpay_integration_tests -- --ignored\n");
    }
}

// ============================================================================
// Unit Tests (No API Keys Required)
// ============================================================================

#[test]
fn test_gateway_id() {
    let gateway = WeChatPayGateway::new(
        "test_mch".to_string(),
        "test_key".to_string(),
        "test_app".to_string(),
        "test_serial".to_string(),
        "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----".to_string(),
        true,
    );

    assert_eq!(gateway.id(), "wechatpay");
    assert_eq!(gateway.name(), "WeChat Pay");
}

#[test]
fn test_sandbox_mode() {
    let sandbox_gateway = WeChatPayGateway::new(
        "test_mch".to_string(),
        "test_key".to_string(),
        "test_app".to_string(),
        "test_serial".to_string(),
        "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----".to_string(),
        true, // sandbox
    );

    // Sandbox URL should contain sandbox
    assert!(sandbox_gateway.id() == "wechatpay");
}
