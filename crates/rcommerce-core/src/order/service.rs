//! Order Service
//!
//! Manages order lifecycle with integrated tax calculation, inventory management,
//! and payment processing. Coordinates with TaxService for accurate tax computation.

use std::sync::Arc;

use rust_decimal::Decimal;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::inventory::InventoryService;
use crate::models::Address;
use crate::order::lifecycle::OrderEventDispatcher;
use crate::order::{
    CreateOrderItem, CreateOrderRequest, Order, OrderItem, OrderStatus, PaymentStatus,
};
use crate::payment::PaymentGateway;
use crate::repository::Database;
use crate::tax::{
    CustomerTaxInfo, TaxAddress, TaxCalculation, TaxContext, TaxService, TaxableItem,
    TransactionType, VatId,
};
use crate::{Error, Result};

/// Product details needed when building an order item snapshot.
struct ProductOrderDetails {
    title: String,
    variant_title: Option<String>,
    sku: Option<String>,
}

/// Order service with integrated tax calculation
pub struct OrderService {
    db: Database,
    payment_gateway: Box<dyn PaymentGateway>,
    inventory_service: InventoryService,
    event_dispatcher: OrderEventDispatcher,
    tax_service: Option<Arc<dyn TaxService>>,
    /// Store code ([accounting].store_code) used to mint store-prefixed
    /// invoice numbers like "88-1024". None disables minting.
    store_code: Option<String>,
}

impl OrderService {
    /// Create a new order service
    pub fn new(
        db: Database,
        payment_gateway: Box<dyn PaymentGateway>,
        inventory_service: InventoryService,
        event_dispatcher: OrderEventDispatcher,
    ) -> Self {
        Self {
            db,
            payment_gateway,
            inventory_service,
            event_dispatcher,
            tax_service: None,
            store_code: None,
        }
    }

    /// Set the store code used for store-prefixed invoice numbers
    /// (e.g. "88" produces "88-1024" via the order_invoice_seq sequence).
    pub fn with_store_code(mut self, store_code: Option<String>) -> Self {
        self.store_code = store_code.filter(|s| !s.is_empty());
        self
    }

    /// Add tax service for tax calculation
    pub fn with_tax_service(mut self, tax_service: Arc<dyn TaxService>) -> Self {
        self.tax_service = Some(tax_service);
        self
    }

    /// Create a new order with tax calculation
    pub async fn create_order(&self, request: CreateOrderRequest) -> Result<Order> {
        info!("Creating order for customer {:?}", request.customer_id);

        // Validate customer if provided
        if let Some(customer_id) = request.customer_id {
            self.validate_customer(customer_id).await?;
        }

        // Validate addresses if provided
        if let Some(billing_id) = request.billing_address_id {
            self.validate_address(billing_id).await?;
        }

        if let Some(shipping_id) = request.shipping_address_id {
            self.validate_address(shipping_id).await?;
        }

        // Calculate taxes if tax service is available
        let tax_calculation = if let Some(ref _tax_service) = self.tax_service {
            // Try to get address info from metadata or use defaults
            let shipping_address = self.get_shipping_address_from_request(&request).await?;
            let billing_address = self.get_billing_address_from_request(&request).await?;
            let vat_id = request.metadata.get("vat_id").and_then(|v| v.as_str());

            match self
                .calculate_order_tax(
                    &request.items,
                    &shipping_address,
                    billing_address.as_ref(),
                    vat_id,
                    request.customer_id,
                )
                .await
            {
                Ok(calc) => Some(calc),
                Err(e) => {
                    warn!("Tax calculation failed, using provided tax_total: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Use calculated tax or fall back to provided values
        let tax_total = tax_calculation
            .as_ref()
            .map(|c| c.total_tax)
            .unwrap_or(request.tax_total);

        // Validate and reserve inventory for items
        let mut order_items = Vec::new();
        let mut subtotal = Decimal::ZERO;
        let mut total_item_tax = Decimal::ZERO;

        for item in request.items.iter() {
            // Fetch product/variant details and validate the product is active
            let details = self
                .get_product_order_details(item.product_id, item.variant_id)
                .await?;

            // Reserve inventory unless explicitly skipped (e.g. bundle parent lines)
            let reservation_id = if item.skip_inventory_reservation {
                None
            } else {
                let reservation = self
                    .inventory_service
                    .reserve_stock(crate::inventory::StockReservation::new(
                        item.product_id,
                        item.variant_id,
                        self.get_default_location_id().await?, // Support first active location
                        None, // Will be updated after order creation
                        item.quantity,
                        chrono::Utc::now() + chrono::Duration::minutes(30),
                    ))
                    .await?;
                Some(reservation.id)
            };

            // Calculate item totals
            let item_subtotal = item.price * Decimal::from(item.quantity);

            // Use calculated tax for this item if available
            let item_tax = tax_calculation
                .as_ref()
                .and_then(|calc| {
                    calc.line_items
                        .iter()
                        .find(|li| li.item_id == item.product_id) // Match by product_id
                        .map(|li| li.tax_amount)
                })
                .unwrap_or(item.tax_amount);

            let item_total = item_subtotal + item_tax;

            subtotal += item_subtotal;
            total_item_tax += item_tax;

            // Create order item
            let order_item = OrderItem {
                id: Uuid::new_v4(),
                order_id: Uuid::nil(), // Will be updated
                product_id: item.product_id,
                variant_id: item.variant_id,
                quantity: item.quantity,
                price: item.price,
                subtotal: item_subtotal,
                tax_amount: item_tax,
                total: item_total,
                sku: details.sku,
                name: details.title,
                variant_name: details.variant_title,
                weight: None, // TODO: Fetch from product
                weight_unit: None,
                is_bundle_component: None,
                metadata: if let Some(reservation_id) = reservation_id {
                    serde_json::json!({
                        "reservation_id": reservation_id,
                    })
                } else {
                    serde_json::json!({})
                },
                created_at: chrono::Utc::now(),
            };

            order_items.push(order_item);
        }

        // Map product ids to the real order item ids for tax reporting
        // (first occurrence wins for duplicate products). Built before the
        // order_items vec is consumed below.
        let mut tax_item_id_map: Vec<(Uuid, Uuid)> = Vec::new();
        for oi in &order_items {
            if !tax_item_id_map.iter().any(|(pid, _)| *pid == oi.product_id) {
                tax_item_id_map.push((oi.product_id, oi.id));
            }
        }

        // Calculate shipping tax
        let _shipping_tax = tax_calculation
            .as_ref()
            .map(|c| c.shipping_tax)
            .unwrap_or_default();

        // Total includes: subtotal + item tax + shipping + shipping tax - discount
        let total = subtotal + tax_total + request.shipping_total - request.discount_total;

        // Generate order number
        let order_number = self.generate_order_number().await?;

        // Create order
        let order_id = Uuid::new_v4();
        let order = sqlx::query_as::<_, Order>(
            r#"
            INSERT INTO orders (
                id, order_number, customer_id, customer_email,
                billing_address_id, shipping_address_id,
                billing_address, shipping_address,
                status, fulfillment_status, payment_status,
                currency, subtotal, tax_total, shipping_total, discount_total, total,
                notes, tags, metadata,
                coupon_code, coupon_id,
                invoice_number, test
            )
            VALUES (
                $1, $2, $3, $4,
                $5, $6,
                $7, $8,
                'pending', 'pending', 'pending',
                $9, $10, $11, $12, $13, $14,
                $15, $16, $17,
                $20, $21,
                CASE WHEN $18 = '' THEN NULL
                     ELSE $18 || '-' || nextval('order_invoice_seq') END,
                $19
            )
            RETURNING *
            "#,
        )
        .bind(order_id)
        .bind(order_number)
        .bind(request.customer_id)
        .bind(request.customer_email)
        .bind(request.billing_address_id)
        .bind(request.shipping_address_id)
        .bind(&request.billing_address)
        .bind(&request.shipping_address)
        .bind(request.currency)
        .bind(subtotal)
        .bind(tax_total)
        .bind(request.shipping_total)
        .bind(request.discount_total)
        .bind(total)
        .bind(request.notes)
        .bind(request.tags.unwrap_or_default())
        .bind(request.metadata)
        .bind(self.store_code.clone().unwrap_or_default())
        .bind(request.test)
        .bind(request.coupon_code)
        .bind(request.coupon_id)
        .fetch_one(self.db.pool())
        .await?;

        // Create order items
        for mut item in order_items {
            item.order_id = order_id;

            // Check for reservation before moving metadata
            let reservation_id: Option<Uuid> =
                if let serde_json::Value::Object(ref map) = &item.metadata {
                    map.get("reservation_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| Uuid::parse_str(s).ok())
                } else {
                    None
                };

            sqlx::query(
                r#"
                INSERT INTO order_items (
                    id, order_id, product_id, variant_id,
                    title, variant_title, sku,
                    quantity, price, subtotal, tax_amount, total,
                    weight, metadata
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                "#,
            )
            .bind(item.id)
            .bind(item.order_id)
            .bind(item.product_id)
            .bind(item.variant_id)
            .bind(&item.name)
            .bind(&item.variant_name)
            .bind(&item.sku)
            .bind(item.quantity)
            .bind(item.price)
            .bind(item.subtotal)
            .bind(item.tax_amount)
            .bind(item.total)
            .bind(item.weight)
            .bind(&item.metadata)
            .execute(self.db.pool())
            .await?;

            // Update reservation reference if found
            if let Some(reservation_id) = reservation_id {
                sqlx::query("UPDATE stock_reservations SET order_id = $1 WHERE id = $2")
                    .bind(order_id)
                    .bind(reservation_id)
                    .execute(self.db.pool())
                    .await?;
            }
        }

        // Record tax transaction for reporting if tax was calculated
        if let (Some(tax_service), Some(calculation)) = (&self.tax_service, tax_calculation) {
            if let Err(e) = tax_service
                .record_tax_transaction(order_id, &calculation, &tax_item_id_map)
                .await
            {
                warn!("Failed to record tax transaction: {}", e);
                // Don't fail order creation if tax recording fails
            }
        }

        // Dispatch order created event
        self.event_dispatcher.order_created(&order).await?;

        info!(
            "Order created: id={}, number={}, total={}",
            order_id, order.order_number, total
        );

        Ok(order)
    }

    /// Calculate tax for order items
    async fn calculate_order_tax(
        &self,
        items: &[CreateOrderItem],
        shipping_address: &Address,
        billing_address: Option<&Address>,
        vat_id: Option<&str>,
        customer_id: Option<Uuid>,
    ) -> Result<TaxCalculation> {
        let tax_service = self
            .tax_service
            .as_ref()
            .ok_or_else(|| Error::not_implemented("Tax service not configured"))?;

        // Convert order items to taxable items, resolving the real product/variant names
        let mut taxable_items = Vec::with_capacity(items.len());
        for item in items {
            let details = self
                .get_product_order_details(item.product_id, item.variant_id)
                .await?;

            taxable_items.push(TaxableItem {
                id: item.product_id, // Use product_id as item id for matching
                product_id: item.product_id,
                quantity: item.quantity,
                unit_price: item.price,
                total_price: item.price * Decimal::from(item.quantity),
                tax_category_id: None, // TODO: Get from product
                is_digital: false,     // TODO: Get from product
                title: details.title,
                sku: details.sku,
            });
        }

        // Build tax context
        let tax_context = TaxContext {
            customer: CustomerTaxInfo {
                customer_id,
                is_tax_exempt: false, // TODO: Check customer exemptions
                vat_id: vat_id.and_then(|v| VatId::parse(v).ok()),
                exemptions: vec![],
            },
            shipping_address: address_to_tax_address(shipping_address),
            billing_address: billing_address
                .map(address_to_tax_address)
                .unwrap_or_else(|| address_to_tax_address(shipping_address)),
            currency: crate::models::Currency::USD, // TODO: Get from request
            transaction_type: if vat_id.is_some() {
                TransactionType::B2B
            } else {
                TransactionType::B2C
            },
        };

        // Calculate tax
        let calculation = tax_service
            .calculate_tax(&taxable_items, &tax_context)
            .await?;

        debug!(
            "Order tax calculated: total_tax={}, items={}",
            calculation.total_tax,
            calculation.line_items.len()
        );

        Ok(calculation)
    }

    /// Get shipping address from request (from metadata or database)
    async fn get_shipping_address_from_request(
        &self,
        request: &CreateOrderRequest,
    ) -> Result<Address> {
        if let Some(address_id) = request.shipping_address_id {
            let address = sqlx::query_as::<_, Address>("SELECT * FROM addresses WHERE id = $1")
                .bind(address_id)
                .fetch_optional(self.db.pool())
                .await?;

            if let Some(addr) = address {
                return Ok(addr);
            }
        }

        // Return a default address if none found
        Ok(create_default_address())
    }

    /// Get billing address from request
    async fn get_billing_address_from_request(
        &self,
        request: &CreateOrderRequest,
    ) -> Result<Option<Address>> {
        if let Some(address_id) = request.billing_address_id {
            let address = sqlx::query_as::<_, Address>("SELECT * FROM addresses WHERE id = $1")
                .bind(address_id)
                .fetch_optional(self.db.pool())
                .await?;

            return Ok(address);
        }

        Ok(None)
    }

    /// Get order by ID
    pub async fn get_order(&self, order_id: Uuid) -> Result<Option<OrderDetail>> {
        let order = sqlx::query_as::<_, Order>("SELECT * FROM orders WHERE id = $1")
            .bind(order_id)
            .fetch_optional(self.db.pool())
            .await?;

        let order = match order {
            Some(o) => o,
            None => return Ok(None),
        };

        let items = self.get_order_items(order_id).await?;

        Ok(Some(OrderDetail {
            order: order.clone(),
            items,
        }))
    }

    /// The most recent order created for a cart. Checkout writes the cart id
    /// into `orders.metadata->>'cart_id'`; used to make checkout idempotent
    /// per cart so a client retry cannot mint a second order/payment.
    pub async fn find_latest_by_cart_id(&self, cart_id: Uuid) -> Result<Option<Order>> {
        let order = sqlx::query_as::<_, Order>(
            "SELECT * FROM orders WHERE metadata->>'cart_id' = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(cart_id.to_string())
        .fetch_optional(self.db.pool())
        .await?;

        Ok(order)
    }

    /// Gateway payment id (intent id) of the most recently recorded payment
    /// for an order, used to re-query the gateway on checkout replays.
    pub async fn latest_payment_gateway_id(&self, order_id: Uuid) -> Result<Option<String>> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT gateway_payment_id FROM payments WHERE order_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(order_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(id)
    }

    /// List orders with filtering
    pub async fn list_orders(&self, filter: super::OrderFilter) -> Result<Vec<Order>> {
        let mut query = String::from("SELECT * FROM orders WHERE 1=1");
        let mut param_count = 0;

        // Track which parameters we need to bind
        let has_customer = filter.customer_id.is_some();
        let has_status = filter.status.is_some();

        if has_customer {
            param_count += 1;
            query.push_str(&format!(" AND customer_id = ${}", param_count));
        }

        if has_status {
            param_count += 1;
            query.push_str(&format!(" AND status = ${}", param_count));
        }

        query.push_str(" ORDER BY created_at DESC");

        // Build query with explicit binds
        let mut query_builder = sqlx::query_as::<_, Order>(&query);

        if let Some(customer_id) = filter.customer_id {
            query_builder = query_builder.bind(customer_id);
        }
        if let Some(status) = filter.status {
            // Use the database enum label; the lifecycle enum's Debug spelling
            // ("canceled") does not match the Postgres value ("cancelled").
            query_builder = query_builder.bind(status_db_label(status));
        }

        let orders = query_builder.fetch_all(self.db.pool()).await?;

        Ok(orders)
    }

    /// Update order status
    pub async fn update_order_status(
        &self,
        order_id: Uuid,
        new_status: OrderStatus,
    ) -> Result<Order> {
        // Get current order
        let order = self
            .get_order(order_id)
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;

        // Validate transition
        if !self.is_valid_status_transition(order.order.status, new_status) {
            return Err(Error::validation(format!(
                "Invalid status transition from {:?} to {:?}",
                order.order.status, new_status
            )));
        }

        // Update status
        let updated_order = sqlx::query_as::<_, Order>(
            "UPDATE orders SET status = $1::order_status, updated_at = NOW() WHERE id = $2 RETURNING *",
        )
        .bind(status_db_label(new_status))
        .bind(order_id)
        .fetch_one(self.db.pool())
        .await?;

        // Dispatch status change event
        self.event_dispatcher
            .order_status_changed(&updated_order, order.order.status)
            .await?;

        Ok(updated_order)
    }

    /// Process payment for an order
    pub async fn process_payment(
        &self,
        order_id: Uuid,
        payment_method: crate::payment::CreatePaymentRequest,
    ) -> Result<crate::payment::Payment> {
        // Get order
        let order = self
            .get_order(order_id)
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;

        // Check if already paid
        if order.order.payment_status == PaymentStatus::Paid {
            return Err(Error::validation("Order already paid"));
        }

        // Create payment via gateway
        let payment = self.payment_gateway.create_payment(payment_method).await?;

        // Confirm payment
        let confirmed_payment = self.payment_gateway.confirm_payment(&payment.id).await?;

        // Update order payment status
        let new_payment_status = match confirmed_payment.status {
            crate::payment::PaymentStatus::Succeeded => PaymentStatus::Paid,
            crate::payment::PaymentStatus::Failed => PaymentStatus::Failed,
            _ => PaymentStatus::Pending,
        };

        sqlx::query("UPDATE orders SET payment_status = $1, updated_at = NOW() WHERE id = $2")
            .bind(format!("{:?}", new_payment_status).to_lowercase())
            .bind(order_id)
            .execute(self.db.pool())
            .await?;

        // If payment succeeded, update order status
        if new_payment_status == PaymentStatus::Paid {
            self.update_order_status(order_id, OrderStatus::Confirmed)
                .await?;
        }

        // Commit inventory reservations
        self.commit_inventory_reservations(order_id).await?;

        Ok(confirmed_payment)
    }

    /// Mark an order as paid after a successful external payment and commit inventory.
    pub async fn confirm_order_after_payment(&self, order_id: Uuid) -> Result<OrderDetail> {
        // Only transition orders still awaiting payment; a racing duplicate
        // (webhook vs client confirm) finds no row to update and falls through
        // to the current order, keeping this idempotent.
        let updated = sqlx::query_as::<_, Order>(
            r#"
            UPDATE orders
            SET status = 'confirmed',
                payment_status = 'paid',
                updated_at = NOW()
            WHERE id = $1 AND status = 'pending'
            RETURNING *
            "#,
        )
        .bind(order_id)
        .fetch_optional(self.db.pool())
        .await?;
        let order = match updated {
            Some(order) => order,
            // Already confirmed by a racing webhook/client confirm; return the
            // current row so this stays idempotent.
            None => {
                sqlx::query_as::<_, Order>("SELECT * FROM orders WHERE id = $1")
                    .bind(order_id)
                    .fetch_one(self.db.pool())
                    .await?
            }
        };

        // Record coupon usage once the payment is confirmed; racing callers
        // are deduplicated by the NOT EXISTS guard inside.
        if let Err(e) = self.record_coupon_usage(&order).await {
            warn!("Failed to record coupon usage for order {}: {}", order_id, e);
        }

        self.commit_inventory_reservations(order_id).await?;
        let items = self.get_order_items(order_id).await?;

        Ok(OrderDetail { order, items })
    }
    ///
    /// Record coupon usage for a newly confirmed order (idempotent).
    ///
    /// Runs at payment confirmation — not at order creation — so abandoned
    /// orders never burn single-use codes. Safe to call from racing paths
    /// (the Airwallex webhook vs the confirm endpoint): the INSERT is guarded
    /// by NOT EXISTS on the order_id, so exactly one usage row is written and
    /// usage_count increments once. Failures are logged, never propagated.
    async fn record_coupon_usage(&self, order: &Order) -> Result<()> {
        use rust_decimal::Decimal;

        if order.discount_total <= Decimal::ZERO {
            return Ok(());
        }

        // Resolve the coupon: prefer the id persisted on the order, then the
        // code, then (for orders created before the coupon was persisted on
        // the order row) the cart's coupon code via the order's cart_id
        // metadata, backfilling the order row defensively.
        let mut coupon_id = order.coupon_id;
        let mut coupon_code = order.coupon_code.clone();

        if coupon_code.is_none() {
            if let Some(code) = self.cart_coupon_code_for_order(order).await? {
                sqlx::query(
                    "UPDATE orders SET coupon_code = $1, updated_at = NOW() \
                     WHERE id = $2 AND coupon_code IS NULL",
                )
                .bind(&code)
                .bind(order.id)
                .execute(self.db.pool())
                .await?;
                coupon_code = Some(code);
            }
        }

        if coupon_id.is_none() {
            if let Some(ref code) = coupon_code {
                coupon_id =
                    sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM coupons WHERE code = $1")
                        .bind(code)
                        .fetch_optional(self.db.pool())
                        .await?;
            }
        }

        let Some(coupon_id) = coupon_id else {
            if coupon_code.is_some() {
                warn!(
                    "Order {} has discount {} and coupon code {:?} but no matching coupon; \
                     usage not recorded",
                    order.id, order.discount_total, coupon_code
                );
            }
            return Ok(());
        };

        let inserted = sqlx::query(
            r#"
            INSERT INTO coupon_usages (id, coupon_id, customer_id, order_id, discount_amount, used_at)
            SELECT $1, $2, $3, $4, $5, NOW()
            WHERE NOT EXISTS (SELECT 1 FROM coupon_usages WHERE order_id = $4)
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(coupon_id)
        .bind(order.customer_id)
        .bind(order.id)
        .bind(order.discount_total)
        .execute(self.db.pool())
        .await?;

        if inserted.rows_affected() > 0 {
            sqlx::query(
                "UPDATE coupons SET usage_count = usage_count + 1, updated_at = NOW() WHERE id = $1",
            )
            .bind(coupon_id)
            .execute(self.db.pool())
            .await?;
            info!(
                "Recorded coupon usage: coupon={} order={} amount={}",
                coupon_id, order.id, order.discount_total
            );
        }

        Ok(())
    }

    /// Coupon code of the cart this order was checked out from, if any.
    async fn cart_coupon_code_for_order(&self, order: &Order) -> Result<Option<String>> {
        let cart_id = order
            .metadata
            .get("cart_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok());
        let Some(cart_id) = cart_id else {
            return Ok(None);
        };
        let code: Option<String> =
            sqlx::query_scalar("SELECT coupon_code FROM carts WHERE id = $1")
                .bind(cart_id)
                .fetch_optional(self.db.pool())
                .await?;
        Ok(code)
    }

    /// Cancel orders that stayed `pending` with an unfinished payment for
    /// longer than the given cutoff, releasing their inventory via the normal
    /// cancellation path. Returns the cancelled order IDs; per-order failures
    /// are logged and skipped.
    pub async fn cancel_abandoned_orders(&self, older_than_minutes: u64) -> Result<Vec<Uuid>> {
        // Enum columns need explicit casts; make_interval's `mins` argument
        // is integer, so clamp to i32 range.
        const ABANDONED_ORDERS_SQL: &str = "SELECT id FROM orders \
             WHERE draft = false AND status = 'pending'::order_status \
               AND payment_status IN ('pending'::payment_status, 'failed'::payment_status) \
               AND created_at < NOW() - make_interval(mins => $1)";

        let candidates: Vec<Uuid> = sqlx::query_scalar(ABANDONED_ORDERS_SQL)
            .bind(older_than_minutes.min(i32::MAX as u64) as i32)
            .fetch_all(self.db.pool())
            .await?;

        let mut cancelled = Vec::new();
        for id in candidates {
            match self
                .cancel_order(id, "Abandoned: payment not completed".to_string())
                .await
            {
                Ok(_) => {
                    info!("Auto-cancelled abandoned order {}", id);
                    cancelled.push(id);
                }
                Err(e) => {
                    warn!("Failed to auto-cancel abandoned order {}: {}", id, e);
                }
            }
        }
        Ok(cancelled)
    }

    /// Cancel an order
    pub async fn cancel_order(&self, order_id: Uuid, reason: String) -> Result<Order> {
        // Get order
        let order = self
            .get_order(order_id)
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;

        // Check if can be canceled
        if !self.can_cancel_order(&order.order) {
            return Err(Error::validation(
                "Order cannot be canceled in current status",
            ));
        }

        // Release inventory reservations
        self.release_inventory_reservations(order_id).await?;

        // Update cancellation reason in metadata first (before reason is consumed)
        sqlx::query("UPDATE orders SET metadata = jsonb_set(metadata, '{cancellation_reason}', $1) WHERE id = $2")
            .bind(serde_json::json!(&reason))
            .bind(order_id)
            .execute(self.db.pool())
            .await?;

        // Process refund if payment was captured
        if order.order.payment_status == PaymentStatus::Paid {
            self.process_refund(order_id, None, reason.clone()).await?;
        } else if order.order.payment_status == PaymentStatus::Authorized {
            // Attempt to void/cancel the authorized (uncaptured) payment via the
            // gateway. The legacy gateway trait has no dedicated void operation, so
            // a refund request is used; a failure here must not block cancellation.
            match self.process_refund(order_id, None, reason.clone()).await {
                Ok(_) => info!("Voided authorized payment for canceled order {}", order_id),
                Err(e) => warn!(
                    "Failed to void authorized payment for order {}: {}",
                    order_id, e
                ),
            }
        }

        // Update order status
        let canceled_order = self
            .update_order_status(order_id, OrderStatus::Canceled)
            .await?;

        Ok(canceled_order)
    }

    /// Refund a paid (or authorized) order through the payment gateway,
    /// restock its items, and mark the order as refunded.
    pub async fn refund_order(
        &self,
        order_id: Uuid,
        amount: Option<Decimal>,
        reason: String,
    ) -> Result<Order> {
        let detail = self
            .get_order(order_id)
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;

        // Only orders with captured or authorized funds can be refunded
        match detail.order.payment_status {
            PaymentStatus::Paid | PaymentStatus::Authorized => {}
            _ => {
                return Err(Error::validation(
                    "Order payment is not in a refundable state",
                ));
            }
        }

        // Process the refund through the gateway using the recorded transaction id
        let refund = self
            .process_refund(order_id, amount, reason.clone())
            .await?;

        // Restock the ordered items
        for item in &detail.items {
            sqlx::query(
                "UPDATE products SET inventory_quantity = inventory_quantity + $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(item.quantity)
            .bind(item.product_id)
            .execute(self.db.pool())
            .await?;
        }

        // Update payment status
        sqlx::query(
            "UPDATE orders SET payment_status = 'refunded', updated_at = NOW() WHERE id = $1",
        )
        .bind(order_id)
        .execute(self.db.pool())
        .await?;

        // Update order status (validated against allowed transitions)
        let refunded_order = self
            .update_order_status(order_id, OrderStatus::Refunded)
            .await?;

        info!(
            "Order refunded: id={}, amount={}, refund_id={}",
            order_id, refund.amount, refund.id
        );

        // Dispatch refund event
        self.event_dispatcher
            .order_refunded(&refunded_order, refund.id, refund.amount, reason)
            .await?;

        Ok(refunded_order)
    }

    /// Fulfill an order
    pub async fn fulfill_order(&self, order_id: Uuid) -> Result<crate::order::Fulfillment> {
        // Get order
        let order = self
            .get_order(order_id)
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;

        // Check if can be fulfilled
        if order.order.status != OrderStatus::Confirmed {
            return Err(Error::validation(
                "Order must be confirmed before fulfillment",
            ));
        }

        // Create fulfillment record
        let fulfillment_id = Uuid::new_v4();
        let fulfillment = sqlx::query_as::<_, crate::order::Fulfillment>(
            r#"
            INSERT INTO fulfillments (id, order_id, status, tracking_number, tracking_url, tracking_company)
            VALUES ($1, $2, 'processing', NULL, NULL, NULL)
            RETURNING *
            "#
        )
        .bind(fulfillment_id)
        .bind(order_id)
        .fetch_one(self.db.pool())
        .await?;

        // Update order fulfillment status
        sqlx::query("UPDATE orders SET fulfillment_status = 'processing' WHERE id = $1")
            .bind(order_id)
            .execute(self.db.pool())
            .await?;

        // Dispatch fulfillment created event
        self.event_dispatcher
            .fulfillment_created(&fulfillment)
            .await?;

        Ok(fulfillment)
    }

    // Helper methods

    async fn validate_customer(&self, customer_id: Uuid) -> Result<()> {
        sqlx::query("SELECT 1 FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_optional(self.db.pool())
            .await?
            .ok_or_else(|| Error::not_found("Customer not found"))?;

        Ok(())
    }

    async fn validate_address(&self, address_id: Uuid) -> Result<()> {
        sqlx::query("SELECT 1 FROM addresses WHERE id = $1")
            .bind(address_id)
            .fetch_optional(self.db.pool())
            .await?
            .ok_or_else(|| Error::not_found("Address not found"))?;

        Ok(())
    }

    async fn get_product_order_details(
        &self,
        product_id: Uuid,
        variant_id: Option<Uuid>,
    ) -> Result<ProductOrderDetails> {
        if let Some(variant_id) = variant_id {
            let row = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
                r#"
                SELECT p.title, v.title, v.sku
                FROM products p
                JOIN product_variants v ON p.id = v.product_id
                WHERE p.id = $1 AND p.is_active = true AND v.id = $2
                "#,
            )
            .bind(product_id)
            .bind(variant_id)
            .fetch_optional(self.db.pool())
            .await?
            .ok_or_else(|| Error::not_found("Product or variant not found"))?;

            Ok(ProductOrderDetails {
                title: row.0,
                variant_title: row.1,
                sku: row.2,
            })
        } else {
            let row = sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT title, sku FROM products WHERE id = $1 AND is_active = true",
            )
            .bind(product_id)
            .fetch_optional(self.db.pool())
            .await?
            .ok_or_else(|| Error::not_found("Product not found or inactive"))?;

            Ok(ProductOrderDetails {
                title: row.0,
                variant_title: None,
                sku: row.1,
            })
        }
    }

    async fn get_order_items(&self, order_id: Uuid) -> Result<Vec<crate::order::OrderItem>> {
        let items = sqlx::query_as::<_, crate::order::OrderItem>(
            "SELECT * FROM order_items WHERE order_id = $1 ORDER BY created_at",
        )
        .bind(order_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(items)
    }

    /// Record a payment for an order (idempotent on gateway + transaction id)
    pub async fn record_payment(
        &self,
        order_id: Uuid,
        payment: &crate::payment::Payment,
    ) -> Result<()> {
        use crate::payment::PaymentStatus;

        let status = match payment.status {
            PaymentStatus::Succeeded => "paid",
            PaymentStatus::Pending | PaymentStatus::Processing => "pending",
            PaymentStatus::Failed => "failed",
            PaymentStatus::Canceled => "cancelled",
            PaymentStatus::Refunded => "refunded",
            PaymentStatus::Disputed => "pending",
        };

        sqlx::query(
            r#"
            INSERT INTO payments (
                order_id, amount, currency, status, gateway,
                gateway_payment_id, payment_method, card_last_four, card_brand, processed_at, test
            )
            SELECT $1, $2, UPPER($3)::currency, $4::payment_status, $5, $6, 'credit_card', $7, $8, NOW(),
                   (SELECT test FROM orders WHERE id = $1)
            WHERE NOT EXISTS (
                SELECT 1 FROM payments WHERE gateway = $5 AND gateway_payment_id = $6
            )
            "#,
        )
        .bind(order_id)
        .bind(payment.amount)
        .bind(&payment.currency)
        .bind(status)
        .bind(&payment.gateway)
        .bind(&payment.transaction_id)
        .bind(&payment.card_last_four)
        .bind(&payment.card_brand)
        .execute(self.db.pool())
        .await?;

        Ok(())
    }

    fn is_valid_status_transition(&self, from: OrderStatus, to: OrderStatus) -> bool {
        use OrderStatus::*;

        match (from, to) {
            // Can go from pending to confirmed (payment received)
            (Pending, Confirmed) => true,
            // Can go from confirmed to processing (being prepared)
            (Confirmed, Processing) => true,
            // Can go from processing to shipped (sent to customer)
            (Processing, Shipped) => true,
            // Can go from shipped to delivered (customer received)
            (Shipped, Delivered) => true,
            // Can go from delivered to completed (all done)
            (Delivered, Completed) => true,
            // Can cancel from most states
            (_, Canceled) => true,
            // Can refund from paid states
            (_, Refunded) => true,
            // Otherwise invalid
            _ => false,
        }
    }

    fn can_cancel_order(&self, order: &Order) -> bool {
        use OrderStatus::*;

        matches!(order.status, Pending | Confirmed | Processing)
    }

    async fn generate_order_number(&self) -> Result<String> {
        // Simple incrementing order number
        // In production, use a more sophisticated system
        let prefix = "ORD";
        let timestamp = chrono::Utc::now().format("%Y%m%d");
        let random: u32 = rand::random();

        Ok(format!("{}-{}-{}", prefix, timestamp, random))
    }

    async fn get_default_location_id(&self) -> Result<Uuid> {
        // Use the first active inventory location as the default fulfillment location.
        let existing = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM inventory_locations WHERE is_active = true ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(self.db.pool())
        .await?;

        if let Some(id) = existing {
            return Ok(id);
        }

        // No location exists (e.g. legacy DB that pre-dates the default
        // warehouse migration) — create one rather than failing order
        // creation, mirroring the checkout service's behaviour.
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO inventory_locations (id, name, code) VALUES ($1, 'Default Warehouse', $2)")
            .bind(id)
            .bind(format!("MAIN-{}", &id.to_string()[..8]))
            .execute(self.db.pool())
            .await?;
        Ok(id)
    }

    async fn commit_inventory_reservations(&self, order_id: Uuid) -> Result<()> {
        let reservations = sqlx::query_as::<_, crate::inventory::StockReservation>(
            "SELECT * FROM stock_reservations WHERE order_id = $1 AND status = 'active'",
        )
        .bind(order_id)
        .fetch_all(self.db.pool())
        .await?;

        for reservation in reservations {
            self.inventory_service
                .commit_reservation(reservation.id)
                .await?;
        }

        Ok(())
    }

    async fn release_inventory_reservations(&self, order_id: Uuid) -> Result<()> {
        let reservations = sqlx::query_as::<_, crate::inventory::StockReservation>(
            "SELECT * FROM stock_reservations WHERE order_id = $1 AND status IN ('active', 'committed')"
        )
        .bind(order_id)
        .fetch_all(self.db.pool())
        .await?;

        for reservation in reservations {
            self.inventory_service
                .release_reservation(reservation.id)
                .await?;
        }

        Ok(())
    }

    /// Refund (or void) the most recent paid/authorized payment recorded for an
    /// order, using the gateway transaction id stored on the payment record.
    async fn process_refund(
        &self,
        order_id: Uuid,
        amount: Option<Decimal>,
        reason: String,
    ) -> Result<crate::payment::Refund> {
        // Find the latest refundable payment record for this order
        let payment = sqlx::query_as::<_, RefundablePaymentRow>(
            r#"
            SELECT id, gateway, gateway_payment_id, currency::text AS currency_text
            FROM payments
            WHERE order_id = $1
              AND gateway_payment_id IS NOT NULL
              AND status IN ('paid', 'authorized')
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(order_id)
        .fetch_optional(self.db.pool())
        .await?
        .ok_or_else(|| Error::validation("No refundable payment found for order"))?;

        let gateway_payment_id = payment.gateway_payment_id.clone();

        info!(
            "Refunding payment {} (gateway: {}) for order {}: {}",
            gateway_payment_id, payment.gateway, order_id, reason
        );

        // Refund through the configured payment gateway
        let refund = self
            .payment_gateway
            .refund_payment(&gateway_payment_id, amount, &reason)
            .await?;

        // Mark the payment record as refunded
        sqlx::query(
            "UPDATE payments SET status = 'refunded' WHERE gateway = $1 AND gateway_payment_id = $2",
        )
        .bind(&payment.gateway)
        .bind(&gateway_payment_id)
        .execute(self.db.pool())
        .await?;

        // Record the refund so order detail APIs and accounting exports can
        // report it; skip if a row with the same gateway refund id exists
        // (e.g. a webhook raced us to it).
        let refund_status = match refund.status {
            crate::payment::RefundStatus::Succeeded => "refunded",
            crate::payment::RefundStatus::Failed => "failed",
            _ => "pending",
        };
        let settled_at = match refund.status {
            crate::payment::RefundStatus::Succeeded => Some(refund.created_at),
            _ => None,
        };
        sqlx::query(
            r#"
            INSERT INTO refunds (id, payment_id, order_id, amount, currency, reason, status,
                                 gateway_refund_id, settled_at, created_at, updated_at)
            SELECT $1, $2, $3, $4, UPPER($5)::currency, $6, $7::payment_status, $8, $9, NOW(), NOW()
            WHERE NOT EXISTS (SELECT 1 FROM refunds WHERE gateway_refund_id = $8)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(payment.id)
        .bind(order_id)
        .bind(refund.amount)
        .bind(&refund.currency)
        .bind(&reason)
        .bind(refund_status)
        .bind(&refund.id)
        .bind(settled_at)
        .execute(self.db.pool())
        .await?;

        Ok(refund)
    }
}

/// Payment row eligible for refund via the order refund path.
#[derive(Debug, sqlx::FromRow)]
struct RefundablePaymentRow {
    id: Uuid,
    gateway: String,
    gateway_payment_id: String,
    currency_text: String,
}

/// Map a lifecycle order status to its database enum label.
///
/// The lifecycle enum uses American spelling (`Canceled`) while the Postgres
/// `order_status` enum uses British spelling (`cancelled`); always persist the
/// database label.
fn status_db_label(status: OrderStatus) -> &'static str {
    use OrderStatus::*;

    match status {
        Pending => "pending",
        Confirmed => "confirmed",
        Processing => "processing",
        OnHold => "on_hold",
        Shipped => "shipped",
        Delivered => "delivered",
        Completed => "completed",
        Canceled => "cancelled",
        Refunded => "refunded",
    }
}

#[async_trait::async_trait]
impl crate::services::Service for OrderService {
    async fn health_check(&self) -> Result<()> {
        // Check database connectivity
        let _ = sqlx::query("SELECT 1").fetch_one(self.db.pool()).await?;
        Ok(())
    }
}

/// Order detail with related data
#[derive(Debug, Clone)]
pub struct OrderDetail {
    pub order: Order,
    pub items: Vec<OrderItem>,
}

/// Convert Address to TaxAddress
fn address_to_tax_address(address: &Address) -> TaxAddress {
    TaxAddress {
        country_code: address.country.clone(),
        region_code: address.state.clone(),
        postal_code: Some(address.zip.clone()),
        city: Some(address.city.clone()),
    }
}

/// Create a default address for tax calculation fallback
fn create_default_address() -> Address {
    Address {
        id: Uuid::nil(),
        customer_id: Uuid::nil(),
        first_name: "Default".to_string(),
        last_name: "Address".to_string(),
        company: None,
        phone: None,
        address1: "123 Default St".to_string(),
        address2: None,
        city: "Los Angeles".to_string(),
        state: Some("CA".to_string()),
        country: "US".to_string(),
        zip: "90210".to_string(),
        is_default_shipping: false,
        is_default_billing: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_abandoned_orders_sql_uses_enum_casts() {
        // The sweeper query must cast string literals against the enum
        // columns; plain string binds against order_status/payment_status
        // fail at runtime (see the earlier enum-cast fixes).
        let src = include_str!("service.rs");
        let query = src
            .split("ABANDONED_ORDERS_SQL")
            .nth(1)
            .expect("sweeper SQL not found");
        assert!(
            query.contains("'pending'::order_status"),
            "status cast missing"
        );
        assert!(
            query.contains("'pending'::payment_status")
                && query.contains("'failed'::payment_status"),
            "payment_status casts missing"
        );
    }

    use super::*;

    #[test]
    fn test_is_valid_status_transition() {
        // Test the OrderStatus transitions directly without needing a database
        use OrderStatus::*;

        assert!(Pending.can_transition_to(Confirmed));
        assert!(Confirmed.can_transition_to(Processing));
        assert!(Processing.can_transition_to(Shipped));
        assert!(!Pending.can_transition_to(Shipped));
        assert!(Pending.can_transition_to(Canceled));
        assert!(Shipped.can_transition_to(Delivered));
        assert!(Delivered.can_transition_to(Completed));
        assert!(!Completed.can_transition_to(Canceled));
    }

    #[test]
    fn test_address_conversion() {
        let address = Address {
            id: Uuid::new_v4(),
            customer_id: Uuid::new_v4(),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            company: None,
            phone: None,
            address1: "123 Main St".to_string(),
            address2: None,
            city: "Berlin".to_string(),
            state: Some("BE".to_string()),
            country: "DE".to_string(),
            zip: "10115".to_string(),
            is_default_shipping: true,
            is_default_billing: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let tax_addr = address_to_tax_address(&address);
        assert_eq!(tax_addr.country_code, "DE");
        assert_eq!(tax_addr.region_code, Some("BE".to_string()));
        assert_eq!(tax_addr.postal_code, Some("10115".to_string()));
    }
}
