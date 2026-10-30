use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rcommerce_core::{
    inventory::{InventoryConfig, InventoryService},
    order::{lifecycle::OrderEventDispatcher, OrderService, OrderStatus, PaymentStatus},
    payment::{
        CreatePaymentRequest, Payment, PaymentGateway, PaymentSession, Refund, RefundStatus,
        WebhookEvent,
    },
    repository::Database,
    Error, Result,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

struct TestGateway {
    id: &'static str,
    fail_refunds: bool,
    refund_calls: Arc<Mutex<Vec<(String, Option<Decimal>)>>>,
}

#[async_trait]
impl PaymentGateway for TestGateway {
    fn id(&self) -> &'static str {
        self.id
    }

    fn name(&self) -> &'static str {
        "Test Gateway"
    }

    async fn create_payment(&self, _request: CreatePaymentRequest) -> Result<PaymentSession> {
        Err(Error::payment_error("unexpected create call"))
    }

    async fn confirm_payment(&self, _payment_id: &str) -> Result<Payment> {
        Err(Error::payment_error("unexpected confirm call"))
    }

    async fn capture_payment(
        &self,
        _payment_id: &str,
        _amount: Option<Decimal>,
    ) -> Result<Payment> {
        Err(Error::payment_error("unexpected capture call"))
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<Refund> {
        if self.fail_refunds {
            return Err(Error::payment_error("default gateway must not be used"));
        }
        self.refund_calls
            .lock()
            .unwrap()
            .push((payment_id.to_string(), amount));
        Ok(Refund {
            id: format!("refund_{}", Uuid::new_v4()),
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(Decimal::new(100, 0)),
            currency: "USD".to_string(),
            status: RefundStatus::Succeeded,
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn get_payment(&self, _payment_id: &str) -> Result<Payment> {
        Err(Error::payment_error("unexpected get call"))
    }

    async fn handle_webhook(&self, _payload: &[u8], _signature: &str) -> Result<WebhookEvent> {
        Err(Error::payment_error("unexpected webhook call"))
    }
}

fn test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string())
}

async fn setup() -> Option<(PgPool, OrderService, Arc<Mutex<Vec<String>>>)> {
    let url = test_db_url();
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("Skipping order refund tests: cannot connect to {url}: {error}");
            return None;
        }
    };
    if let Err(error) = rcommerce_core::ensure_test_migrations(&pool).await {
        eprintln!("Skipping order refund tests: migration failed: {error}");
        return None;
    }

    let selected = Arc::new(Mutex::new(Vec::new()));
    let refund_calls = Arc::new(Mutex::new(Vec::new()));
    let secondary = Arc::new(TestGateway {
        id: "secondary",
        fail_refunds: false,
        refund_calls,
    });
    let resolver_selected = selected.clone();
    let resolver = Arc::new(move |gateway: &str| -> Option<Arc<dyn PaymentGateway>> {
        resolver_selected.lock().unwrap().push(gateway.to_string());
        Some(secondary.clone())
    });
    let default = TestGateway {
        id: "mock",
        fail_refunds: true,
        refund_calls: Arc::new(Mutex::new(Vec::new())),
    };
    let service = OrderService::new(
        Database::new(pool.clone()),
        Box::new(default),
        InventoryService::new(Database::new(pool.clone()), InventoryConfig::default()),
        OrderEventDispatcher::new(),
    )
    .with_payment_gateway_resolver(resolver);
    Some((pool, service, selected))
}

#[tokio::test]
async fn stored_gateway_and_cumulative_refunds_control_finalization() {
    let Some((pool, service, selected)) = setup().await else {
        return;
    };
    let order_id = Uuid::new_v4();
    let payment_id = Uuid::new_v4();
    let product_id = Uuid::new_v4();
    sqlx::query("INSERT INTO products (id, title, slug, price, inventory_quantity) VALUES ($1, 'Refund Test', $2, 50, 10)")
        .bind(product_id)
        .bind(format!("refund-test-{product_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO orders (id, order_number, customer_email, currency, subtotal, total, status, payment_status, fulfillment_status) VALUES ($1, $2, 'refund@example.com', 'USD', 100, 100, 'confirmed', 'paid', 'pending')",
    )
    .bind(order_id)
    .bind(format!("REFUND-{order_id}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO order_items (order_id, product_id, title, quantity, price, total) VALUES ($1, $2, 'Refund Test', 2, 50, 100)",
    )
    .bind(order_id)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO payments (id, order_id, amount, currency, status, gateway, gateway_payment_id) VALUES ($1, $2, 100, 'USD', 'paid', 'secondary', 'intent_secondary')",
    )
    .bind(payment_id)
    .bind(order_id)
    .execute(&pool)
    .await
    .unwrap();

    let partial = service
        .refund_order(order_id, Some(Decimal::new(40, 0)), "partial".to_string())
        .await
        .unwrap();
    assert_eq!(partial.status, OrderStatus::Confirmed);
    assert_eq!(partial.payment_status, PaymentStatus::Paid);
    let inventory: i32 =
        sqlx::query_scalar("SELECT inventory_quantity FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(inventory, 10);
    let available: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(available, 0);

    let second_product_id = Uuid::new_v4();
    sqlx::query("INSERT INTO products (id, title, slug, price, inventory_quantity) VALUES ($1, 'Failure Test', $2, 0, $3)")
        .bind(second_product_id)
        .bind(format!("failure-test-{second_product_id}"))
        .bind(i32::MAX)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO order_items (order_id, product_id, title, quantity, price, total) VALUES ($1, $2, 'Failure Test', 1, 0, 0)",
    )
    .bind(order_id)
    .bind(second_product_id)
    .execute(&pool)
    .await
    .unwrap();
    service
        .refund_order(order_id, Some(Decimal::new(60, 0)), "remainder".to_string())
        .await
        .expect_err("the second item must trigger a restock failure");
    let rolled_back = service.get_order(order_id).await.unwrap().unwrap();
    assert_eq!(rolled_back.order.status, OrderStatus::Confirmed);
    assert_eq!(rolled_back.order.payment_status, PaymentStatus::Paid);
    let inventory: i32 =
        sqlx::query_scalar("SELECT inventory_quantity FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(inventory, 10);
    let available: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(available, 0);

    sqlx::query("UPDATE products SET inventory_quantity = 0 WHERE id = $1")
        .bind(second_product_id)
        .execute(&pool)
        .await
        .unwrap();
    let refunded = service
        .refund_order(order_id, None, "retry after restock failure".to_string())
        .await
        .unwrap();
    assert_eq!(refunded.status, OrderStatus::Refunded);
    assert_eq!(refunded.payment_status, PaymentStatus::Refunded);
    let inventory: i32 =
        sqlx::query_scalar("SELECT inventory_quantity FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(inventory, 12);
    let available: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(available, 2);
    assert!(!service.reconcile_order_refund(order_id).await.unwrap());
    let inventory: i32 =
        sqlx::query_scalar("SELECT inventory_quantity FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(inventory, 12);
    let available: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(available, 2);

    let selected = selected.lock().unwrap();
    assert_eq!(selected.as_slice(), ["secondary", "secondary"]);
    drop(selected);
    sqlx::query("DELETE FROM orders WHERE id = $1")
        .bind(order_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM stock_movements WHERE product_id = ANY($1)")
        .bind(vec![product_id, second_product_id])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_levels WHERE product_id = ANY($1)")
        .bind(vec![product_id, second_product_id])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM products WHERE id = ANY($1)")
        .bind(vec![product_id, second_product_id])
        .execute(&pool)
        .await
        .unwrap();
}
