//! Cart Repository

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use crate::{
    models::{Cart, CartItem},
    Error, Result,
};

/// Cart repository trait
#[async_trait]
pub trait CartRepository: Send + Sync {
    /// Find cart by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Cart>>;

    /// Find active cart by customer ID
    async fn find_active_by_customer(&self, customer_id: Uuid) -> Result<Option<Cart>>;

    /// Find active cart by session token
    async fn find_active_by_session(&self, session_token: &str) -> Result<Option<Cart>>;

    /// Create a new cart
    async fn create(&self, cart: &Cart) -> Result<()>;

    /// Update cart
    async fn update(&self, cart: &Cart) -> Result<()>;

    /// Delete cart
    async fn delete(&self, id: Uuid) -> Result<()>;

    /// Assign customer to cart (for guest cart conversion)
    async fn assign_customer(&self, cart_id: Uuid, customer_id: Uuid) -> Result<()>;

    /// Mark cart as converted to order
    async fn mark_converted(&self, cart_id: Uuid, order_id: Option<Uuid>) -> Result<()>;

    /// Update cart expiration
    async fn update_expiration(&self, cart_id: Uuid, expires_at: DateTime<Utc>) -> Result<()>;

    /// Get cart items
    async fn get_items(&self, cart_id: Uuid) -> Result<Vec<CartItem>>;

    /// Find specific item in cart
    async fn find_item(
        &self,
        cart_id: Uuid,
        product_id: Uuid,
        variant_id: Option<Uuid>,
    ) -> Result<Option<CartItem>>;

    /// Find item by ID
    async fn find_item_by_id(&self, item_id: Uuid) -> Result<Option<CartItem>>;

    /// Add item to cart
    async fn add_item(&self, item: &CartItem) -> Result<()>;

    /// Update cart item
    async fn update_item(&self, item: &CartItem) -> Result<()>;

    /// Remove item from cart
    async fn remove_item(&self, item_id: Uuid) -> Result<()>;

    /// Clear all items from cart
    async fn clear_items(&self, cart_id: Uuid) -> Result<()>;
}

/// PostgreSQL implementation of CartRepository
pub struct PgCartRepository {
    pool: Pool<Postgres>,
}

impl PgCartRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CartRepository for PgCartRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Cart>> {
        let cart = sqlx::query_as::<_, Cart>(
            r#"
            SELECT * FROM carts WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(cart)
    }

    async fn find_active_by_customer(&self, customer_id: Uuid) -> Result<Option<Cart>> {
        let cart = sqlx::query_as::<_, Cart>(
            r#"
            SELECT * FROM carts 
            WHERE customer_id = $1 
            AND converted_to_order = false
            AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(customer_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(cart)
    }

    async fn find_active_by_session(&self, session_token: &str) -> Result<Option<Cart>> {
        let cart = sqlx::query_as::<_, Cart>(
            r#"
            SELECT * FROM carts 
            WHERE session_token = $1 
            AND converted_to_order = false
            AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(session_token)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(cart)
    }

    async fn create(&self, cart: &Cart) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO carts (
                id, customer_id, session_token, currency, subtotal, discount_total,
                tax_total, shipping_total, total, coupon_code, email, shipping_address_id,
                billing_address_id, shipping_method, notes, created_at, updated_at,
                expires_at, converted_to_order, order_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
            "#
        )
        .bind(cart.id)
        .bind(cart.customer_id)
        .bind(&cart.session_token)
        .bind(cart.currency)
        .bind(cart.subtotal)
        .bind(cart.discount_total)
        .bind(cart.tax_total)
        .bind(cart.shipping_total)
        .bind(cart.total)
        .bind(&cart.coupon_code)
        .bind(&cart.email)
        .bind(cart.shipping_address_id)
        .bind(cart.billing_address_id)
        .bind(&cart.shipping_method)
        .bind(&cart.notes)
        .bind(cart.created_at)
        .bind(cart.updated_at)
        .bind(cart.expires_at)
        .bind(cart.converted_to_order)
        .bind(cart.order_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn update(&self, cart: &Cart) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE carts SET
                customer_id = $2,
                session_token = $3,
                currency = $4,
                subtotal = $5,
                discount_total = $6,
                tax_total = $7,
                shipping_total = $8,
                total = $9,
                coupon_code = $10,
                email = $11,
                shipping_address_id = $12,
                billing_address_id = $13,
                shipping_method = $14,
                notes = $15,
                updated_at = $16,
                expires_at = $17,
                converted_to_order = $18,
                order_id = $19
            WHERE id = $1
            "#,
        )
        .bind(cart.id)
        .bind(cart.customer_id)
        .bind(&cart.session_token)
        .bind(cart.currency)
        .bind(cart.subtotal)
        .bind(cart.discount_total)
        .bind(cart.tax_total)
        .bind(cart.shipping_total)
        .bind(cart.total)
        .bind(&cart.coupon_code)
        .bind(&cart.email)
        .bind(cart.shipping_address_id)
        .bind(cart.billing_address_id)
        .bind(&cart.shipping_method)
        .bind(&cart.notes)
        .bind(cart.updated_at)
        .bind(cart.expires_at)
        .bind(cart.converted_to_order)
        .bind(cart.order_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM carts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn assign_customer(&self, cart_id: Uuid, customer_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE carts SET
                customer_id = $2,
                session_token = NULL,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(cart_id)
        .bind(customer_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn mark_converted(&self, cart_id: Uuid, order_id: Option<Uuid>) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE carts SET
                converted_to_order = true,
                order_id = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(cart_id)
        .bind(order_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn update_expiration(&self, cart_id: Uuid, expires_at: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE carts SET
                expires_at = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(cart_id)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn get_items(&self, cart_id: Uuid) -> Result<Vec<CartItem>> {
        let items = sqlx::query_as::<_, CartItem>(
            r#"
            SELECT * FROM cart_items WHERE cart_id = $1 ORDER BY created_at DESC
            "#,
        )
        .bind(cart_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(items)
    }

    async fn find_item(
        &self,
        cart_id: Uuid,
        product_id: Uuid,
        variant_id: Option<Uuid>,
    ) -> Result<Option<CartItem>> {
        let item = sqlx::query_as::<_, CartItem>(
            r#"
            SELECT * FROM cart_items 
            WHERE cart_id = $1 AND product_id = $2 AND (variant_id = $3 OR (variant_id IS NULL AND $3 IS NULL))
            "#
        )
        .bind(cart_id)
        .bind(product_id)
        .bind(variant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(item)
    }

    async fn find_item_by_id(&self, item_id: Uuid) -> Result<Option<CartItem>> {
        let item = sqlx::query_as::<_, CartItem>(
            r#"
            SELECT * FROM cart_items WHERE id = $1
            "#,
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(item)
    }

    async fn add_item(&self, item: &CartItem) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO cart_items (
                id, cart_id, product_id, variant_id, quantity, unit_price, original_price,
                subtotal, discount_amount, total, sku, title, variant_title, image_url,
                requires_shipping, is_gift_card, custom_attributes, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            "#
        )
        .bind(item.id)
        .bind(item.cart_id)
        .bind(item.product_id)
        .bind(item.variant_id)
        .bind(item.quantity)
        .bind(item.unit_price)
        .bind(item.original_price)
        .bind(item.subtotal)
        .bind(item.discount_amount)
        .bind(item.total)
        .bind(&item.sku)
        .bind(&item.title)
        .bind(&item.variant_title)
        .bind(&item.image_url)
        .bind(item.requires_shipping)
        .bind(item.is_gift_card)
        .bind(&item.custom_attributes)
        .bind(item.created_at)
        .bind(item.updated_at)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn update_item(&self, item: &CartItem) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE cart_items SET
                quantity = $2,
                unit_price = $3,
                subtotal = $4,
                discount_amount = $5,
                total = $6,
                custom_attributes = $7,
                updated_at = $8
            WHERE id = $1
            "#,
        )
        .bind(item.id)
        .bind(item.quantity)
        .bind(item.unit_price)
        .bind(item.subtotal)
        .bind(item.discount_amount)
        .bind(item.total)
        .bind(&item.custom_attributes)
        .bind(item.updated_at)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn remove_item(&self, item_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM cart_items WHERE id = $1")
            .bind(item_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn clear_items(&self, cart_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM cart_items WHERE cart_id = $1")
            .bind(cart_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }
}
