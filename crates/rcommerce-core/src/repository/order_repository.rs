//! Order Repository
//!
//! Database repository for order-related operations following the repository pattern.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    order::{FulfillmentStatus, Order, OrderItem, OrderStatus, PaymentStatus},
    Error, Result,
};

/// Order repository trait - database agnostic
#[async_trait]
pub trait OrderRepository: Send + Sync + 'static {
    /// Find order by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Order>>;

    /// Find order by order number
    async fn find_by_order_number(&self, order_number: &str) -> Result<Option<Order>>;

    /// List orders with filtering
    async fn list_orders(&self, filter: &OrderFilter) -> Result<Vec<Order>>;

    /// Count orders by filter
    async fn count_orders(&self, filter: &OrderFilter) -> Result<i64>;

    /// Create a new order
    async fn create_order(&self, order: &Order) -> Result<Order>;

    /// Update an order
    async fn update_order(&self, order: &Order) -> Result<Order>;

    /// Update order status
    async fn update_status(&self, id: Uuid, status: OrderStatus) -> Result<Order>;

    /// Update payment status
    async fn update_payment_status(&self, id: Uuid, status: PaymentStatus) -> Result<()>;

    /// Update fulfillment status
    async fn update_fulfillment_status(&self, id: Uuid, status: FulfillmentStatus) -> Result<()>;

    /// Delete an order
    async fn delete_order(&self, id: Uuid) -> Result<bool>;

    /// Get order items
    async fn get_order_items(&self, order_id: Uuid) -> Result<Vec<OrderItem>>;

    /// Create order item
    async fn create_order_item(&self, item: &OrderItem) -> Result<OrderItem>;

    /// Update order item
    async fn update_order_item(&self, item: &OrderItem) -> Result<OrderItem>;

    /// Get customer orders
    async fn get_customer_orders(
        &self,
        customer_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Order>>;

    /// Generate order number
    async fn generate_order_number(&self) -> Result<String>;
}

/// Filter parameters for listing orders
#[derive(Debug, Clone, Default)]
pub struct OrderFilter {
    pub customer_id: Option<Uuid>,
    pub status: Option<OrderStatus>,
    pub payment_status: Option<PaymentStatus>,
    pub fulfillment_status: Option<FulfillmentStatus>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub search: Option<String>,
}

/// PostgreSQL implementation of OrderRepository
pub struct PostgresOrderRepository {
    db: sqlx::PgPool,
}

impl PostgresOrderRepository {
    /// Create a new PostgreSQL order repository
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl OrderRepository for PostgresOrderRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Order>> {
        let order = sqlx::query_as::<_, Order>("SELECT * FROM orders WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch order: {}", e)))?;

        Ok(order)
    }

    async fn find_by_order_number(&self, order_number: &str) -> Result<Option<Order>> {
        let order = sqlx::query_as::<_, Order>("SELECT * FROM orders WHERE order_number = $1")
            .bind(order_number)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch order: {}", e)))?;

        Ok(order)
    }

    async fn list_orders(&self, filter: &OrderFilter) -> Result<Vec<Order>> {
        let mut sql = String::from("SELECT * FROM orders WHERE 1=1");
        let mut bind_idx = 0;

        if filter.customer_id.is_some() {
            bind_idx += 1;
            sql.push_str(&format!(" AND customer_id = ${}", bind_idx));
        }
        if filter.status.is_some() {
            bind_idx += 1;
            sql.push_str(&format!(" AND status = ${}", bind_idx));
        }
        if filter.payment_status.is_some() {
            bind_idx += 1;
            sql.push_str(&format!(" AND payment_status = ${}", bind_idx));
        }
        if filter.fulfillment_status.is_some() {
            bind_idx += 1;
            sql.push_str(&format!(" AND fulfillment_status = ${}", bind_idx));
        }
        if filter.date_from.is_some() {
            bind_idx += 1;
            sql.push_str(&format!(" AND created_at >= ${}", bind_idx));
        }
        if filter.date_to.is_some() {
            bind_idx += 1;
            sql.push_str(&format!(" AND created_at <= ${}", bind_idx));
        }

        sql.push_str(" ORDER BY created_at DESC");

        let mut query = sqlx::query_as::<_, Order>(&sql);

        if let Some(customer_id) = filter.customer_id {
            query = query.bind(customer_id);
        }
        if let Some(status) = &filter.status {
            query = query.bind(format!("{:?}", status).to_lowercase());
        }
        if let Some(payment_status) = &filter.payment_status {
            query = query.bind(format!("{:?}", payment_status).to_lowercase());
        }
        if let Some(fulfillment_status) = &filter.fulfillment_status {
            query = query.bind(format!("{:?}", fulfillment_status).to_lowercase());
        }
        if let Some(date_from) = filter.date_from {
            query = query.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            query = query.bind(date_to);
        }

        let orders = query
            .fetch_all(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch orders: {}", e)))?;

        Ok(orders)
    }

    async fn count_orders(&self, filter: &OrderFilter) -> Result<i64> {
        let mut sql = String::from("SELECT COUNT(*) FROM orders WHERE 1=1");
        let mut bind_idx = 0;

        if filter.customer_id.is_some() {
            bind_idx += 1;
            sql.push_str(&format!(" AND customer_id = ${}", bind_idx));
        }
        if filter.status.is_some() {
            bind_idx += 1;
            sql.push_str(&format!(" AND status = ${}", bind_idx));
        }

        let mut query = sqlx::query_scalar::<_, i64>(&sql);

        if let Some(customer_id) = filter.customer_id {
            query = query.bind(customer_id);
        }
        if let Some(status) = &filter.status {
            query = query.bind(format!("{:?}", status).to_lowercase());
        }

        let count = query
            .fetch_one(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to count orders: {}", e)))?;

        Ok(count)
    }

    async fn create_order(&self, order: &Order) -> Result<Order> {
        let order = sqlx::query_as::<_, Order>(
            r#"
            INSERT INTO orders (
                id, order_number, customer_id, customer_email,
                billing_address_id, shipping_address_id,
                status, fulfillment_status, payment_status,
                currency, subtotal, tax_total, shipping_total, discount_total, total,
                notes, tags, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            RETURNING *
            "#,
        )
        .bind(order.id)
        .bind(&order.order_number)
        .bind(order.customer_id)
        .bind(&order.customer_email)
        .bind(order.billing_address_id)
        .bind(order.shipping_address_id)
        .bind(format!("{:?}", order.status).to_lowercase())
        .bind(format!("{:?}", order.fulfillment_status).to_lowercase())
        .bind(format!("{:?}", order.payment_status).to_lowercase())
        .bind(&order.currency)
        .bind(order.subtotal)
        .bind(order.tax_total)
        .bind(order.shipping_total)
        .bind(order.discount_total)
        .bind(order.total)
        .bind(order.notes.as_ref())
        .bind(order.tags.clone().unwrap_or_default())
        .bind(&order.metadata)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create order: {}", e)))?;

        Ok(order)
    }

    async fn update_order(&self, order: &Order) -> Result<Order> {
        let order = sqlx::query_as::<_, Order>(
            r#"
            UPDATE orders 
            SET customer_email = $1,
                billing_address_id = $2,
                shipping_address_id = $3,
                status = $4,
                fulfillment_status = $5,
                payment_status = $6,
                subtotal = $7,
                tax_total = $8,
                shipping_total = $9,
                discount_total = $10,
                total = $11,
                notes = $12,
                tags = $13,
                metadata = $14,
                updated_at = NOW()
            WHERE id = $15
            RETURNING *
            "#,
        )
        .bind(&order.customer_email)
        .bind(order.billing_address_id)
        .bind(order.shipping_address_id)
        .bind(format!("{:?}", order.status).to_lowercase())
        .bind(format!("{:?}", order.fulfillment_status).to_lowercase())
        .bind(format!("{:?}", order.payment_status).to_lowercase())
        .bind(order.subtotal)
        .bind(order.tax_total)
        .bind(order.shipping_total)
        .bind(order.discount_total)
        .bind(order.total)
        .bind(order.notes.as_ref())
        .bind(order.tags.clone().unwrap_or_default())
        .bind(&order.metadata)
        .bind(order.id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update order: {}", e)))?;

        Ok(order)
    }

    async fn update_status(&self, id: Uuid, status: OrderStatus) -> Result<Order> {
        let order = sqlx::query_as::<_, Order>(
            "UPDATE orders SET status = $1::order_status, updated_at = NOW() WHERE id = $2 RETURNING *",
        )
        .bind(format!("{:?}", status).to_lowercase())
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update order status: {}", e)))?;

        Ok(order)
    }

    async fn update_payment_status(&self, id: Uuid, status: PaymentStatus) -> Result<()> {
        sqlx::query("UPDATE orders SET payment_status = $1, updated_at = NOW() WHERE id = $2")
            .bind(format!("{:?}", status).to_lowercase())
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to update payment status: {}", e)))?;

        Ok(())
    }

    async fn update_fulfillment_status(&self, id: Uuid, status: FulfillmentStatus) -> Result<()> {
        sqlx::query("UPDATE orders SET fulfillment_status = $1, updated_at = NOW() WHERE id = $2")
            .bind(format!("{:?}", status).to_lowercase())
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to update fulfillment status: {}", e)))?;

        Ok(())
    }

    async fn delete_order(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM orders WHERE id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to delete order: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_order_items(&self, order_id: Uuid) -> Result<Vec<OrderItem>> {
        let items = sqlx::query_as::<_, OrderItem>(
            "SELECT * FROM order_items WHERE order_id = $1 ORDER BY created_at",
        )
        .bind(order_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch order items: {}", e)))?;

        Ok(items)
    }

    async fn create_order_item(&self, item: &OrderItem) -> Result<OrderItem> {
        let item = sqlx::query_as::<_, OrderItem>(
            r#"
            INSERT INTO order_items (
                id, order_id, product_id, variant_id,
                quantity, price, subtotal, tax_amount, total,
                sku, name, variant_name, weight, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(item.id)
        .bind(item.order_id)
        .bind(item.product_id)
        .bind(item.variant_id)
        .bind(item.quantity)
        .bind(item.price)
        .bind(item.subtotal)
        .bind(item.tax_amount)
        .bind(item.total)
        .bind(item.sku.as_ref())
        .bind(&item.name)
        .bind(item.variant_name.as_ref())
        .bind(item.weight)
        .bind(&item.metadata)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create order item: {}", e)))?;

        Ok(item)
    }

    async fn update_order_item(&self, item: &OrderItem) -> Result<OrderItem> {
        let item = sqlx::query_as::<_, OrderItem>(
            r#"
            UPDATE order_items 
            SET quantity = $1,
                price = $2,
                subtotal = $3,
                tax_amount = $4,
                total = $5,
                metadata = $6,
                updated_at = NOW()
            WHERE id = $7
            RETURNING *
            "#,
        )
        .bind(item.quantity)
        .bind(item.price)
        .bind(item.subtotal)
        .bind(item.tax_amount)
        .bind(item.total)
        .bind(&item.metadata)
        .bind(item.id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update order item: {}", e)))?;

        Ok(item)
    }

    async fn get_customer_orders(
        &self,
        customer_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Order>> {
        let orders = sqlx::query_as::<_, Order>(
            "SELECT * FROM orders WHERE customer_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(customer_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch customer orders: {}", e)))?;

        Ok(orders)
    }

    async fn generate_order_number(&self) -> Result<String> {
        let prefix = "ORD";
        let timestamp = chrono::Utc::now().format("%Y%m%d");
        let random: u32 = rand::random();

        Ok(format!("{}-{}-{}", prefix, timestamp, random))
    }
}
