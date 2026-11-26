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
use sqlx::{Pool, Postgres};
use uuid::Uuid;

/// Test harness that manages the test client
pub struct TestHarness {
    base_url: String,
    http_client: Client,
    /// Present when `TEST_DATABASE_URL` is set — used to complete the
    /// email-verification step the designed register→login flow requires,
    /// exactly like `integration_tests.rs`'s `create_test_customer`.
    db: Option<Pool<Postgres>>,
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

        let db = match std::env::var("TEST_DATABASE_URL") {
            Ok(url) => Some(
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(2)
                    .connect(&url)
                    .await?,
            ),
            Err(_) => None,
        };

        Ok(Self {
            base_url,
            http_client,
            db,
        })
    }

    /// Register a unique customer, mark the email verified in the database
    /// (login requires a verified email by design — the production flow
    /// verifies via an emailed link, the suite bypasses the email hop),
    /// log in, and return `(email, bearer token)`. Requires
    /// `TEST_DATABASE_URL`.
    pub async fn verified_customer_token(&self) -> anyhow::Result<(String, String)> {
        let db = self.db.as_ref().ok_or_else(|| {
            anyhow::anyhow!("TEST_DATABASE_URL is required for the verified-login path")
        })?;
        let email = format!("test-{}@example.com", Uuid::new_v4());

        let register = self
            .http_client
            .post(format!("{}/api/v1/auth/register", self.base_url))
            .json(&json!({
                "email": email,
                "password": "TestPassword123!",
                "first_name": "Test",
                "last_name": "User"
            }))
            .send()
            .await?;
        assert_eq!(
            register.status(),
            StatusCode::CREATED,
            "registration must succeed"
        );

        sqlx::query("UPDATE customers SET email_verified = true WHERE email = $1")
            .bind(&email)
            .execute(db)
            .await?;

        let login = self
            .http_client
            .post(format!("{}/api/v1/auth/login", self.base_url))
            .json(&json!({
                "email": email,
                "password": "TestPassword123!"
            }))
            .send()
            .await?;
        assert_eq!(
            login.status(),
            StatusCode::OK,
            "login after email verification must succeed"
        );
        let auth: AuthResponse = login.json().await?;
        Ok((email, auth.token))
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
#[ignore = "Requires running server at TEST_SERVER_URL"]
async fn test_complete_customer_flow() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Register → (designed) email verification → login → profile.
    let (test_email, token) = harness
        .verified_customer_token()
        .await
        .expect("verified login must succeed");

    let profile_response = harness
        .client()
        .get(format!("{}/api/v1/customers/me", harness.base_url()))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("Failed to get profile");

    assert_eq!(profile_response.status(), StatusCode::OK);
    let profile: serde_json::Value = profile_response
        .json()
        .await
        .expect("Failed to parse profile");
    assert_eq!(
        profile
            .get("customer")
            .and_then(|c| c.get("email"))
            .and_then(|v| v.as_str()),
        Some(test_email.as_str())
    );
}

#[tokio::test]
#[ignore = "Requires running server at TEST_SERVER_URL"]
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
#[ignore = "Requires running server at TEST_SERVER_URL"]
async fn test_order_creation_flow() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Seed a dedicated in-stock product: which products exist at this
    // point depends on which suites ran before (some leave zero-inventory
    // fixtures, and order creation refuses overselling by design, #60).
    let product_id = Uuid::new_v4();
    {
        let db = harness
            .db
            .as_ref()
            .expect("TEST_DATABASE_URL is required to seed the order test product");
        sqlx::query(
            "INSERT INTO products (id, title, slug, product_type, price, currency, inventory_quantity, is_active, created_at, updated_at)
             VALUES ($1, $2, $3, $4::product_type, $5, $6::currency, $7, $8, $9, $10)",
        )
        .bind(product_id)
        .bind("Order Test Product")
        .bind(format!("order-test-{product_id}"))
        .bind("simple")
        .bind(rust_decimal::Decimal::new(1999, 2))
        .bind("USD")
        .bind(100i32)
        .bind(true)
        .bind(chrono::Utc::now())
        .bind(chrono::Utc::now())
        .execute(db)
        .await
        .expect("seed product insert must succeed");
    }
    let product_id = product_id.to_string();

    // Create an order as a verified, authenticated customer (the order
    // routes require auth by design).
    let (test_email, token) = harness
        .verified_customer_token()
        .await
        .expect("verified login must succeed");
    let order_response = harness
        .client()
        .post(format!("{}/api/v1/orders", harness.base_url()))
        .header("Authorization", format!("Bearer {token}"))
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
    let order_status = order_response.status();
    if order_status != StatusCode::CREATED {
        let body = order_response.text().await.unwrap_or_default();
        panic!("unexpected order-create status {order_status}: {body}");
    }

    if order_response.status() == StatusCode::CREATED {
        let order: OrderResponse = order_response.json().await.expect("Failed to parse order");
        assert_eq!(order.email, test_email);
        assert!(!order.order_number.is_empty());

        // Get order by ID
        let get_response = harness
            .client()
            .get(format!("{}/api/v1/orders/{}", harness.base_url(), order.id))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("Failed to get order");

        assert_eq!(get_response.status(), StatusCode::OK);
    }
}

#[tokio::test]
#[ignore = "Requires running server at TEST_SERVER_URL"]
async fn test_payment_methods() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    let (_email, token) = harness
        .verified_customer_token()
        .await
        .expect("verified login must succeed");
    let response = harness
        .client()
        .post(format!("{}/api/v1/payments/methods", harness.base_url()))
        .header("Authorization", format!("Bearer {token}"))
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
#[ignore = "Requires running server at TEST_SERVER_URL"]
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
#[ignore = "Requires running server at TEST_SERVER_URL"]
async fn test_error_handling() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Unauthenticated requests to the order routes get 401, not 404: auth
    // runs before existence (by design — order existence must not leak to
    // anonymous callers).
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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

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
