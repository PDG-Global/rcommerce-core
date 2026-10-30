use std::sync::Arc;

use anyhow::{anyhow, ensure};
use axum::{extract::State, http::StatusCode, Extension, Json};
use rcommerce_api::middleware::JwtAuth;
use rcommerce_api::routes::order::{
    create_order, CreateOrderItem, CreateOrderRequest, OrderResponse,
};
use rcommerce_api::state::{AppState, AppStateParams};
use rcommerce_core::inventory::{InventoryConfig, InventoryService};
use rcommerce_core::models::Address;
use rcommerce_core::order::lifecycle::OrderEventDispatcher;
use rcommerce_core::payment::agnostic::PaymentService;
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
use rcommerce_core::shipping::carriers::ManualProvider;
use rcommerce_core::shipping::ShippingProviderFactory;
use rcommerce_core::{Config, FileUploadService, StripeGateway};
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

async fn test_app_state(pool: PgPool) -> AppState {
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
    let mut shipping_factory = ShippingProviderFactory::new();
    shipping_factory.register(Box::new(ManualProvider::new()));
    let shipping_factory = Arc::new(shipping_factory);
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
        PaymentService::new("stripe".to_string()),
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
        "AUD".to_string(),
        None,
    ))
    .await
}

fn request(
    product_id: Uuid,
    customer_id: Option<Uuid>,
    customer_email: &str,
) -> CreateOrderRequest {
    CreateOrderRequest {
        customer_id,
        customer_email: customer_email.to_string(),
        billing_address_id: None,
        shipping_address_id: None,
        shipping_address: Some(Address {
            id: Uuid::new_v4(),
            customer_id: Uuid::nil(),
            first_name: "Test".to_string(),
            last_name: "Buyer".to_string(),
            company: None,
            phone: None,
            address1: "1 Rue de Test".to_string(),
            address2: None,
            city: "Paris".to_string(),
            state: None,
            country: "FR".to_string(),
            zip: "75001".to_string(),
            is_default_shipping: false,
            is_default_billing: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }),
        items: vec![CreateOrderItem {
            product_id,
            variant_id: None,
            quantity: 1,
        }],
        notes: None,
        coupon_code: None,
    }
}

async fn create(
    state: &AppState,
    auth: JwtAuth,
    request: CreateOrderRequest,
) -> anyhow::Result<OrderResponse> {
    match create_order(State(state.clone()), Extension(auth), Json(request)).await {
        Ok((status, Json(response))) => {
            ensure!(status == StatusCode::CREATED, "unexpected status: {status}");
            Ok(response)
        }
        Err((status, Json(body))) => Err(anyhow!("order creation failed ({status}): {body}")),
    }
}

fn assert_aud_totals(response: &OrderResponse) {
    assert_eq!(response.currency, "AUD");
    assert_eq!(response.subtotal, Decimal::from(100));
    assert_eq!(response.tax_total, Decimal::from(20));
    assert_eq!(response.shipping_total, Decimal::from(10));
    assert_eq!(response.total, Decimal::from(130));
}

async fn assert_stored(
    pool: &PgPool,
    order_id: Uuid,
    customer_id: Option<Uuid>,
    customer_email: &str,
) -> anyhow::Result<()> {
    let (stored_customer_id, stored_email, stored_currency): (Option<Uuid>, String, String) =
        sqlx::query_as(
            "SELECT customer_id, customer_email, currency::text FROM orders WHERE id = $1",
        )
        .bind(order_id)
        .fetch_one(pool)
        .await?;
    assert_eq!(stored_customer_id, customer_id);
    assert_eq!(stored_email, customer_email);
    assert_eq!(stored_currency, "AUD");
    Ok(())
}

#[tokio::test]
#[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
async fn direct_create_order_enforces_ownership_and_store_currency() -> anyhow::Result<()> {
    let database_url = std::env::var("TEST_DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    rcommerce_core::ensure_test_migrations(&pool).await?;
    let state = test_app_state(pool.clone()).await;

    let customer_id = Uuid::new_v4();
    let other_customer_id = Uuid::new_v4();
    let customer_email = format!("customer-{customer_id}@example.com");
    let other_customer_email = format!("other-{other_customer_id}@example.com");
    for (id, email) in [
        (customer_id, customer_email),
        (other_customer_id, other_customer_email),
    ] {
        sqlx::query("INSERT INTO customers (id, email, currency) VALUES ($1, $2, 'AUD')")
            .bind(id)
            .bind(email)
            .execute(&pool)
            .await?;
    }

    let product_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO products (id, title, slug, price, currency, inventory_quantity, is_active) VALUES ($1, 'AUD Test Product', $2, 100, 'AUD', 20, true)",
    )
    .bind(product_id)
    .bind(format!("aud-test-product-{product_id}"))
    .execute(&pool)
    .await?;

    let customer_auth = JwtAuth {
        customer_id,
        email: "customer@example.com".to_string(),
        permissions: vec!["read".to_string()],
    };
    let customer_order = create(
        &state,
        customer_auth,
        request(
            product_id,
            Some(other_customer_id),
            "spoof-attempt@example.com",
        ),
    )
    .await?;
    assert_eq!(customer_order.customer_id, Some(customer_id));
    assert_eq!(customer_order.customer_email, "spoof-attempt@example.com");
    assert_aud_totals(&customer_order);
    assert_stored(
        &pool,
        customer_order.id,
        Some(customer_id),
        "spoof-attempt@example.com",
    )
    .await?;

    let admin_auth = || JwtAuth {
        customer_id: Uuid::new_v4(),
        email: "admin@example.com".to_string(),
        permissions: vec!["admin".to_string()],
    };
    let on_behalf_order = create(
        &state,
        admin_auth(),
        request(product_id, Some(other_customer_id), "on-behalf@example.com"),
    )
    .await?;
    assert_eq!(on_behalf_order.customer_id, Some(other_customer_id));
    assert_aud_totals(&on_behalf_order);
    assert_stored(
        &pool,
        on_behalf_order.id,
        Some(other_customer_id),
        "on-behalf@example.com",
    )
    .await?;

    let guest_order = create(
        &state,
        admin_auth(),
        request(product_id, None, "guest@example.com"),
    )
    .await?;
    assert_eq!(guest_order.customer_id, None);
    assert_aud_totals(&guest_order);
    assert_stored(&pool, guest_order.id, None, "guest@example.com").await?;

    sqlx::query("DELETE FROM orders WHERE id = ANY($1)")
        .bind(vec![customer_order.id, on_behalf_order.id, guest_order.id])
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product_id)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM customers WHERE id = ANY($1)")
        .bind(vec![customer_id, other_customer_id])
        .execute(&pool)
        .await?;

    Ok(())
}
