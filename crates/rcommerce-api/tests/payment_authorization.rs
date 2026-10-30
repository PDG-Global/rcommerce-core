use std::sync::{Arc, Mutex};

use anyhow::ensure;
use async_trait::async_trait;
use axum::{extract::State, Extension, Json};
use rcommerce_api::middleware::JwtAuth;
use rcommerce_api::routes::payment::{
    complete_payment_action, delete_payment_method, get_payment_status, get_saved_payment_methods,
    handle_webhook, initiate_payment, refund_payment, save_payment_method,
    CompletePaymentActionApiRequest, InitiatePaymentApiRequest, RefundRequest,
    SavePaymentMethodRequest,
};
use rcommerce_api::state::{AppState, AppStateParams};
use rcommerce_core::inventory::{InventoryConfig, InventoryService};
use rcommerce_core::order::lifecycle::OrderEventDispatcher;
use rcommerce_core::payment::agnostic::*;
use rcommerce_core::payment::gateways::MockPaymentGateway;
use rcommerce_core::repository::cart_repository::PgCartRepository;
use rcommerce_core::repository::coupon_repository::PgCouponRepository;
use rcommerce_core::repository::{
    CustomerRepository, Database, PostgresApiKeyRepository, PostgresSubscriptionRepository,
    ProductRepository,
};
use rcommerce_core::services::{
    AuthService, CartService, CheckoutConfig, CheckoutService, CouponService, CustomerService,
    OrderService, ProductService,
};
use rcommerce_core::shipping::ShippingProviderFactory;
use rcommerce_core::{Config, Error, FileUploadService, Result, StripeGateway};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Default)]
struct SecondaryCalls {
    initiates: Vec<InitiatePaymentRequest>,
    completions: Vec<String>,
    statuses: Vec<String>,
    refunds: Vec<(String, Option<Decimal>)>,
}

#[derive(Clone)]
struct SecondaryGateway {
    calls: Arc<Mutex<SecondaryCalls>>,
    status: Arc<Mutex<PaymentStatus>>,
    initiate_delay: Arc<Mutex<Option<std::time::Duration>>>,
    cancel_initiation: Arc<Mutex<bool>>,
    cancel_completion: Arc<Mutex<bool>>,
    payment_id: Arc<Mutex<Option<String>>>,
}

impl Default for SecondaryGateway {
    fn default() -> Self {
        Self {
            calls: Arc::new(Mutex::new(SecondaryCalls::default())),
            status: Arc::new(Mutex::new(PaymentStatus::Pending)),
            initiate_delay: Arc::new(Mutex::new(None)),
            cancel_initiation: Arc::new(Mutex::new(false)),
            cancel_completion: Arc::new(Mutex::new(false)),
            payment_id: Arc::new(Mutex::new(None)),
        }
    }
}

impl SecondaryGateway {
    fn calls(&self) -> std::sync::MutexGuard<'_, SecondaryCalls> {
        self.calls.lock().unwrap()
    }

    fn payment_id(&self) -> String {
        self.payment_id
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "secondary_intent_1".to_string())
    }
}

fn card_info() -> PaymentMethodInfo {
    PaymentMethodInfo {
        method_type: PaymentMethodType::Card,
        last_four: Some("4242".to_string()),
        card_brand: Some("visa".to_string()),
        exp_month: Some("12".to_string()),
        exp_year: Some("2030".to_string()),
        cardholder_name: Some("Test Buyer".to_string()),
        token: None,
    }
}

#[async_trait]
impl AgnosticPaymentGateway for SecondaryGateway {
    async fn get_config(&self) -> Result<GatewayConfig> {
        Ok(GatewayConfig {
            gateway_id: "secondary".to_string(),
            gateway_name: "Secondary".to_string(),
            payment_methods: vec![],
            supports_3ds: true,
            supports_webhooks: false,
            supports_refunds: true,
            supports_partial_refunds: true,
            supported_currencies: vec!["USD".to_string()],
            default_currency: "USD".to_string(),
        })
    }

    async fn initiate_payment(
        &self,
        request: InitiatePaymentRequest,
    ) -> Result<InitiatePaymentResponse> {
        self.calls().initiates.push(request);
        let delay = { *self.initiate_delay.lock().unwrap() };
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        if *self.cancel_initiation.lock().unwrap() {
            return Ok(InitiatePaymentResponse::Failed {
                payment_id: self.payment_id(),
                error_code: "payment_cancelled".to_string(),
                error_message: "Payment cancelled".to_string(),
                retry_allowed: false,
            });
        }
        Ok(InitiatePaymentResponse::RequiresAction {
            payment_id: self.payment_id(),
            action_type: PaymentActionType::ThreeDSecure,
            action_data: serde_json::json!({"challenge": true}),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        })
    }

    async fn complete_payment_action(
        &self,
        request: CompletePaymentActionRequest,
    ) -> Result<CompletePaymentActionResponse> {
        self.calls().completions.push(request.payment_id.clone());
        if *self.cancel_completion.lock().unwrap() {
            *self.status.lock().unwrap() = PaymentStatus::Cancelled;
            return Ok(CompletePaymentActionResponse::Failed {
                payment_id: request.payment_id,
                error_code: "payment_cancelled".to_string(),
                error_message: "Payment cancelled".to_string(),
                retry_allowed: false,
            });
        }
        *self.status.lock().unwrap() = PaymentStatus::Succeeded;
        Ok(CompletePaymentActionResponse::Success {
            payment_id: request.payment_id,
            transaction_id: "secondary_transaction_1".to_string(),
            payment_status: PaymentStatus::Succeeded,
            payment_method: card_info(),
            receipt_url: None,
        })
    }

    async fn get_payment_status(&self, payment_id: &str) -> Result<PaymentStatus> {
        self.calls().statuses.push(payment_id.to_string());
        Ok(self.status.lock().unwrap().clone())
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<RefundResponse> {
        self.calls().refunds.push((payment_id.to_string(), amount));
        Ok(RefundResponse {
            refund_id: "secondary_refund_1".to_string(),
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(dec!(40.00)),
            currency: "USD".to_string(),
            status: RefundStatus::Succeeded,
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn handle_webhook(
        &self,
        payload: &[u8],
        _headers: &[(String, String)],
    ) -> Result<WebhookEvent> {
        serde_json::from_slice(payload).map_err(Into::into)
    }

    async fn tokenize_payment_method(
        &self,
        _payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken> {
        Err(Error::payment_error("Not supported"))
    }

    async fn get_saved_payment_methods(
        &self,
        _customer_id: &str,
    ) -> Result<Vec<PaymentMethodInfo>> {
        Ok(vec![])
    }

    async fn delete_payment_method(&self, _token: &str) -> Result<()> {
        Ok(())
    }
}

async fn test_app_state(pool: PgPool, payment_service: PaymentService) -> AppState {
    let db = Database::new(pool.clone());
    let product_service = ProductService::new(ProductRepository::new(db.clone()));
    let customer_service = CustomerService::new(CustomerRepository::new(db.clone()));
    let auth_service = AuthService::new(Config::default());
    let cart_repo: Arc<dyn rcommerce_core::repository::CartRepository> =
        Arc::new(PgCartRepository::new(pool.clone()));
    let coupon_repo: Arc<dyn rcommerce_core::repository::CouponRepository> =
        Arc::new(PgCouponRepository::new(pool.clone()));
    let coupon_service_arc = Arc::new(CouponService::new(coupon_repo.clone(), cart_repo.clone()));
    let cart_service = Arc::new(
        CartService::new(
            cart_repo.clone(),
            coupon_repo.clone(),
            coupon_service_arc.clone(),
        )
        .with_bundle_service(Arc::new(rcommerce_core::BundleService::new(db.clone())))
        .with_database(db.clone()),
    );
    let inventory_service = Arc::new(InventoryService::new(
        db.clone(),
        InventoryConfig::default(),
    ));
    let order_service = Arc::new(OrderService::new(
        db.clone(),
        Box::new(StripeGateway::new(
            "sk_test_dummy".to_string(),
            "whsec_dummy".to_string(),
        )),
        inventory_service.as_ref().clone(),
        OrderEventDispatcher::new(),
    ));
    let tax_service = Arc::new(rcommerce_core::tax::DefaultTaxService::new(pool.clone()));
    let shipping_factory = Arc::new(ShippingProviderFactory::new());
    let payment_gateway: Arc<dyn rcommerce_core::PaymentGateway> = Arc::new(StripeGateway::new(
        "sk_test_dummy".to_string(),
        "whsec_dummy".to_string(),
    ));
    let checkout_service = Arc::new(CheckoutService::new(
        cart_service.clone(),
        tax_service.clone(),
        order_service.clone(),
        payment_gateway,
        shipping_factory.clone(),
        CheckoutConfig::default(),
    ));

    AppState::new(AppStateParams::new(
        product_service,
        customer_service,
        auth_service,
        db,
        None,
        PostgresApiKeyRepository::new(pool.clone()),
        PostgresSubscriptionRepository::new(pool),
        CouponService::new(coupon_repo, cart_repo),
        payment_service,
        FileUploadService::new_local(
            std::path::PathBuf::from("./test_uploads"),
            "http://localhost:8080/uploads".to_string(),
        )
        .unwrap(),
        cart_service,
        order_service,
        tax_service,
        shipping_factory,
        checkout_service,
        None,
        inventory_service,
        None,
        None,
        None,
        Default::default(),
        Default::default(),
        "USD".to_string(),
        None,
    ))
    .await
}

async fn test_pool() -> anyhow::Result<PgPool> {
    let database_url = std::env::var("TEST_DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    rcommerce_core::Migrator::new(pool.clone())
        .migrate()
        .await?;
    Ok(pool)
}

async fn create_customer(pool: &PgPool, email: &str) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers (id, email, first_name, last_name, currency) \
         VALUES ($1, $2, 'Test', 'Buyer', 'USD')",
    )
    .bind(id)
    .bind(email)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn create_order(
    pool: &PgPool,
    customer_id: Option<Uuid>,
    customer_email: &str,
) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orders (id, order_number, customer_id, customer_email, currency, \
         subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, \
         fulfillment_status) VALUES ($1, $2, $3, $4, 'USD', 40, 0, 0, 0, 40, \
         'pending', 'pending', 'pending')",
    )
    .bind(id)
    .bind(format!("TEST-{id}"))
    .bind(customer_id)
    .bind(customer_email)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn create_active_reservation(pool: &PgPool, order_id: Uuid) -> anyhow::Result<Uuid> {
    let product_id = Uuid::new_v4();
    sqlx::query("INSERT INTO products (id, title, slug, price) VALUES ($1, $2, $3, 40)")
        .bind(product_id)
        .bind(format!("Reservation product {product_id}"))
        .bind(format!("reservation-{product_id}"))
        .execute(pool)
        .await?;
    let location_id: Uuid =
        sqlx::query_scalar("SELECT id FROM inventory_locations WHERE is_active LIMIT 1")
            .fetch_one(pool)
            .await?;
    let reservation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO stock_reservations \
         (id, product_id, location_id, order_id, quantity, expires_at) \
         VALUES ($1, $2, $3, $4, 1, NOW() + INTERVAL '1 hour')",
    )
    .bind(reservation_id)
    .bind(product_id)
    .bind(location_id)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(reservation_id)
}

async fn assert_cancelled_with_released_reservation(
    pool: &PgPool,
    order_id: Uuid,
) -> anyhow::Result<()> {
    let states: (String, String, String, String) = sqlx::query_as(
        "SELECT p.status::text, o.status::text, o.payment_status::text, r.status::text \
         FROM orders o JOIN payments p ON p.order_id = o.id \
         JOIN stock_reservations r ON r.order_id = o.id WHERE o.id = $1",
    )
    .bind(order_id)
    .fetch_one(pool)
    .await?;
    ensure!(
        states
            == (
                "cancelled".to_string(),
                "cancelled".to_string(),
                "cancelled".to_string(),
                "released".to_string()
            )
    );
    Ok(())
}

fn auth(customer_id: Uuid) -> JwtAuth {
    JwtAuth {
        customer_id,
        email: "customer@example.com".to_string(),
        permissions: vec!["read".to_string()],
    }
}

fn admin() -> JwtAuth {
    JwtAuth {
        customer_id: Uuid::new_v4(),
        email: "admin@example.com".to_string(),
        permissions: vec!["admin".to_string()],
    }
}

fn card_data() -> PaymentMethodData {
    PaymentMethodData::Card {
        number: "4242424242424242".to_string(),
        exp_month: "12".to_string(),
        exp_year: "2030".to_string(),
        cvc: "123".to_string(),
        name: "Test Buyer".to_string(),
    }
}

fn initiate_request(
    order_id: Uuid,
    gateway_id: &str,
    data: PaymentMethodData,
) -> InitiatePaymentApiRequest {
    InitiatePaymentApiRequest {
        gateway_id: gateway_id.to_string(),
        amount: "40.00".to_string(),
        currency: "USD".to_string(),
        payment_method_type: PaymentMethodType::Card,
        order_id: order_id.to_string(),
        customer_email: "spoofed@example.com".to_string(),
        payment_method_data: data,
        save_payment_method: false,
        description: "Test order".to_string(),
        idempotency_key: None,
    }
}

async fn create_paid_order(
    state: &AppState,
    pool: &PgPool,
    owner: Uuid,
    gateway_id: &str,
) -> anyhow::Result<Uuid> {
    let order_id = create_order(pool, Some(owner), "paid@example.com").await?;
    let _ = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(order_id, gateway_id, card_data())),
    )
    .await?;
    let _ = complete_payment_action(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path(format!("{gateway_id}:secondary_intent_1")),
        Json(CompletePaymentActionApiRequest {
            action_type: PaymentActionType::ThreeDSecure,
            action_data: serde_json::json!({}),
        }),
    )
    .await?;
    Ok(order_id)
}

async fn send_webhook(
    state: &AppState,
    gateway_id: &str,
    event_type: WebhookEventType,
    payment_id: &str,
    transaction_id: Option<&str>,
    data: serde_json::Value,
) -> anyhow::Result<()> {
    let event = WebhookEvent {
        event_type,
        payment_id: payment_id.to_string(),
        transaction_id: transaction_id.map(str::to_string),
        data,
        timestamp: chrono::Utc::now(),
    };
    let _ = handle_webhook(
        State(state.clone()),
        axum::extract::Path(gateway_id.to_string()),
        axum::http::HeaderMap::new(),
        serde_json::to_string(&event)?,
    )
    .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn direct_payment_authorization_persistence_and_secondary_refund() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let secondary = SecondaryGateway::default();
    let mut payment_service = PaymentService::new("mock".to_string());
    payment_service.register_gateway("mock".to_string(), Box::new(MockPaymentGateway::new()));
    payment_service.register_gateway("secondary".to_string(), Box::new(secondary.clone()));
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(&pool, &format!("owner-{}@example.com", Uuid::new_v4())).await?;
    let other = create_customer(&pool, &format!("other-{}@example.com", Uuid::new_v4())).await?;
    let order_id = create_order(&pool, Some(owner), "authoritative@example.com").await?;
    let other_order_id = create_order(&pool, Some(other), "other@example.com").await?;

    let foreign = initiate_payment(
        State(state.clone()),
        Extension(auth(other)),
        Json(initiate_request(order_id, "secondary", card_data())),
    )
    .await
    .unwrap_err();
    ensure!(foreign.status_code() == 404);

    let mut mismatched = initiate_request(order_id, "secondary", card_data());
    mismatched.amount = "0.01".to_string();
    let mismatch = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(mismatched),
    )
    .await
    .unwrap_err();
    ensure!(mismatch.status_code() == 400);
    ensure!(secondary.calls().initiates.is_empty());

    let response = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(order_id, "secondary", card_data())),
    )
    .await?;
    ensure!(matches!(
        response.0,
        InitiatePaymentResponse::RequiresAction { .. }
    ));
    {
        let calls = secondary.calls();
        ensure!(calls.initiates.len() == 1);
        let request = &calls.initiates[0];
        ensure!(request.amount == dec!(40.00));
        ensure!(request.currency == "USD");
        ensure!(request.customer_id == Some(owner));
        ensure!(request.customer_email == "authoritative@example.com");
    }

    let payment: (String, String, Decimal, String, String) = sqlx::query_as(
        "SELECT gateway, gateway_payment_id, amount, currency::text, status::text \
         FROM payments WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(payment.0 == "secondary");
    ensure!(payment.1 == "secondary_intent_1");
    ensure!(payment.2 == dec!(40.00));
    ensure!(payment.3 == "USD");
    ensure!(payment.4 == "pending");

    let foreign_status = get_payment_status(
        State(state.clone()),
        Extension(auth(other)),
        axum::extract::Path("secondary_intent_1".to_string()),
    )
    .await
    .unwrap_err();
    ensure!(foreign_status.status_code() == 404);
    let foreign_action = complete_payment_action(
        State(state.clone()),
        Extension(auth(other)),
        axum::extract::Path("secondary_intent_1".to_string()),
        Json(CompletePaymentActionApiRequest {
            action_type: PaymentActionType::ThreeDSecure,
            action_data: serde_json::json!({}),
        }),
    )
    .await
    .unwrap_err();
    ensure!(foreign_action.status_code() == 404);
    ensure!(secondary.calls().statuses.is_empty());
    ensure!(secondary.calls().completions.is_empty());

    *secondary.status.lock().unwrap() = PaymentStatus::Failed;
    let _ = get_payment_status(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path("secondary_intent_1".to_string()),
    )
    .await?;
    let failed: (String, String, String) = sqlx::query_as(
        "SELECT p.status::text, o.status::text, o.payment_status::text \
         FROM payments p JOIN orders o ON o.id = p.order_id WHERE p.order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(
        failed
            == (
                "failed".to_string(),
                "pending".to_string(),
                "failed".to_string()
            )
    );

    let _ = complete_payment_action(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path("secondary_intent_1".to_string()),
        Json(CompletePaymentActionApiRequest {
            action_type: PaymentActionType::ThreeDSecure,
            action_data: serde_json::json!({"approved": true}),
        }),
    )
    .await?;
    let payment_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE order_id = $1")
            .bind(order_id)
            .fetch_one(&pool)
            .await?;
    ensure!(payment_count == 1);
    let paid: (String, String, String) = sqlx::query_as(
        "SELECT p.status::text, o.status::text, o.payment_status::text \
         FROM payments p JOIN orders o ON o.id = p.order_id WHERE p.order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(
        paid == (
            "paid".to_string(),
            "confirmed".to_string(),
            "paid".to_string()
        )
    );

    let _ = get_payment_status(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path("secondary_intent_1".to_string()),
    )
    .await?;
    let payment_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE order_id = $1")
            .bind(order_id)
            .fetch_one(&pool)
            .await?;
    ensure!(payment_count == 1);

    let unauthorized_refund = refund_payment(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path("secondary_intent_1".to_string()),
        Json(RefundRequest {
            amount: None,
            reason: "customer_request".to_string(),
        }),
    )
    .await
    .unwrap_err();
    ensure!(unauthorized_refund.status_code() == 403);
    ensure!(secondary.calls().refunds.is_empty());

    let _ = refund_payment(
        State(state.clone()),
        Extension(admin()),
        axum::extract::Path("secondary_intent_1".to_string()),
        Json(RefundRequest {
            amount: None,
            reason: "customer_request".to_string(),
        }),
    )
    .await?;
    ensure!(secondary.calls().refunds.len() == 1);
    let refund_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refunds WHERE gateway_refund_id = 'secondary_refund_1'",
    )
    .fetch_one(&pool)
    .await?;
    ensure!(refund_count == 1);
    let refunded: (String, String, String, bool) = sqlx::query_as(
        "SELECT p.status::text, o.status::text, o.payment_status::text, \
         r.settled_at IS NOT NULL FROM payments p \
         JOIN orders o ON o.id = p.order_id JOIN refunds r ON r.payment_id = p.id \
         WHERE p.order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(
        refunded
            == (
                "refunded".to_string(),
                "refunded".to_string(),
                "refunded".to_string(),
                true
            )
    );

    let missing = get_payment_status(
        State(state.clone()),
        Extension(admin()),
        axum::extract::Path("missing_intent".to_string()),
    )
    .await
    .unwrap_err();
    ensure!(missing.status_code() == 404);

    let foreign_order = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(other_order_id, "secondary", card_data())),
    )
    .await
    .unwrap_err();
    ensure!(foreign_order.status_code() == 404);
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn saved_method_ownership_and_card_token_use() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let mut payment_service = PaymentService::new("mock".to_string());
    payment_service.register_gateway("mock".to_string(), Box::new(MockPaymentGateway::new()));
    payment_service.register_gateway(
        "secondary".to_string(),
        Box::new(SecondaryGateway::default()),
    );
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(&pool, &format!("saved-{}@example.com", Uuid::new_v4())).await?;
    let other = create_customer(
        &pool,
        &format!("saved-other-{}@example.com", Uuid::new_v4()),
    )
    .await?;
    let owner_order = create_order(&pool, Some(owner), "saved@example.com").await?;
    let other_order = create_order(&pool, Some(other), "saved-other@example.com").await?;

    let token = save_payment_method(
        State(state.clone()),
        Extension(auth(owner)),
        Json(SavePaymentMethodRequest {
            gateway_id: "mock".to_string(),
            payment_method_data: card_data(),
        }),
    )
    .await?
    .0
    .token;
    let provider_customer_id: String = sqlx::query_scalar(
        "SELECT gateway_customer_id FROM payment_gateway_customers \
         WHERE customer_id = $1 AND gateway = 'mock'",
    )
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    ensure!(provider_customer_id.starts_with("mock_customer_"));
    ensure!(provider_customer_id != owner.to_string());
    let stored_hash: String = sqlx::query_scalar(
        "SELECT token_hash FROM saved_payment_methods WHERE customer_id = $1 AND gateway = 'mock'",
    )
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    ensure!(stored_hash == hex::encode(Sha256::digest(token.as_bytes())));
    ensure!(stored_hash != token);

    let methods = get_saved_payment_methods(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path(owner),
    )
    .await?
    .0;
    ensure!(methods.len() == 1);
    ensure!(methods[0].token.as_deref() == Some(token.as_str()));
    let foreign_list = get_saved_payment_methods(
        State(state.clone()),
        Extension(auth(other)),
        axum::extract::Path(owner),
    )
    .await
    .unwrap_err();
    ensure!(foreign_list.status_code() == 404);

    let foreign_token = initiate_payment(
        State(state.clone()),
        Extension(auth(other)),
        Json(initiate_request(
            other_order,
            "mock",
            PaymentMethodData::CardToken {
                token: token.clone(),
                cvc: Some("123".to_string()),
            },
        )),
    )
    .await
    .unwrap_err();
    ensure!(foreign_token.status_code() == 404);

    let _ = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(
            owner_order,
            "mock",
            PaymentMethodData::CardToken {
                token: token.clone(),
                cvc: Some("123".to_string()),
            },
        )),
    )
    .await?;

    let foreign_delete = delete_payment_method(
        State(state.clone()),
        Extension(auth(other)),
        axum::extract::Path(token.clone()),
    )
    .await
    .unwrap_err();
    ensure!(foreign_delete.status_code() == 404);
    let _ = delete_payment_method(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path(token.clone()),
    )
    .await?;
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM saved_payment_methods WHERE customer_id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await?;
    ensure!(remaining == 0);

    let unsupported = save_payment_method(
        State(state.clone()),
        Extension(auth(owner)),
        Json(SavePaymentMethodRequest {
            gateway_id: "secondary".to_string(),
            payment_method_data: card_data(),
        }),
    )
    .await
    .unwrap_err();
    ensure!(unsupported.status_code() == 402);
    let secondary_records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM saved_payment_methods WHERE customer_id = $1 AND gateway = 'secondary'",
    )
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    ensure!(secondary_records == 0);
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn initiation_rejects_unpayable_orders_and_serializes_first_attempts() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let gateway_id = "payment-race";
    let secondary = SecondaryGateway::default();
    let mut payment_service = PaymentService::new(gateway_id.to_string());
    payment_service.register_gateway(gateway_id.to_string(), Box::new(secondary.clone()));
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(&pool, &format!("race-{}@example.com", Uuid::new_v4())).await?;

    for (order_status, payment_status) in [
        ("pending", "paid"),
        ("pending", "refunded"),
        ("cancelled", "cancelled"),
        ("confirmed", "paid"),
    ] {
        let order_id = create_order(&pool, Some(owner), "race@example.com").await?;
        sqlx::query(
            "UPDATE orders SET status = $2::order_status, payment_status = $3::payment_status \
             WHERE id = $1",
        )
        .bind(order_id)
        .bind(order_status)
        .bind(payment_status)
        .execute(&pool)
        .await?;
        let error = initiate_payment(
            State(state.clone()),
            Extension(auth(owner)),
            Json(initiate_request(order_id, gateway_id, card_data())),
        )
        .await
        .unwrap_err();
        ensure!(error.status_code() == 400);
    }
    ensure!(secondary.calls().initiates.is_empty());

    let order_id = create_order(&pool, Some(owner), "race@example.com").await?;
    *secondary.initiate_delay.lock().unwrap() = Some(std::time::Duration::from_millis(150));
    let first_state = state.clone();
    let first = tokio::spawn(async move {
        initiate_payment(
            State(first_state),
            Extension(auth(owner)),
            Json(initiate_request(order_id, gateway_id, card_data())),
        )
        .await
    });
    for _ in 0..50 {
        if secondary.calls().initiates.len() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    ensure!(secondary.calls().initiates.len() == 1);
    let duplicate = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(order_id, gateway_id, card_data())),
    )
    .await
    .unwrap_err();
    ensure!(duplicate.status_code() == 400);
    let _ = first.await??;
    ensure!(secondary.calls().initiates.len() == 1);
    let sequential_retry = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(order_id, gateway_id, card_data())),
    )
    .await
    .unwrap_err();
    ensure!(sequential_retry.status_code() == 400);
    ensure!(secondary.calls().initiates.len() == 1);
    let active_attempts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payment_initiation_attempts WHERE order_id = $1")
            .bind(order_id)
            .fetch_one(&pool)
            .await?;
    ensure!(active_attempts == 0);
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn saved_card_token_uses_owned_provider_customer_and_denies_guest() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let gateway_id = "saved-card-provider";
    let secondary = SecondaryGateway::default();
    let mut payment_service = PaymentService::new(gateway_id.to_string());
    payment_service.register_gateway(gateway_id.to_string(), Box::new(secondary.clone()));
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(&pool, &format!("token-{}@example.com", Uuid::new_v4())).await?;
    let owner_order = create_order(&pool, Some(owner), "token@example.com").await?;
    let guest_order = create_order(&pool, None, "guest@example.com").await?;
    let token = format!("provider_token_{}", Uuid::new_v4());
    let provider_customer_id = format!("provider_customer_{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO payment_gateway_customers (customer_id, gateway, gateway_customer_id) \
         VALUES ($1, $2, $3)",
    )
    .bind(owner)
    .bind(gateway_id)
    .bind(&provider_customer_id)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO saved_payment_methods (customer_id, gateway, token_hash, method_type) \
         VALUES ($1, $2, $3, 'card')",
    )
    .bind(owner)
    .bind(gateway_id)
    .bind(hex::encode(Sha256::digest(token.as_bytes())))
    .execute(&pool)
    .await?;

    let _ = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(
            owner_order,
            gateway_id,
            PaymentMethodData::CardToken {
                token: token.clone(),
                cvc: Some("123".to_string()),
            },
        )),
    )
    .await?;
    {
        let calls = secondary.calls();
        ensure!(calls.initiates.len() == 1);
        let metadata = &calls.initiates[0].metadata;
        ensure!(metadata["gateway_customer_id"] == provider_customer_id);
        ensure!(metadata["gateway_customer_id"] != owner.to_string());
    }

    let guest = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(
            guest_order,
            gateway_id,
            PaymentMethodData::CardToken {
                token,
                cvc: Some("123".to_string()),
            },
        )),
    )
    .await
    .unwrap_err();
    ensure!(guest.status_code() == 404);
    ensure!(secondary.calls().initiates.len() == 1);
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn payment_identifiers_are_order_bound_and_gateway_disambiguated() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let primary_id = "identity-primary";
    let other_id = "identity-other";
    let primary = SecondaryGateway::default();
    let other = SecondaryGateway::default();
    let mut payment_service = PaymentService::new(primary_id.to_string());
    payment_service.register_gateway(primary_id.to_string(), Box::new(primary.clone()));
    payment_service.register_gateway(other_id.to_string(), Box::new(other.clone()));
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(&pool, &format!("identity-{}@example.com", Uuid::new_v4())).await?;
    let first_order = create_order(&pool, Some(owner), "identity@example.com").await?;
    let colliding_order = create_order(&pool, Some(owner), "identity@example.com").await?;
    let other_order = create_order(&pool, Some(owner), "identity@example.com").await?;

    let _ = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(first_order, primary_id, card_data())),
    )
    .await?;
    let collision = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(colliding_order, primary_id, card_data())),
    )
    .await
    .unwrap_err();
    ensure!(collision.status_code() == 402);
    let bound_order: Uuid = sqlx::query_scalar(
        "SELECT order_id FROM payments WHERE gateway = $1 AND gateway_payment_id = $2",
    )
    .bind(primary_id)
    .bind("secondary_intent_1")
    .fetch_one(&pool)
    .await?;
    ensure!(bound_order == first_order);
    let colliding_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE order_id = $1")
            .bind(colliding_order)
            .fetch_one(&pool)
            .await?;
    ensure!(colliding_rows == 0);

    let _ = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(other_order, other_id, card_data())),
    )
    .await?;
    let ambiguous = get_payment_status(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path("secondary_intent_1".to_string()),
    )
    .await
    .unwrap_err();
    ensure!(ambiguous.status_code() == 402);
    ensure!(primary.calls().statuses.is_empty());
    ensure!(other.calls().statuses.is_empty());

    let composite = get_payment_status(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path(format!("{primary_id}:secondary_intent_1")),
    )
    .await?;
    ensure!(composite.payment_id == "secondary_intent_1");
    ensure!(primary.calls().statuses.as_slice() == ["secondary_intent_1"]);
    let other_payment_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM payments WHERE gateway = $1 AND gateway_payment_id = $2",
    )
    .bind(other_id)
    .bind("secondary_intent_1")
    .fetch_one(&pool)
    .await?;
    let _ = get_payment_status(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path(other_payment_id.to_string()),
    )
    .await?;
    ensure!(other.calls().statuses.as_slice() == ["secondary_intent_1"]);

    let _ = complete_payment_action(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path(format!("{primary_id}:secondary_intent_1")),
        Json(CompletePaymentActionApiRequest {
            action_type: PaymentActionType::ThreeDSecure,
            action_data: serde_json::json!({}),
        }),
    )
    .await?;
    ensure!(primary.calls().completions.as_slice() == ["secondary_intent_1"]);
    let ambiguous_refund = refund_payment(
        State(state.clone()),
        Extension(admin()),
        axum::extract::Path("secondary_intent_1".to_string()),
        Json(RefundRequest {
            amount: None,
            reason: "customer_request".to_string(),
        }),
    )
    .await
    .unwrap_err();
    ensure!(ambiguous_refund.status_code() == 402);
    ensure!(primary.calls().refunds.is_empty());
    let _ = refund_payment(
        State(state.clone()),
        Extension(admin()),
        axum::extract::Path(format!("{primary_id}:secondary_intent_1")),
        Json(RefundRequest {
            amount: None,
            reason: "customer_request".to_string(),
        }),
    )
    .await?;
    ensure!(primary.calls().refunds.len() == 1);
    ensure!(other.calls().refunds.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn webhook_lookup_is_scoped_to_path_gateway() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let gateway_id = "webhook-owner";
    let secondary = SecondaryGateway::default();
    let mut payment_service = PaymentService::new(gateway_id.to_string());
    payment_service.register_gateway(gateway_id.to_string(), Box::new(secondary));
    payment_service.register_gateway(
        "webhook-unknown".to_string(),
        Box::new(SecondaryGateway::default()),
    );
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(&pool, &format!("webhook-{}@example.com", Uuid::new_v4())).await?;
    let order_id = create_order(&pool, Some(owner), "webhook@example.com").await?;
    let _ = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(order_id, gateway_id, card_data())),
    )
    .await?;

    send_webhook(
        &state,
        "webhook-unknown",
        WebhookEventType::PaymentSucceeded,
        "secondary_intent_1",
        None,
        serde_json::json!({}),
    )
    .await?;
    let untouched: (String, String) = sqlx::query_as(
        "SELECT p.status::text, o.status::text FROM payments p \
         JOIN orders o ON o.id = p.order_id WHERE p.order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(untouched == ("pending".to_string(), "pending".to_string()));

    send_webhook(
        &state,
        gateway_id,
        WebhookEventType::PaymentSucceeded,
        "secondary_intent_1",
        None,
        serde_json::json!({}),
    )
    .await?;
    let updated: (String, String) = sqlx::query_as(
        "SELECT p.status::text, o.status::text FROM payments p \
         JOIN orders o ON o.id = p.order_id WHERE p.order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(updated == ("paid".to_string(), "confirmed".to_string()));
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn synchronous_partial_refund_stays_paid_without_a_webhook() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let gateway_id = "refund-sync-partial";
    let secondary = SecondaryGateway::default();
    let mut payment_service = PaymentService::new(gateway_id.to_string());
    payment_service.register_gateway(gateway_id.to_string(), Box::new(secondary));
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(
        &pool,
        &format!("sync-refund-{}@example.com", Uuid::new_v4()),
    )
    .await?;
    let order_id = create_paid_order(&state, &pool, owner, gateway_id).await?;

    let _ = refund_payment(
        State(state.clone()),
        Extension(admin()),
        axum::extract::Path(format!("{gateway_id}:secondary_intent_1")),
        Json(RefundRequest {
            amount: Some("10.00".to_string()),
            reason: "customer_request".to_string(),
        }),
    )
    .await?;
    let refunded: (String, String, String, Decimal, bool) = sqlx::query_as(
        "SELECT p.status::text, o.status::text, o.payment_status::text, r.amount, \
         r.settled_at IS NOT NULL FROM payments p JOIN orders o ON o.id = p.order_id \
         JOIN refunds r ON r.payment_id = p.id WHERE p.order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(
        refunded
            == (
                "paid".to_string(),
                "confirmed".to_string(),
                "paid".to_string(),
                dec!(10.00),
                true
            )
    );

    send_webhook(
        &state,
        gateway_id,
        WebhookEventType::PaymentPartiallyRefunded,
        "secondary_intent_1",
        Some("secondary_refund_1"),
        serde_json::json!({
            "id": "secondary_refund_1",
            "amount": "10.00",
            "currency": "USD",
            "status": "succeeded"
        }),
    )
    .await?;
    let (refund_count, settled): (i64, Decimal) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(amount), 0) FROM refunds \
         WHERE payment_id = (SELECT id FROM payments WHERE order_id = $1) AND status = 'refunded'",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(refund_count == 1);
    ensure!(settled == dec!(10.00));
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn refund_webhooks_reconcile_partial_then_full_with_actual_amounts() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let gateway_id = "refund-webhook-cumulative";
    let secondary = SecondaryGateway::default();
    let mut payment_service = PaymentService::new(gateway_id.to_string());
    payment_service.register_gateway(gateway_id.to_string(), Box::new(secondary));
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(
        &pool,
        &format!("webhook-refund-{}@example.com", Uuid::new_v4()),
    )
    .await?;
    let order_id = create_paid_order(&state, &pool, owner, gateway_id).await?;
    let payment_id: Uuid = sqlx::query_scalar("SELECT id FROM payments WHERE order_id = $1")
        .bind(order_id)
        .fetch_one(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO refunds (payment_id, order_id, amount, currency, status, gateway_refund_id) \
         VALUES ($1, $2, 1, 'USD', 'pending', 'webhook_refund_partial')",
    )
    .bind(payment_id)
    .bind(order_id)
    .execute(&pool)
    .await?;

    send_webhook(
        &state,
        gateway_id,
        WebhookEventType::PaymentPartiallyRefunded,
        "webhook_refund_partial",
        Some("secondary_intent_1"),
        serde_json::json!({"amount": "10.00", "currency": "USD"}),
    )
    .await?;
    let partial: (String, String, String, Decimal, bool) = sqlx::query_as(
        "SELECT p.status::text, o.status::text, o.payment_status::text, r.amount, \
         r.settled_at IS NOT NULL FROM payments p JOIN orders o ON o.id = p.order_id \
         JOIN refunds r ON r.payment_id = p.id WHERE p.order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(
        partial
            == (
                "paid".to_string(),
                "confirmed".to_string(),
                "paid".to_string(),
                dec!(10.00),
                true
            )
    );

    send_webhook(
        &state,
        gateway_id,
        WebhookEventType::PaymentRefunded,
        "webhook_refund_full",
        Some("secondary_intent_1"),
        serde_json::json!({"amount": 30.00, "currency": "USD"}),
    )
    .await?;
    let full: (String, String, String, Decimal, i64) = sqlx::query_as(
        "SELECT p.status::text, o.status::text, o.payment_status::text, \
         SUM(r.amount), COUNT(*) FILTER (WHERE r.settled_at IS NULL) \
         FROM payments p JOIN orders o ON o.id = p.order_id \
         JOIN refunds r ON r.payment_id = p.id WHERE p.order_id = $1 \
         GROUP BY p.status, o.status, o.payment_status",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await?;
    ensure!(
        full == (
            "refunded".to_string(),
            "refunded".to_string(),
            "refunded".to_string(),
            dec!(40.00),
            0
        )
    );

    send_webhook(
        &state,
        gateway_id,
        WebhookEventType::PaymentRefunded,
        "webhook_refund_full",
        Some("secondary_intent_1"),
        serde_json::json!({"amount": 30.00, "currency": "USD"}),
    )
    .await?;
    let settled: Decimal = sqlx::query_scalar(
        "SELECT SUM(amount) FROM refunds WHERE payment_id = $1 AND status = 'refunded'",
    )
    .bind(payment_id)
    .fetch_one(&pool)
    .await?;
    ensure!(settled == dec!(40.00));
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn migration_022_upgrade_handles_legacy_duplicate_identifiers() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let schema = format!("payment_022_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await?;
    let mut connection = pool.acquire().await?;
    sqlx::query(&format!("SET search_path TO {schema}, public"))
        .execute(&mut *connection)
        .await?;
    sqlx::raw_sql(
        "CREATE TABLE customers (id UUID PRIMARY KEY, airwallex_customer_id VARCHAR(255)); \
         CREATE TABLE orders (id UUID PRIMARY KEY); \
         CREATE TABLE payments ( \
             id UUID PRIMARY KEY, order_id UUID NOT NULL REFERENCES orders(id), \
             gateway VARCHAR(50) NOT NULL, gateway_payment_id VARCHAR(255), \
             updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW() \
         );",
    )
    .execute(&mut *connection)
    .await?;
    let first_customer = Uuid::new_v4();
    let duplicate_customer = Uuid::new_v4();
    let unique_customer = Uuid::new_v4();
    for (customer_id, provider_id) in [
        (first_customer, "cus_duplicate"),
        (duplicate_customer, "cus_duplicate"),
        (unique_customer, "cus_unique"),
    ] {
        sqlx::query("INSERT INTO customers (id, airwallex_customer_id) VALUES ($1, $2)")
            .bind(customer_id)
            .bind(provider_id)
            .execute(&mut *connection)
            .await?;
    }
    let order_id = Uuid::new_v4();
    sqlx::query("INSERT INTO orders (id) VALUES ($1)")
        .bind(order_id)
        .execute(&mut *connection)
        .await?;
    for (payment_id, provider_id) in [
        (Uuid::new_v4(), "intent_duplicate"),
        (Uuid::new_v4(), "intent_duplicate"),
        (Uuid::new_v4(), "intent_unique"),
    ] {
        sqlx::query(
            "INSERT INTO payments (id, order_id, gateway, gateway_payment_id) \
             VALUES ($1, $2, 'airwallex', $3)",
        )
        .bind(payment_id)
        .bind(order_id)
        .bind(provider_id)
        .execute(&mut *connection)
        .await?;
    }

    sqlx::raw_sql(include_str!(
        "../../rcommerce-core/migrations/022_saved_payment_method_ownership.sql"
    ))
    .execute(&mut *connection)
    .await?;

    let mapped_duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payment_gateway_customers WHERE gateway_customer_id = 'cus_duplicate'",
    )
    .fetch_one(&mut *connection)
    .await?;
    let mapped_unique: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payment_gateway_customers \
         WHERE customer_id = $1 AND gateway_customer_id = 'cus_unique'",
    )
    .bind(unique_customer)
    .fetch_one(&mut *connection)
    .await?;
    ensure!(mapped_duplicate == 0);
    ensure!(mapped_unique == 1);
    let quarantined: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payment_identifier_conflicts \
         WHERE gateway_payment_id = 'intent_duplicate'",
    )
    .fetch_one(&mut *connection)
    .await?;
    let remaining_duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payments WHERE gateway_payment_id = 'intent_duplicate'",
    )
    .fetch_one(&mut *connection)
    .await?;
    ensure!(quarantined == 2);
    ensure!(remaining_duplicate == 0);
    let duplicate_insert = sqlx::query(
        "INSERT INTO payments (id, order_id, gateway, gateway_payment_id) \
         VALUES ($1, $2, 'airwallex', 'intent_unique')",
    )
    .bind(Uuid::new_v4())
    .bind(order_id)
    .execute(&mut *connection)
    .await;
    ensure!(duplicate_insert.is_err());

    sqlx::query("SET search_path TO public")
        .execute(&mut *connection)
        .await?;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut *connection)
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn cancelled_outcomes_finalize_orders_and_release_reservations() -> anyhow::Result<()> {
    let pool = test_pool().await?;
    let initiate_gateway = SecondaryGateway::default();
    let complete_gateway = SecondaryGateway::default();
    let poll_gateway = SecondaryGateway::default();
    *initiate_gateway.payment_id.lock().unwrap() = Some("cancel_initiate_intent".to_string());
    *complete_gateway.payment_id.lock().unwrap() = Some("cancel_complete_intent".to_string());
    *poll_gateway.payment_id.lock().unwrap() = Some("cancel_poll_intent".to_string());
    let mut payment_service = PaymentService::new("cancel-initiate".to_string());
    payment_service.register_gateway(
        "cancel-initiate".to_string(),
        Box::new(initiate_gateway.clone()),
    );
    payment_service.register_gateway(
        "cancel-complete".to_string(),
        Box::new(complete_gateway.clone()),
    );
    payment_service.register_gateway("cancel-poll".to_string(), Box::new(poll_gateway.clone()));
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(
        &pool,
        &format!("cancellation-{}@example.com", Uuid::new_v4()),
    )
    .await?;

    let initiate_order = create_order(&pool, Some(owner), "cancel@example.com").await?;
    create_active_reservation(&pool, initiate_order).await?;
    *initiate_gateway.cancel_initiation.lock().unwrap() = true;
    let response = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(
            initiate_order,
            "cancel-initiate",
            card_data(),
        )),
    )
    .await?
    .0;
    ensure!(matches!(
        response,
        InitiatePaymentResponse::Failed { ref error_code, .. } if error_code == "payment_cancelled"
    ));
    assert_cancelled_with_released_reservation(&pool, initiate_order).await?;

    let complete_order = create_order(&pool, Some(owner), "cancel@example.com").await?;
    create_active_reservation(&pool, complete_order).await?;
    let _ = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(
            complete_order,
            "cancel-complete",
            card_data(),
        )),
    )
    .await?;
    *complete_gateway.cancel_completion.lock().unwrap() = true;
    let response = complete_payment_action(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path("cancel-complete:cancel_complete_intent".to_string()),
        Json(CompletePaymentActionApiRequest {
            action_type: PaymentActionType::ThreeDSecure,
            action_data: serde_json::json!({}),
        }),
    )
    .await?
    .0;
    ensure!(matches!(
        response,
        CompletePaymentActionResponse::Failed { ref error_code, .. } if error_code == "payment_cancelled"
    ));
    assert_cancelled_with_released_reservation(&pool, complete_order).await?;

    let poll_order = create_order(&pool, Some(owner), "cancel@example.com").await?;
    create_active_reservation(&pool, poll_order).await?;
    let _ = initiate_payment(
        State(state.clone()),
        Extension(auth(owner)),
        Json(initiate_request(poll_order, "cancel-poll", card_data())),
    )
    .await?;
    *poll_gateway.status.lock().unwrap() = PaymentStatus::Cancelled;
    let response = get_payment_status(
        State(state.clone()),
        Extension(auth(owner)),
        axum::extract::Path("cancel-poll:cancel_poll_intent".to_string()),
    )
    .await?
    .0;
    ensure!(response.status == PaymentStatus::Cancelled);
    assert_cancelled_with_released_reservation(&pool, poll_order).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn succeeded_failed_and_cancelled_webhooks_preserve_refunded_payments() -> anyhow::Result<()>
{
    let pool = test_pool().await?;
    let gateway_id = "refunded-terminal-webhooks";
    let secondary = SecondaryGateway::default();
    let mut payment_service = PaymentService::new(gateway_id.to_string());
    payment_service.register_gateway(gateway_id.to_string(), Box::new(secondary));
    let state = test_app_state(pool.clone(), payment_service).await;
    let owner = create_customer(
        &pool,
        &format!("terminal-refund-{}@example.com", Uuid::new_v4()),
    )
    .await?;
    let order_id = create_paid_order(&state, &pool, owner, gateway_id).await?;
    let _ = refund_payment(
        State(state.clone()),
        Extension(admin()),
        axum::extract::Path(format!("{gateway_id}:secondary_intent_1")),
        Json(RefundRequest {
            amount: None,
            reason: "customer_request".to_string(),
        }),
    )
    .await?;

    for event_type in [
        WebhookEventType::PaymentSucceeded,
        WebhookEventType::PaymentFailed,
        WebhookEventType::PaymentCancelled,
    ] {
        send_webhook(
            &state,
            gateway_id,
            event_type,
            "secondary_intent_1",
            None,
            serde_json::json!({}),
        )
        .await?;
        let states: (String, String, String) = sqlx::query_as(
            "SELECT p.status::text, o.status::text, o.payment_status::text \
             FROM payments p JOIN orders o ON o.id = p.order_id WHERE p.order_id = $1",
        )
        .bind(order_id)
        .fetch_one(&pool)
        .await?;
        ensure!(
            states
                == (
                    "refunded".to_string(),
                    "refunded".to_string(),
                    "refunded".to_string()
                )
        );
    }
    Ok(())
}
