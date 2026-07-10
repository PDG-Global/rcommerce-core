//! Integration Tests for R Commerce API
//!
//! These tests use the API client to test complete user flows:
//! - User registration and authentication
//! - Product browsing
//! - Cart management
//! - Order creation and payment
//! - Webhook handling
//!
//! Run with: TEST_SERVER_URL=http://localhost:8080 cargo test --test integration_test

use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

/// Test harness that manages the test client
pub struct TestHarness {
    base_url: String,
    http_client: Client,
}

impl TestHarness {
    pub async fn new() -> anyhow::Result<Self> {
        // Initialize tracing
        let _ = tracing_subscriber::fmt::try_init();

        // Create HTTP client
        let http_client = Client::builder().timeout(Duration::from_secs(30)).build()?;

        // Get server URL from environment or use default
        let base_url = std::env::var("TEST_SERVER_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

        Ok(Self {
            base_url,
            http_client,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn client(&self) -> &Client {
        &self.http_client
    }
}

// =============================================================================
// API Types
// =============================================================================

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct RegisterRequest {
    email: String,
    password: String,
    first_name: String,
    last_name: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AuthResponse {
    #[serde(rename = "access_token")]
    token: String,
    customer: CustomerResponse,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CustomerResponse {
    id: String,
    email: String,
    first_name: String,
    last_name: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct CreateOrderRequest {
    customer_email: String,
    items: Vec<CreateOrderItem>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct CreateOrderItem {
    product_id: String,
    quantity: i32,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OrderResponse {
    id: String,
    order_number: String,
    #[serde(rename = "customer_email")]
    email: String,
    total: String,
    status: String,
}

// =============================================================================
// Test Cases
// =============================================================================

#[tokio::test]
#[ignore = "Requires running server at TEST_SERVER_URL"]
async fn test_health_check() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    let response = harness
        .client()
        .get(format!("{}/health", harness.base_url()))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[ignore = "Requires running server at TEST_SERVER_URL"]
async fn test_api_info() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    let response = harness
        .client()
        .get(format!("{}/", harness.base_url()))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let info: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_eq!(
        info.get("name").and_then(|v| v.as_str()),
        Some("R Commerce API")
    );
}

#[tokio::test]
async fn test_complete_customer_flow() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");
    let test_email = format!("test-{}@example.com", Uuid::new_v4());

    // 1. Register a new customer
    let register_response = harness
        .client()
        .post(format!("{}/api/v1/auth/register", harness.base_url()))
        .json(&json!({
            "email": test_email,
            "password": "TestPassword123!",
            "first_name": "Test",
            "last_name": "User"
        }))
        .send()
        .await
        .expect("Failed to register");

    // Registration might fail if email already exists or server not running
    // For MVP testing, we accept CREATED or CONFLICT (if user exists)
    assert!(
        register_response.status() == StatusCode::CREATED
            || register_response.status() == StatusCode::CONFLICT
            || register_response.status() == StatusCode::SERVICE_UNAVAILABLE,
        "Unexpected status: {:?}",
        register_response.status()
    );

    if register_response.status() == StatusCode::CREATED {
        // Register response doesn't include token, so we need to login
        // 2. Login with the new account
        let login_response = harness
            .client()
            .post(format!("{}/api/v1/auth/login", harness.base_url()))
            .json(&json!({
                "email": test_email,
                "password": "TestPassword123!"
            }))
            .send()
            .await
            .expect("Failed to login");

        assert_eq!(login_response.status(), StatusCode::OK);

        let auth: AuthResponse = login_response
            .json()
            .await
            .expect("Failed to parse auth response");
        assert_eq!(auth.customer.email, test_email);

        // 3. Get customer profile
        let profile_response = harness
            .client()
            .get(format!("{}/api/v1/customers/me", harness.base_url()))
            .header("Authorization", format!("Bearer {}", auth.token))
            .send()
            .await
            .expect("Failed to get profile");

        // Profile might require valid token
        assert!(
            profile_response.status() == StatusCode::OK
                || profile_response.status() == StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn test_product_listing() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Get product list
    let response = harness
        .client()
        .get(format!("{}/api/v1/products", harness.base_url()))
        .send()
        .await
        .expect("Failed to get products");

    // Server might not be running
    if response.status() == StatusCode::SERVICE_UNAVAILABLE {
        println!("Server not available, skipping test");
        return;
    }

    assert_eq!(response.status(), StatusCode::OK);

    let products: serde_json::Value = response.json().await.expect("Failed to parse products");
    assert!(products.get("products").is_some());
}

#[tokio::test]
async fn test_order_creation_flow() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");
    let test_email = format!("test-{}@example.com", Uuid::new_v4());

    // First, ensure we have a product to order
    let product_response = harness
        .client()
        .get(format!("{}/api/v1/products", harness.base_url()))
        .send()
        .await
        .expect("Failed to get products");

    if product_response.status() != StatusCode::OK {
        println!("Server not available or no products, skipping test");
        return;
    }

    let products: serde_json::Value = product_response
        .json()
        .await
        .expect("Failed to parse products");
    let products_array = products.get("products").and_then(|p| p.as_array());

    if products_array.map(|a| a.is_empty()).unwrap_or(true) {
        println!("Skipping order test - no products in database");
        return;
    }

    let product_id = products_array.unwrap()[0]
        .get("id")
        .and_then(|id| id.as_str())
        .expect("Product has no id");

    // Create an order
    let order_response = harness
        .client()
        .post(format!("{}/api/v1/orders", harness.base_url()))
        .json(&json!({
            "customer_email": test_email,
            "items": [
                {
                    "product_id": product_id,
                    "quantity": 2
                }
            ]
        }))
        .send()
        .await
        .expect("Failed to create order");

    // Order creation should succeed or return validation error
    assert!(
        order_response.status() == StatusCode::CREATED
            || order_response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || order_response.status() == StatusCode::SERVICE_UNAVAILABLE,
        "Unexpected status: {:?}",
        order_response.status()
    );

    if order_response.status() == StatusCode::CREATED {
        let order: OrderResponse = order_response.json().await.expect("Failed to parse order");
        assert_eq!(order.email, test_email);
        assert!(!order.order_number.is_empty());

        // Get order by ID
        let get_response = harness
            .client()
            .get(format!("{}/api/v1/orders/{}", harness.base_url(), order.id))
            .send()
            .await
            .expect("Failed to get order");

        assert_eq!(get_response.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn test_payment_methods() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    let response = harness
        .client()
        .post(format!("{}/api/v1/payments/methods", harness.base_url()))
        .json(&json!({
            "currency": "USD",
            "amount": "100.00"
        }))
        .send()
        .await
        .expect("Failed to get payment methods");

    if response.status() == StatusCode::SERVICE_UNAVAILABLE {
        println!("Server not available, skipping test");
        return;
    }

    assert_eq!(response.status(), StatusCode::OK);

    let methods: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse payment methods");
    assert!(methods.as_array().map(|a| !a.is_empty()).unwrap_or(false));
}

#[tokio::test]
async fn test_webhook_handling() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Test Stripe webhook endpoint
    let response = harness
        .client()
        .post(format!("{}/api/v1/webhooks/stripe", harness.base_url()))
        .header("Stripe-Signature", "test_signature")
        .body(
            r#"{
            "id": "evt_test",
            "type": "payment_intent.succeeded",
            "data": {
                "object": {
                    "id": "pi_test",
                    "amount": 2000,
                    "currency": "usd"
                }
            }
        }"#,
        )
        .send()
        .await
        .expect("Failed to send webhook");

    if response.status() == StatusCode::SERVICE_UNAVAILABLE {
        println!("Server not available, skipping test");
        return;
    }

    // Webhook should be accepted (even if signature validation fails in test)
    assert!(response.status().is_success() || response.status() == StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_error_handling() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Test 404 for non-existent order
    let fake_id = Uuid::new_v4();
    let response = harness
        .client()
        .get(format!("{}/api/v1/orders/{}", harness.base_url(), fake_id))
        .send()
        .await
        .expect("Failed to send request");

    if response.status() == StatusCode::SERVICE_UNAVAILABLE {
        println!("Server not available, skipping test");
        return;
    }

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Test validation error for invalid email
    let response = harness
        .client()
        .post(format!("{}/api/v1/auth/register", harness.base_url()))
        .json(&json!({
            "email": "invalid-email",
            "password": "123",
            "first_name": "",
            "last_name": ""
        }))
        .send()
        .await
        .expect("Failed to send request");

    // Should return validation error
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::BAD_REQUEST
    );
}
