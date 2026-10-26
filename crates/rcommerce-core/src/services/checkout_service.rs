//! Checkout Service
//!
//! Orchestrates the complete checkout flow including cart validation,
//! shipping calculation, tax calculation, order creation, and payment processing.
//! This service integrates Cart, Tax, Shipping, Order, and Payment services.

use std::sync::Arc;

use chrono::Utc;
use rust_decimal::Decimal;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    models::{Address, Cart, CartItem, Currency},
    order::service::OrderDetail,
    order::{CreateOrderItem, CreateOrderRequest, OrderService},
    payment::{CreatePaymentRequest, PaymentGateway, PaymentMethod},
    services::CartService,
    shipping::{Package, RateOptions, ShippingProviderFactory, ShippingRate},
    tax::{
        CustomerTaxInfo, TaxAddress, TaxCalculation, TaxContext, TaxService, TaxableItem,
        TransactionType, VatId,
    },
    Error, Result,
};

/// Checkout service that orchestrates the complete checkout flow
pub struct CheckoutService {
    cart_service: Arc<CartService>,
    tax_service: Arc<dyn TaxService>,
    order_service: Arc<OrderService>,
    payment_gateway: Arc<dyn PaymentGateway>,
    #[allow(dead_code)]
    shipping_factory: Arc<ShippingProviderFactory>,
    /// Coupon lookups (code -> id) when persisting the coupon on the order;
    /// None = coupon linkage is skipped.
    coupon_service: Option<Arc<crate::services::CouponService>>,
    config: CheckoutConfig,
}

/// Checkout configuration
#[derive(Debug, Clone)]
pub struct CheckoutConfig {
    /// Default shipping rate per kg
    pub default_shipping_rate: Decimal,
    /// Base shipping cost
    pub base_shipping_cost: Decimal,
    /// Free shipping threshold (None = no free shipping)
    pub free_shipping_threshold: Option<Decimal>,
    /// Default currency
    pub default_currency: Currency,
    /// Whether to validate VAT IDs during checkout
    pub validate_vat_ids: bool,
    /// Seller country code for OSS determination
    pub seller_country_code: String,
    /// Stripe publishable key (for Stripe Elements)
    pub stripe_publishable_key: Option<String>,
    /// Airwallex client ID (for Airwallex.js Drop-in)
    pub airwallex_client_id: Option<String>,
    /// Airwallex environment: "demo" or "prod"
    pub airwallex_env: Option<String>,
    /// Public site URL sent to payment providers in metadata (spec 2.1)
    pub site_url: Option<String>,
    /// Percentage discount applied to the merchandise subtotal when the
    /// checkout includes a subscription option ([subscriptions] config)
    pub subscription_discount_percent: i64,
}

impl Default for CheckoutConfig {
    fn default() -> Self {
        use rust_decimal_macros::dec;
        Self {
            default_shipping_rate: dec!(5.00),
            base_shipping_cost: dec!(10.00),
            free_shipping_threshold: Some(dec!(100.00)),
            default_currency: Currency::USD,
            validate_vat_ids: true,
            seller_country_code: "US".to_string(),
            stripe_publishable_key: None,
            airwallex_client_id: None,
            airwallex_env: None,
            site_url: None,
            subscription_discount_percent: 0,
        }
    }
}

/// Subscription option selected at checkout.
///
/// When present, the order starts a subscription: the merchandise subtotal
/// gets the configured subscription discount, and (once payment succeeds) a
/// Subscription row is created that bills the stored-card consent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscriptionCheckoutOption {
    /// Billing interval (validated against the subscription_interval enum)
    pub interval: crate::models::SubscriptionInterval,
    /// How many intervals between billings (default 1)
    #[serde(default)]
    pub interval_count: Option<i32>,
}

/// Subscription discount for a merchandise subtotal, rounded half-up to 2dp.
pub fn subscription_discount(subtotal: Decimal, discount_percent: i64) -> Decimal {
    if discount_percent <= 0 || subtotal <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    (subtotal * Decimal::from(discount_percent) / Decimal::from(100))
        .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero)
}

/// Unit price for a renewal order line: subscription amount split across the
/// subscribed quantity, rounded half-up to 2dp.
pub fn renewal_unit_price(amount: Decimal, quantity: i32) -> Decimal {
    if quantity <= 0 {
        return amount.round_dp(2);
    }
    (amount / Decimal::from(quantity))
        .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero)
}

/// Checkout summary for displaying to customer
#[derive(Debug, Clone)]
pub struct CheckoutSummary {
    pub cart_id: Uuid,
    pub items: Vec<CartItem>,
    pub subtotal: Decimal,
    pub discount_total: Decimal,
    pub shipping_total: Decimal,
    pub shipping_tax: Decimal,
    pub item_tax: Decimal,
    pub tax_total: Decimal,
    pub total: Decimal,
    pub currency: Currency,
    pub available_shipping_rates: Vec<ShippingRate>,
    pub selected_shipping_rate: Option<ShippingRate>,
    pub tax_breakdown: Vec<TaxBreakdownItem>,
    pub vat_id_valid: Option<bool>,
    pub publishable_key: Option<String>,
    pub airwallex_client_id: Option<String>,
    pub airwallex_env: Option<String>,
}

/// Tax breakdown item for display
#[derive(Debug, Clone)]
pub struct TaxBreakdownItem {
    pub tax_zone_name: String,
    pub tax_rate_name: String,
    pub rate: Decimal,
    pub taxable_amount: Decimal,
    pub tax_amount: Decimal,
}

/// Checkout initialization request
#[derive(Debug, Clone)]
pub struct InitiateCheckoutRequest {
    pub cart_id: Uuid,
    pub shipping_address: Address,
    pub billing_address: Option<Address>,
    pub vat_id: Option<String>,
    pub customer_id: Option<Uuid>,
    pub currency: Option<Currency>,
    /// Subscription option, when the customer is subscribing
    pub subscription: Option<SubscriptionCheckoutOption>,
}

/// Shipping selection request
#[derive(Debug, Clone)]
pub struct SelectShippingRequest {
    pub cart_id: Uuid,
    pub shipping_address: Address,
    pub shipping_rate: ShippingRate,
    pub package: Package,
}

/// Complete checkout request
#[derive(Debug, Clone)]
pub struct CompleteCheckoutRequest {
    pub cart_id: Uuid,
    pub shipping_address: Address,
    pub billing_address: Option<Address>,
    pub payment_method: PaymentMethod,
    pub customer_email: String,
    pub customer_id: Option<Uuid>,
    pub vat_id: Option<String>,
    pub notes: Option<String>,
    pub selected_shipping_rate: ShippingRate,
    /// Mark this order as a test order (spec 2.2). Test orders are exported
    /// with test=true so accounting can book them to a suspense account.
    pub test: Option<bool>,
    /// Subscription option: turns this checkout into a subscription initial
    /// order once payment succeeds.
    pub subscription: Option<SubscriptionCheckoutOption>,
    /// Stored-card consent id (verified by the storefront before checkout).
    /// Mandatory when `subscription` is present.
    pub payment_consent_id: Option<String>,
}

/// Checkout result
#[derive(Debug, Clone)]
pub struct CheckoutResult {
    pub order: OrderDetail,
    pub payment_id: String,
    pub total_charged: Decimal,
    pub currency: Currency,
    pub client_secret: Option<String>,
    pub payment_intent_id: Option<String>,
    pub requires_confirmation: bool,
}

/// Confirm checkout request (after client-side Stripe confirmation)
#[derive(Debug, Clone)]
pub struct ConfirmCheckoutRequest {
    pub order_id: Uuid,
    pub payment_intent_id: String,
    pub cart_id: Uuid,
}

/// Tax calculation result with shipping
#[derive(Debug, Clone)]
pub struct TaxCalculationWithShipping {
    pub item_tax: Decimal,
    pub shipping_tax: Decimal,
    pub total_tax: Decimal,
    pub calculation: TaxCalculation,
}

impl CheckoutService {
    /// Create a new checkout service
    pub fn new(
        cart_service: Arc<CartService>,
        tax_service: Arc<dyn TaxService>,
        order_service: Arc<OrderService>,
        payment_gateway: Arc<dyn PaymentGateway>,
        shipping_factory: Arc<ShippingProviderFactory>,
        config: CheckoutConfig,
    ) -> Self {
        Self::with_coupon_service(
            cart_service,
            tax_service,
            order_service,
            payment_gateway,
            shipping_factory,
            config,
            None,
        )
    }

    /// Create a new checkout service with coupon lookup support, so the
    /// coupon applied to the cart is persisted on the created order.
    pub fn with_coupon_service(
        cart_service: Arc<CartService>,
        tax_service: Arc<dyn TaxService>,
        order_service: Arc<OrderService>,
        payment_gateway: Arc<dyn PaymentGateway>,
        shipping_factory: Arc<ShippingProviderFactory>,
        config: CheckoutConfig,
        coupon_service: Option<Arc<crate::services::CouponService>>,
    ) -> Self {
        Self {
            cart_service,
            tax_service,
            order_service,
            payment_gateway,
            shipping_factory,
            coupon_service,
            config,
        }
    }

    /// Initiate checkout - calculate totals, tax, and available shipping rates
    pub async fn initiate_checkout(
        &self,
        request: InitiateCheckoutRequest,
    ) -> Result<CheckoutSummary> {
        info!("Initiating checkout for cart {}", request.cart_id);

        // Get cart with items
        let cart_with_items = self
            .cart_service
            .get_cart_with_items(request.cart_id)
            .await?;
        let cart = cart_with_items.cart;
        let items = cart_with_items.items;

        // Validate cart
        self.validate_cart(&cart, &items).await?;

        // Calculate subtotal (already in cart)
        let subtotal = cart.subtotal;
        // Subscription discount applies to the merchandise subtotal
        let subscription_discount_total = match request.subscription {
            Some(_) => subscription_discount(subtotal, self.config.subscription_discount_percent),
            None => Decimal::ZERO,
        };
        let discount_total = cart.discount_total + subscription_discount_total;

        // Validate VAT ID if provided
        let vat_id_valid = if let Some(ref vat_id) = request.vat_id {
            if self.config.validate_vat_ids {
                match self.validate_vat_id(vat_id).await {
                    Ok(valid) => Some(valid),
                    Err(e) => {
                        warn!("VAT ID validation failed: {}", e);
                        Some(false)
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // Calculate tax (bundle components are not taxed independently)
        let tax_items: Vec<&CartItem> = items
            .iter()
            .filter(|i| !self.is_bundle_component(i))
            .collect();
        let tax_result = self
            .calculate_tax(
                &tax_items,
                &request.shipping_address,
                request.billing_address.as_ref(),
                request.vat_id.as_deref(),
                request.customer_id,
                cart.currency,
            )
            .await?;

        // Get available shipping rates
        let package = self.estimate_package(&items).await?;
        let shipping_rates = self
            .get_shipping_rates(&request.shipping_address, &package)
            .await?;

        // Default to the cheapest available rate (updated when customer selects rate)
        let shipping_total = shipping_rates
            .first()
            .map(|r| r.total_cost)
            .unwrap_or_else(|| self.estimate_shipping_cost(subtotal));
        // Shipping is not taxed (matches the legacy WooCommerce setup); EU VAT
        // comes from the tax-zone rates (10% per EU country) on items only.
        let shipping_tax = Decimal::ZERO;

        let tax_total = tax_result.total_tax;
        let total = subtotal - discount_total + shipping_total + tax_total;

        // Build tax breakdown
        let tax_breakdown = tax_result
            .calculation
            .tax_breakdown
            .iter()
            .map(|tb| TaxBreakdownItem {
                tax_zone_name: tb.tax_zone_name.clone(),
                tax_rate_name: tb.tax_rate_name.clone(),
                rate: tb.rate,
                taxable_amount: tb.taxable_amount,
                tax_amount: tb.tax_amount,
            })
            .collect();

        let summary = CheckoutSummary {
            cart_id: cart.id,
            items,
            subtotal,
            discount_total,
            shipping_total,
            shipping_tax,
            item_tax: tax_result.item_tax,
            tax_total,
            total,
            currency: cart.currency,
            available_shipping_rates: shipping_rates,
            selected_shipping_rate: None,
            tax_breakdown,
            vat_id_valid,
            publishable_key: self.config.stripe_publishable_key.clone(),
            airwallex_client_id: self.config.airwallex_client_id.clone(),
            airwallex_env: self.config.airwallex_env.clone(),
        };

        debug!(
            "Checkout summary: subtotal={}, tax={}, total={}",
            summary.subtotal, summary.tax_total, summary.total
        );

        Ok(summary)
    }

    /// Select shipping method and recalculate totals
    pub async fn select_shipping(&self, request: SelectShippingRequest) -> Result<CheckoutSummary> {
        info!("Selecting shipping for cart {}", request.cart_id);

        // Get cart
        let cart_with_items = self
            .cart_service
            .get_cart_with_items(request.cart_id)
            .await?;
        let cart = cart_with_items.cart;
        let items = cart_with_items.items;

        // Use shipping address provided in the request (from the frontend state)
        let shipping_address = request.shipping_address;

        // Re-quote shipping server-side from the cart weight and validate the
        // client-submitted rate before trusting its price.
        let package = self.estimate_package(&items).await?;
        let validated_rate = self
            .validate_shipping_rate(&request.shipping_rate, &shipping_address, &package)
            .await?;

        // Shipping is not taxed (matches the legacy WooCommerce setup); EU VAT
        // comes from the tax-zone rates (10% per EU country) on items only.
        let shipping_total = validated_rate.total_cost;
        let shipping_tax = Decimal::ZERO;

        // Recalculate totals
        let subtotal = cart.subtotal;
        let discount_total = cart.discount_total;

        // Get item tax (recalculate to ensure consistency; bundle components
        // are not taxed independently)
        let tax_items: Vec<&CartItem> = items
            .iter()
            .filter(|i| !self.is_bundle_component(i))
            .collect();
        let tax_result = self
            .calculate_tax(
                &tax_items,
                &shipping_address,
                None, // billing address
                None, // vat_id
                cart.customer_id,
                cart.currency,
            )
            .await?;

        let tax_total = tax_result.total_tax;
        let total = subtotal - discount_total + shipping_total + tax_total;

        // Get available shipping rates (server-estimated package, same as initiate)
        let shipping_rates = self.get_shipping_rates(&shipping_address, &package).await?;

        let summary = CheckoutSummary {
            cart_id: cart.id,
            items,
            subtotal,
            discount_total,
            shipping_total,
            shipping_tax,
            item_tax: tax_result.item_tax,
            tax_total,
            total,
            currency: cart.currency,
            available_shipping_rates: shipping_rates,
            selected_shipping_rate: Some(validated_rate),
            tax_breakdown: vec![], // TODO: Rebuild breakdown
            vat_id_valid: None,
            publishable_key: self.config.stripe_publishable_key.clone(),
            airwallex_client_id: self.config.airwallex_client_id.clone(),
            airwallex_env: self.config.airwallex_env.clone(),
        };

        Ok(summary)
    }

    /// Complete checkout - create order and process payment
    pub async fn complete_checkout(
        &self,
        request: CompleteCheckoutRequest,
    ) -> Result<CheckoutResult> {
        info!("Completing checkout for cart {}", request.cart_id);

        // Get cart with items
        let cart_with_items = self
            .cart_service
            .get_cart_with_items(request.cart_id)
            .await?;
        let cart = cart_with_items.cart;
        let items = cart_with_items.items;

        // Idempotency: checkout pages retry (e.g. after a poll timeout while
        // the first payment is still pending). Reuse the order already
        // created for this cart instead of minting a second payment intent.
        // The existing order's totals/addresses are authoritative here — no
        // revalidation or recalculation on the replay path.
        if let Some(prior) = self
            .order_service
            .find_latest_by_cart_id(request.cart_id)
            .await?
        {
            match prior.status {
                crate::order::OrderStatus::Pending => {
                    return self.replay_pending_order(prior, &cart).await;
                }
                crate::order::OrderStatus::Confirmed
                | crate::order::OrderStatus::Processing
                | crate::order::OrderStatus::OnHold
                | crate::order::OrderStatus::Shipped
                | crate::order::OrderStatus::Delivered
                | crate::order::OrderStatus::Completed => {
                    info!(
                        "Reusing existing {} order {} for cart {} (checkout replay)",
                        format!("{:?}", prior.status).to_lowercase(),
                        prior.id,
                        request.cart_id
                    );
                    let detail = self
                        .order_service
                        .get_order(prior.id)
                        .await?
                        .ok_or_else(|| Error::not_found("Order not found"))?;
                    return Ok(CheckoutResult {
                        total_charged: prior.total,
                        currency: prior.currency.parse().unwrap_or(Currency::USD),
                        client_secret: None,
                        payment_intent_id: None,
                        requires_confirmation: false,
                        order: detail,
                        payment_id: String::new(),
                    });
                }
                crate::order::OrderStatus::Canceled | crate::order::OrderStatus::Refunded => {
                    // A cancelled/refunded order never blocks a fresh attempt.
                    info!(
                        "Cart {} has {} prior order {}; creating a fresh order",
                        request.cart_id,
                        format!("{:?}", prior.status).to_lowercase(),
                        prior.id
                    );
                }
            }
        }

        // Validate cart one final time
        self.validate_cart(&cart, &items).await?;

        // Calculate final tax (bundle components are not taxed independently;
        // tax is applied to the bundle parent line only).
        let tax_items: Vec<&CartItem> = items
            .iter()
            .filter(|i| !self.is_bundle_component(i))
            .collect();
        let tax_result = self
            .calculate_tax(
                &tax_items,
                &request.shipping_address,
                request.billing_address.as_ref(),
                request.vat_id.as_deref(),
                request.customer_id,
                cart.currency,
            )
            .await?;

        // Re-quote shipping server-side from the cart weight and validate the
        // client-submitted rate before trusting its price.
        let package = self.estimate_package(&items).await?;
        let validated_rate = self
            .validate_shipping_rate(
                &request.selected_shipping_rate,
                &request.shipping_address,
                &package,
            )
            .await?;

        // Shipping is not taxed (matches the legacy WooCommerce setup); EU VAT
        // comes from the tax-zone rates (10% per EU country) on items only.
        let shipping_total = validated_rate.total_cost;

        // Subscription validation: single-product carts only, and a stored
        // card consent is required to bill future renewals.
        let subscription = request.subscription.clone();
        let subscription_quantity: Option<i32>;
        let subscription_product_id;
        if subscription.is_some() {
            let consent_present = request
                .payment_consent_id
                .as_ref()
                .map(|c| !c.is_empty())
                .unwrap_or(false);
            if !consent_present {
                return Err(Error::validation(
                    "payment_consent_id is required for subscription checkout",
                ));
            }
            // Single-product rule: every line (ignoring $0 bundle components)
            // must be the same product.
            let distinct: Vec<Uuid> = items
                .iter()
                .filter(|i| !self.is_bundle_component(i))
                .map(|i| i.product_id)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if distinct.len() != 1 {
                return Err(Error::validation(
                    "Subscription carts must contain a single product",
                ));
            }
            subscription_product_id = distinct.first().copied();
            subscription_quantity = Some(
                items
                    .iter()
                    .filter(|i| !self.is_bundle_component(i))
                    .map(|i| i.quantity)
                    .sum(),
            );
        } else {
            subscription_quantity = None;
            subscription_product_id = None;
        }

        // Subscription discount applies to the merchandise subtotal
        let subscription_discount_total = match subscription {
            Some(_) => {
                subscription_discount(cart.subtotal, self.config.subscription_discount_percent)
            }
            None => Decimal::ZERO,
        };
        let discount_total = cart.discount_total + subscription_discount_total;

        let tax_total = tax_result.total_tax;
        let total = cart.subtotal - discount_total + shipping_total + tax_total;

        // Create order items. Bundle parents carry the bundle price but do not
        // reserve inventory; component lines are $0 fulfillment rows that reserve
        // the underlying product stock.
        let mut order_items: Vec<CreateOrderItem> = Vec::new();
        for item in &items {
            if self.is_bundle_component(item) {
                // Component line: $0 price, reserve component stock.
                order_items.push(CreateOrderItem {
                    product_id: item.product_id,
                    variant_id: item.variant_id,
                    quantity: item.quantity,
                    price: Decimal::ZERO,
                    tax_amount: Decimal::ZERO,
                    skip_inventory_reservation: false,
                });
            } else if self.is_bundle_parent(item) {
                // Bundle parent line: carries the bundle price, no inventory reservation.
                let item_tax = tax_result
                    .calculation
                    .line_items
                    .iter()
                    .find(|li| li.item_id == item.id)
                    .map(|li| li.tax_amount)
                    .unwrap_or_default();
                order_items.push(CreateOrderItem {
                    product_id: item.product_id,
                    variant_id: item.variant_id,
                    quantity: item.quantity,
                    price: item.unit_price,
                    tax_amount: item_tax,
                    skip_inventory_reservation: true,
                });
            } else {
                // Regular line item.
                let item_tax = tax_result
                    .calculation
                    .line_items
                    .iter()
                    .find(|li| li.item_id == item.id)
                    .map(|li| li.tax_amount)
                    .unwrap_or_default();
                order_items.push(CreateOrderItem {
                    product_id: item.product_id,
                    variant_id: item.variant_id,
                    quantity: item.quantity,
                    price: item.unit_price,
                    tax_amount: item_tax,
                    skip_inventory_reservation: false,
                });
            }
        }

        // Persist the cart's coupon on the order (usage itself is only
        // recorded at payment confirmation — see OrderService).
        let cart_coupon_code = cart.coupon_code.clone();
        let cart_coupon_id = if let (Some(svc), Some(code)) =
            (self.coupon_service.as_ref(), cart_coupon_code.as_deref())
        {
            match svc.get_coupon_by_code(code).await {
                Ok(coupon) => Some(coupon.id),
                Err(e) => {
                    warn!(
                        "Failed to resolve coupon {} for order persistence: {}",
                        code, e
                    );
                    None
                }
            }
        } else {
            None
        };

        // Create order
        let mut order_metadata = serde_json::json!({
            "cart_id": cart.id.to_string(),
            "vat_id": request.vat_id,
            "shipping_carrier": validated_rate.carrier,
            "shipping_service": validated_rate.service_code,
        });
        if let Some(ref sub) = subscription {
            order_metadata["subscription"] = serde_json::json!({
                "interval": sub.interval,
                "interval_count": sub.interval_count.unwrap_or(1),
                "payment_consent_id": request.payment_consent_id,
                "product_id": subscription_product_id,
                "quantity": subscription_quantity,
            });
        }

        let create_order_request = CreateOrderRequest {
            customer_id: request.customer_id,
            customer_email: request.customer_email.clone(),
            billing_address_id: None,
            shipping_address_id: None,
            billing_address: request
                .billing_address
                .as_ref()
                .map(|a| serde_json::to_value(a).unwrap_or_default()),
            shipping_address: Some(
                serde_json::to_value(&request.shipping_address).unwrap_or_default(),
            ),
            items: order_items,
            currency: cart.currency.to_string(),
            subtotal: cart.subtotal,
            tax_total,
            shipping_total,
            discount_total,
            total,
            notes: request.notes,
            tags: None,
            test: request.test.unwrap_or(false),
            coupon_code: cart_coupon_code,
            coupon_id: cart_coupon_id,
            metadata: order_metadata,
        };

        let order = self
            .order_service
            .create_order(create_order_request)
            .await?;

        // Record tax transaction for reporting. The calculation's line item ids
        // are cart item ids; map them to the real order item ids by matching
        // product/variant/quantity in order of appearance. Reporting-only: a
        // failure here must not fail checkout after the order was created.
        let order_detail = self.order_service.get_order(order.id).await?;
        if let Some(detail) = &order_detail {
            let mut item_id_map: Vec<(Uuid, Uuid)> = Vec::new();
            let mut used: Vec<Uuid> = Vec::new();
            for cart_item in &items {
                if let Some(oi) = detail.items.iter().find(|oi| {
                    !used.contains(&oi.id)
                        && oi.product_id == cart_item.product_id
                        && oi.variant_id == cart_item.variant_id
                        && oi.quantity == cart_item.quantity
                }) {
                    used.push(oi.id);
                    item_id_map.push((cart_item.id, oi.id));
                }
            }
            if let Err(e) = self
                .tax_service
                .record_tax_transaction(order.id, &tax_result.calculation, &item_id_map)
                .await
            {
                tracing::warn!("Failed to record tax transaction: {}", e);
            }
        }

        // Process payment. The metadata is what lands in the provider's bank
        // feed — order_id must be the store-prefixed invoice reference and
        // must always include the site URL (accounting spec 2.1).
        let order_reference = order
            .invoice_number
            .clone()
            .unwrap_or_else(|| order.order_number.clone());
        let payment_request = CreatePaymentRequest {
            amount: total,
            currency: cart.currency.to_string(),
            order_id: order.id,
            customer_id: request.customer_id,
            customer_email: request.customer_email.clone(),
            payment_method: request.payment_method,
            billing_address: request.billing_address.clone(),
            payment_consent_id: request.payment_consent_id.clone(),
            merchant_initiated: request.payment_consent_id.as_ref().map(|_| true),
            metadata: serde_json::json!({
                "order_id": order.id.to_string(),
                "order_number": order.order_number,
                "order_reference": order_reference,
                "site_url": self.config.site_url,
                "test": request.test.unwrap_or(false),
            }),
        };

        let payment = self.payment_gateway.create_payment(payment_request).await?;

        // Client-side confirmation flow (Stripe Elements, Airwallex Drop-in):
        // return client_secret for frontend confirmation
        if self.payment_gateway.id() == "stripe" || self.payment_gateway.id() == "airwallex" {
            info!(
                "Checkout awaiting {} confirmation: order={}, payment_intent={}",
                self.payment_gateway.id(),
                order.id,
                payment.id
            );

            // Record the pending payment immediately so an early webhook
            // (payment_intent.succeeded arriving before the customer finishes
            // the drop-in) can match the intent to this order. Idempotent.
            let pending_payment = crate::payment::Payment {
                id: payment.id.clone(),
                gateway: self.payment_gateway.id().to_string(),
                amount: payment.amount,
                currency: cart.currency.to_string(),
                status: crate::payment::PaymentStatus::Pending,
                order_id: order.id,
                customer_id: request.customer_id,
                payment_method: "card".to_string(),
                transaction_id: payment.id.clone(),
                client_secret: Some(payment.client_secret.clone()),
                card_brand: None,
                card_last_four: None,
                captured_at: None,
                created_at: chrono::Utc::now(),
            };
            if let Err(e) = self
                .order_service
                .record_payment(order.id, &pending_payment)
                .await
            {
                warn!(
                    "Failed to record pending payment for order {}: {}",
                    order.id, e
                );
            }

            let order_detail = self
                .order_service
                .get_order(order.id)
                .await?
                .ok_or_else(|| Error::not_found("Order not found after creation"))?;
            return Ok(CheckoutResult {
                order: order_detail,
                payment_id: payment.id.clone(),
                total_charged: payment.amount,
                currency: cart.currency,
                client_secret: Some(payment.client_secret.clone()),
                payment_intent_id: Some(payment.id.clone()),
                requires_confirmation: true,
            });
        }

        // Synchronous confirmation for mock and other gateways
        let confirmed_payment = self.payment_gateway.confirm_payment(&payment.id).await?;

        // Record the payment for order history
        if let Err(e) = self
            .order_service
            .record_payment(order.id, &confirmed_payment)
            .await
        {
            warn!("Failed to record payment for order {}: {}", order.id, e);
        }

        // Mark cart as converted and finalize the order
        self.cart_service
            .mark_converted(cart.id, Some(order.id))
            .await?;
        let finalized_order = self
            .order_service
            .confirm_order_after_payment(order.id)
            .await?;

        info!(
            "Checkout complete: order={}, payment={}",
            finalized_order.order.id, payment.id
        );

        Ok(CheckoutResult {
            order: finalized_order,
            payment_id: confirmed_payment.id,
            total_charged: confirmed_payment.amount,
            currency: cart.currency,
            client_secret: None,
            payment_intent_id: None,
            requires_confirmation: false,
        })
    }

    /// Replay path for `complete_checkout` when the cart already has a
    /// pending order: never create a second order. If the recorded intent
    /// has since succeeded, run the same finalization steps as
    /// `confirm_checkout`; otherwise hand the existing order (and its intent
    /// id / client secret) back for client-side confirmation.
    async fn replay_pending_order(
        &self,
        prior: crate::order::Order,
        cart: &Cart,
    ) -> Result<CheckoutResult> {
        let detail = self
            .order_service
            .get_order(prior.id)
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;
        let intent_id = self
            .order_service
            .latest_payment_gateway_id(prior.id)
            .await?;

        if let Some(ref intent_id) = intent_id {
            match self.payment_gateway.get_payment(intent_id).await {
                Ok(payment) if payment.status == crate::payment::PaymentStatus::Succeeded => {
                    info!(
                        "Reusing existing order {} for cart {}: payment {} succeeded; finalizing",
                        prior.id, cart.id, intent_id
                    );
                    if let Err(e) = self.order_service.record_payment(prior.id, &payment).await {
                        warn!("Failed to record payment for order {}: {}", prior.id, e);
                    }
                    if let Err(e) = self
                        .cart_service
                        .mark_converted(cart.id, Some(prior.id))
                        .await
                    {
                        warn!("Failed to mark cart {} converted: {}", cart.id, e);
                    }
                    let finalized = self
                        .order_service
                        .confirm_order_after_payment(prior.id)
                        .await?;
                    return Ok(CheckoutResult {
                        payment_id: payment.id.clone(),
                        total_charged: payment.amount,
                        currency: payment.currency.parse().unwrap_or(Currency::USD),
                        client_secret: payment.client_secret.clone(),
                        payment_intent_id: Some(intent_id.clone()),
                        requires_confirmation: false,
                        order: finalized,
                    });
                }
                Ok(payment) => {
                    info!(
                        "Reusing existing pending order {} for cart {} (payment {} still {:?})",
                        prior.id, cart.id, intent_id, payment.status
                    );
                    return Ok(CheckoutResult {
                        payment_id: payment.id.clone(),
                        total_charged: payment.amount,
                        currency: payment.currency.parse().unwrap_or(Currency::USD),
                        client_secret: payment.client_secret.clone(),
                        payment_intent_id: Some(intent_id.clone()),
                        requires_confirmation: true,
                        order: detail,
                    });
                }
                Err(e) => {
                    warn!(
                        "Could not refresh payment {} for existing order {} ({}); \
                         returning the pending order unchanged",
                        intent_id, prior.id, e
                    );
                    return Ok(CheckoutResult {
                        payment_id: intent_id.clone(),
                        total_charged: Decimal::ZERO,
                        currency: prior.currency.parse().unwrap_or(Currency::USD),
                        client_secret: None,
                        payment_intent_id: Some(intent_id.clone()),
                        requires_confirmation: true,
                        order: detail,
                    });
                }
            }
        }

        // Pending order with no payment recorded yet.
        info!(
            "Reusing existing pending order {} for cart {} (no payment recorded yet)",
            prior.id, cart.id
        );
        Ok(CheckoutResult {
            payment_id: String::new(),
            total_charged: Decimal::ZERO,
            currency: prior.currency.parse().unwrap_or(Currency::USD),
            client_secret: None,
            payment_intent_id: None,
            requires_confirmation: true,
            order: detail,
        })
    }

    /// A succeeded intent proves nothing about *this* order: it must be the
    /// intent recorded for it at checkout and cover its total, otherwise a
    /// cheap order's payment could confirm an expensive one (or many).
    async fn ensure_payment_pays_order(
        &self,
        payment: &crate::payment::Payment,
        order: &crate::order::Order,
        payment_intent_id: &str,
    ) -> Result<()> {
        if !self
            .order_service
            .payment_belongs_to_order(order.id, payment_intent_id)
            .await?
        {
            return Err(Error::validation("Payment does not belong to this order"));
        }
        if !payment_covers_order(
            payment.amount,
            &payment.currency,
            order.total,
            &order.currency,
        ) {
            return Err(Error::payment_error(
                "Payment amount does not cover the order total",
            ));
        }
        Ok(())
    }

    /// Confirm checkout after client-side payment confirmation (Stripe Elements)
    pub async fn confirm_checkout(
        &self,
        request: ConfirmCheckoutRequest,
    ) -> Result<CheckoutResult> {
        info!(
            "Confirming checkout: order={}, payment_intent={}",
            request.order_id, request.payment_intent_id
        );

        // Check the order's current status FIRST: a racing webhook may already
        // have confirmed it, and a page refresh / double submit of the
        // confirm call must be an idempotent success, not an error.
        let existing = self
            .order_service
            .get_order(request.order_id)
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;
        let already_final = matches!(
            existing.order.status,
            crate::order::OrderStatus::Confirmed
                | crate::order::OrderStatus::Processing
                | crate::order::OrderStatus::OnHold
                | crate::order::OrderStatus::Shipped
                | crate::order::OrderStatus::Delivered
                | crate::order::OrderStatus::Completed
        );

        // Verify payment succeeded
        let payment = self
            .payment_gateway
            .get_payment(&request.payment_intent_id)
            .await?;

        if payment.status != crate::payment::PaymentStatus::Succeeded {
            return Err(Error::payment_error(format!(
                "Payment not succeeded. Status: {:?}",
                payment.status
            )));
        }

        self.ensure_payment_pays_order(&payment, &existing.order, &request.payment_intent_id)
            .await?;

        if already_final {
            info!(
                "Order {} already {:?}; confirm replay is a no-op (payment {})",
                request.order_id, existing.order.status, request.payment_intent_id
            );
            return Ok(CheckoutResult {
                order: existing,
                payment_id: payment.id,
                total_charged: payment.amount,
                currency: payment.currency.parse().unwrap_or(Currency::USD),
                client_secret: payment.client_secret,
                payment_intent_id: Some(request.payment_intent_id),
                requires_confirmation: false,
            });
        }

        // Record the payment for order history
        if let Err(e) = self
            .order_service
            .record_payment(request.order_id, &payment)
            .await
        {
            warn!(
                "Failed to record payment for order {}: {}",
                request.order_id, e
            );
        }

        // Mark cart as converted. A racing webhook may already have converted
        // the cart — that must not fail this request.
        let cart_already_converted = self
            .cart_service
            .get_cart_with_items(request.cart_id)
            .await
            .map(|c| c.cart.converted_to_order)
            .unwrap_or(false);
        if !cart_already_converted {
            if let Err(e) = self
                .cart_service
                .mark_converted(request.cart_id, Some(request.order_id))
                .await
            {
                warn!(
                    "Failed to mark cart {} converted (non-fatal): {}",
                    request.cart_id, e
                );
            }
        }
        let finalized_order = self
            .order_service
            .confirm_order_after_payment(request.order_id)
            .await?;

        info!(
            "Checkout confirmed: order={}, payment_intent={}",
            finalized_order.order.id, request.payment_intent_id
        );

        Ok(CheckoutResult {
            order: finalized_order,
            payment_id: payment.id,
            total_charged: payment.amount,
            currency: payment.currency.parse().unwrap_or(Currency::USD),
            client_secret: None,
            payment_intent_id: Some(request.payment_intent_id),
            requires_confirmation: false,
        })
    }

    /// Validate cart before checkout
    async fn validate_cart(&self, cart: &Cart, items: &[CartItem]) -> Result<()> {
        if cart.converted_to_order {
            return Err(Error::validation(
                "Cart has already been converted to an order",
            ));
        }

        if items.is_empty() {
            return Err(Error::validation("Cart is empty"));
        }

        if cart.expires_at.map(|exp| exp < Utc::now()).unwrap_or(false) {
            return Err(Error::validation("Cart has expired"));
        }

        // TODO: Validate inventory availability
        // TODO: Validate product prices haven't changed

        Ok(())
    }

    /// Calculate tax for cart items
    async fn calculate_tax(
        &self,
        items: &[&CartItem],
        shipping_address: &Address,
        billing_address: Option<&Address>,
        vat_id: Option<&str>,
        customer_id: Option<Uuid>,
        currency: Currency,
    ) -> Result<TaxCalculationWithShipping> {
        // Convert cart items to taxable items
        let taxable_items: Vec<TaxableItem> = items
            .iter()
            .map(|item| TaxableItem {
                id: item.id,
                product_id: item.product_id,
                quantity: item.quantity,
                unit_price: item.unit_price,
                total_price: item.total,
                tax_category_id: None, // TODO: Get from product
                is_digital: false,     // TODO: Get from product
                title: item.title.clone(),
                sku: item.sku.clone(),
            })
            .collect();

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
            currency,
            transaction_type: if vat_id.is_some() {
                TransactionType::B2B
            } else {
                TransactionType::B2C
            },
        };

        // Calculate tax
        let calculation = self
            .tax_service
            .calculate_tax(&taxable_items, &tax_context)
            .await?;

        let item_tax: Decimal = calculation.line_items.iter().map(|li| li.tax_amount).sum();
        let total_tax = item_tax + calculation.shipping_tax;

        Ok(TaxCalculationWithShipping {
            item_tax,
            shipping_tax: calculation.shipping_tax,
            total_tax,
            calculation,
        })
    }

    /// Calculate tax on shipping cost. Unused since shipping is no longer
    /// taxed (matches the legacy WooCommerce setup).
    #[allow(dead_code)]
    async fn calculate_shipping_tax(
        &self,
        shipping_cost: Decimal,
        destination: &Address,
    ) -> Result<Decimal> {
        let tax_address = address_to_tax_address(destination);

        // Use TaxCalculator directly for shipping tax
        let _calculator = crate::tax::TaxCalculator::new(
            vec![], // rates will be fetched by tax service
            vec![], // zones will be fetched by tax service
            vec![], // categories
        );

        // For now, use tax service to get rate and calculate
        let rates = self
            .tax_service
            .get_tax_rates(
                &tax_address.country_code,
                tax_address.region_code.as_deref(),
                tax_address.postal_code.as_deref(),
            )
            .await?;

        if let Some(rate) = rates.first() {
            Ok((shipping_cost * rate.rate).round_dp(2))
        } else {
            Ok(Decimal::ZERO)
        }
    }

    /// Re-quote shipping server-side and return the authoritative rate matching
    /// the client-submitted one by (carrier, service_code). The submitted cost
    /// is never trusted: a missing identifier, an unknown rate, or a price that
    /// differs from the fresh quote by more than a cent is rejected.
    async fn validate_shipping_rate(
        &self,
        submitted: &ShippingRate,
        destination: &Address,
        package: &Package,
    ) -> Result<ShippingRate> {
        let rates = self.get_shipping_rates(destination, package).await?;
        match_shipping_rate(submitted, &rates).cloned()
    }

    /// Get available shipping rates
    async fn get_shipping_rates(
        &self,
        destination: &Address,
        package: &Package,
    ) -> Result<Vec<ShippingRate>> {
        // TODO: Get origin address from configuration
        let origin = Address {
            id: Uuid::new_v4(),
            customer_id: Uuid::nil(),
            first_name: "Store".to_string(),
            last_name: "Warehouse".to_string(),
            company: None,
            phone: None,
            address1: "123 Warehouse St".to_string(),
            address2: None,
            city: "Los Angeles".to_string(),
            state: Some("CA".to_string()),
            country: "US".to_string(),
            zip: "90210".to_string(),
            is_default_shipping: false,
            is_default_billing: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let options = RateOptions {
            carriers: None,
            services: None,
            include_insurance: false,
            insurance_value: None,
            signature_confirmation: false,
            adult_signature: false,
            saturday_delivery: false,
            hold_for_pickup: false,
            currency: Some("USD".to_string()),
        };

        // Use the injected shipping factory
        let aggregator =
            crate::shipping::ShippingRateAggregator::new((*self.shipping_factory).clone());
        let rates = aggregator
            .get_all_rates(&origin, destination, package, &options)
            .await?;

        Ok(rates)
    }

    /// Estimate package dimensions from cart items
    async fn estimate_package(&self, items: &[CartItem]) -> Result<Package> {
        // Fetch actual product weights from database
        let product_ids: Vec<Uuid> = items.iter().map(|i| i.product_id).collect();
        let weights = self
            .cart_service
            .get_product_weights(&product_ids)
            .await
            .unwrap_or_default();

        let total_weight: Decimal = items
            .iter()
            .filter(|item| !self.is_bundle_component(item))
            .map(|item| {
                let weight_per_item = weights.get(&item.product_id).copied().unwrap_or(dec!(0.5));
                Decimal::from(item.quantity) * weight_per_item
            })
            .sum();

        Ok(Package {
            weight: total_weight,
            weight_unit: "kg".to_string(),
            length: Some(dec!(30.0)),
            width: Some(dec!(20.0)),
            height: Some(dec!(15.0)),
            dimension_unit: Some("cm".to_string()),
            predefined_package: None,
        })
    }

    /// Legacy scheme: EU VAT on underdeclared customs values. Superseded by
    /// flat 10% tax-zone rates on the full item value (matches the old
    /// WooCommerce store); kept for reference.
    #[allow(dead_code)]
    async fn calculate_eu_vat(&self, items: &[CartItem]) -> Decimal {
        let product_ids: Vec<Uuid> = items.iter().map(|i| i.product_id).collect();
        let categories = self
            .cart_service
            .get_product_categories(&product_ids)
            .await
            .unwrap_or_default();

        let mut total_declared_value = Decimal::ZERO;

        for item in items {
            let category = categories
                .get(&item.product_id)
                .map(|s| s.as_str())
                .unwrap_or("accessories");
            let value_per_item = match category {
                "wheels" => dec!(10.00),
                "bushings" => dec!(2.00),
                _ => dec!(5.00), // Default for accessories etc
            };
            total_declared_value += value_per_item * Decimal::from(item.quantity);
        }

        // 20% VAT
        total_declared_value * dec!(0.20)
    }

    /// Estimate shipping cost before rate selection
    fn estimate_shipping_cost(&self, subtotal: Decimal) -> Decimal {
        // Check free shipping threshold
        if let Some(threshold) = self.config.free_shipping_threshold {
            if subtotal >= threshold {
                return Decimal::ZERO;
            }
        }

        // Default estimate
        self.config.base_shipping_cost
    }

    /// Validate VAT ID
    async fn validate_vat_id(&self, vat_id: &str) -> Result<bool> {
        let result = self.tax_service.validate_vat_id(vat_id).await?;
        Ok(result.is_valid)
    }

    /// Check if a cart item is a bundle component
    fn is_bundle_component(&self, item: &CartItem) -> bool {
        item.custom_attributes
            .as_ref()
            .and_then(|attrs| attrs.get("is_bundle_component"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Check if a cart item is a bundle parent
    fn is_bundle_parent(&self, item: &CartItem) -> bool {
        item.custom_attributes
            .as_ref()
            .and_then(|attrs| attrs.get("is_bundle_parent"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

/// Match a client-submitted shipping rate against a fresh server-side quote.
/// Pure function (no I/O) so the price-tamper logic is unit-testable.
fn match_shipping_rate<'a>(
    submitted: &ShippingRate,
    quoted: &'a [ShippingRate],
) -> Result<&'a ShippingRate> {
    if submitted.carrier.trim().is_empty() || submitted.service_code.trim().is_empty() {
        return Err(Error::validation(
            "Selected shipping rate is missing carrier/service identifiers",
        ));
    }
    let matched = quoted
        .iter()
        .find(|r| r.carrier == submitted.carrier && r.service_code == submitted.service_code)
        .ok_or_else(|| {
            Error::validation(format!(
                "Selected shipping rate ({}/{}) is not available",
                submitted.carrier, submitted.service_code
            ))
        })?;
    let price_diff = (matched.total_cost - submitted.total_cost).abs();
    if price_diff > dec!(0.01) {
        return Err(Error::validation(format!(
            "Selected shipping rate price mismatch: quoted {}, submitted {}",
            matched.total_cost, submitted.total_cost
        )));
    }
    Ok(matched)
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

use rust_decimal_macros::dec;

/// Whether a gateway payment pays for an order: at least its total, in its currency
fn payment_covers_order(
    amount: Decimal,
    currency: &str,
    order_total: Decimal,
    order_currency: &str,
) -> bool {
    amount >= order_total && currency.eq_ignore_ascii_case(order_currency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::lifecycle::OrderEventDispatcher;
    use crate::payment::{
        CreatePaymentRequest, Payment, PaymentSession, PaymentSessionStatus, Refund, RefundStatus,
        WebhookEvent,
    };
    use crate::services::cart_service::ProductDetails;
    use crate::services::CouponService;

    #[test]
    fn test_payment_covers_order() {
        assert!(payment_covers_order(dec!(50.00), "usd", dec!(50.00), "USD"));
        assert!(payment_covers_order(dec!(50.01), "USD", dec!(50.00), "USD"));
        assert!(!payment_covers_order(dec!(0.01), "USD", dec!(50.00), "USD"));
        assert!(!payment_covers_order(
            dec!(50.00),
            "EUR",
            dec!(50.00),
            "USD"
        ));
    }

    #[test]
    fn test_checkout_config_default() {
        let config = CheckoutConfig::default();
        assert_eq!(config.default_shipping_rate, dec!(5.00));
        assert_eq!(config.base_shipping_cost, dec!(10.00));
        assert_eq!(config.free_shipping_threshold, Some(dec!(100.00)));
        assert_eq!(config.stripe_publishable_key, None);
        assert_eq!(config.subscription_discount_percent, 0);
    }

    #[test]
    fn test_subscription_discount_calculation() {
        // 10% of 100.00
        assert_eq!(subscription_discount(dec!(100.00), 10), dec!(10.00));
        // 15% of 153.00
        assert_eq!(subscription_discount(dec!(153.00), 15), dec!(22.95));
        // Half-up rounding: 10% of 19.99
        assert_eq!(subscription_discount(dec!(19.99), 10), dec!(2.00));
        // Zero percent / zero subtotal → no discount
        assert_eq!(subscription_discount(dec!(100.00), 0), Decimal::ZERO);
        assert_eq!(subscription_discount(Decimal::ZERO, 10), Decimal::ZERO);
    }

    #[test]
    fn test_renewal_unit_price_split() {
        // $153.00 across 4 bags → $38.25 each
        assert_eq!(renewal_unit_price(dec!(153.00), 4), dec!(38.25));
        assert_eq!(renewal_unit_price(dec!(100.00), 1), dec!(100.00));
        // Zero/negative quantity falls back to the full amount
        assert_eq!(renewal_unit_price(dec!(99.99), 0), dec!(99.99));
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let tax_addr = address_to_tax_address(&address);
        assert_eq!(tax_addr.country_code, "DE");
        assert_eq!(tax_addr.region_code, Some("BE".to_string()));
        assert_eq!(tax_addr.postal_code, Some("10115".to_string()));
        assert_eq!(tax_addr.city, Some("Berlin".to_string()));
    }

    fn rate(carrier: &str, service: &str, total: &str) -> ShippingRate {
        ShippingRate {
            provider_id: "manual".to_string(),
            carrier: carrier.to_string(),
            service_code: service.to_string(),
            service_name: service.to_string(),
            rate: dec!(0),
            currency: "USD".to_string(),
            delivery_days: None,
            delivery_date: None,
            estimated: false,
            insurance_fee: None,
            fuel_surcharge: None,
            handling_fee: None,
            other_fees: Default::default(),
            total_cost: total.parse().unwrap(),
        }
    }

    #[test]
    fn test_match_shipping_rate_accepts_matching_price() {
        let quoted = vec![rate("manual", "standard", "10.00")];
        let submitted = rate("manual", "standard", "10.00");
        let matched = match_shipping_rate(&submitted, &quoted).unwrap();
        assert_eq!(matched.total_cost, dec!(10.00));
    }

    #[test]
    fn test_match_shipping_rate_accepts_sub_cent_rounding_diff() {
        let quoted = vec![rate("manual", "standard", "10.00")];
        let submitted = rate("manual", "standard", "10.005");
        assert!(match_shipping_rate(&submitted, &quoted).is_ok());
    }

    #[test]
    fn test_match_shipping_rate_rejects_tampered_low_price() {
        let quoted = vec![rate("manual", "standard", "10.00")];
        let submitted = rate("manual", "standard", "0.00");
        let err = match_shipping_rate(&submitted, &quoted).unwrap_err();
        assert!(err.to_string().contains("mismatch"), "unexpected: {}", err);
    }

    #[test]
    fn test_match_shipping_rate_rejects_unknown_rate() {
        let quoted = vec![rate("manual", "standard", "10.00")];
        let submitted = rate("fake", "free", "0.00");
        assert!(match_shipping_rate(&submitted, &quoted).is_err());
    }

    #[test]
    fn test_match_shipping_rate_rejects_missing_identifiers() {
        let quoted = vec![rate("manual", "standard", "10.00")];
        let mut submitted = rate("manual", "standard", "10.00");
        submitted.service_code = String::new();
        assert!(match_shipping_rate(&submitted, &quoted).is_err());
    }

    /// Gateway whose payment status is controllable from the test, so the
    /// pending → succeeded lifecycle of a client-side confirmation flow can be
    /// exercised. Identifies as "airwallex" so checkout takes the
    /// requires_confirmation path.
    struct ControllableGateway {
        status: std::sync::Mutex<crate::payment::PaymentStatus>,
        sessions: std::sync::Mutex<Vec<(String, String, Decimal)>>, // id, secret, amount
    }

    impl Default for ControllableGateway {
        fn default() -> Self {
            Self {
                status: std::sync::Mutex::new(crate::payment::PaymentStatus::Pending),
                sessions: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl ControllableGateway {
        fn set_status(&self, status: crate::payment::PaymentStatus) {
            *self.status.lock().unwrap() = status;
        }
    }

    #[async_trait::async_trait]
    impl crate::payment::PaymentGateway for ControllableGateway {
        fn id(&self) -> &'static str {
            "airwallex"
        }
        fn name(&self) -> &'static str {
            "Controllable Test Gateway"
        }

        async fn create_payment(&self, request: CreatePaymentRequest) -> Result<PaymentSession> {
            let id = format!("pi_{}", Uuid::new_v4());
            let secret = format!("secret_{}", Uuid::new_v4());
            self.sessions
                .lock()
                .unwrap()
                .push((id.clone(), secret.clone(), request.amount));
            Ok(PaymentSession {
                id,
                client_secret: secret,
                status: PaymentSessionStatus::Open,
                amount: request.amount,
                currency: request.currency,
                customer_id: request.customer_id,
            })
        }

        async fn confirm_payment(&self, payment_id: &str) -> Result<Payment> {
            self.get_payment(payment_id).await
        }

        async fn capture_payment(
            &self,
            payment_id: &str,
            _amount: Option<Decimal>,
        ) -> Result<Payment> {
            self.get_payment(payment_id).await
        }

        async fn refund_payment(
            &self,
            _payment_id: &str,
            _amount: Option<Decimal>,
            reason: &str,
        ) -> Result<Refund> {
            Ok(Refund {
                id: "ref_1".to_string(),
                payment_id: "pay_1".to_string(),
                amount: Decimal::ONE,
                currency: "USD".to_string(),
                status: RefundStatus::Succeeded,
                reason: reason.to_string(),
                created_at: Utc::now(),
            })
        }

        async fn get_payment(&self, payment_id: &str) -> Result<Payment> {
            let (secret, amount) = self
                .sessions
                .lock()
                .unwrap()
                .iter()
                .find(|(id, _, _)| id == payment_id)
                .map(|(_, s, a)| (Some(s.clone()), *a))
                .unwrap_or((None, Decimal::ZERO));
            Ok(Payment {
                id: format!("pay_{}", Uuid::new_v4()),
                gateway: self.id().to_string(),
                amount,
                currency: "USD".to_string(),
                status: self.status.lock().unwrap().clone(),
                order_id: Uuid::nil(),
                customer_id: None,
                payment_method: "card".to_string(),
                transaction_id: payment_id.to_string(),
                client_secret: secret,
                card_brand: None,
                card_last_four: None,
                captured_at: None,
                created_at: Utc::now(),
            })
        }

        async fn handle_webhook(&self, _payload: &[u8], _signature: &str) -> Result<WebhookEvent> {
            Err(Error::validation("not implemented in test gateway"))
        }
    }

    /// DB-backed checkout idempotency tests. Require PostgreSQL; set
    /// TEST_DATABASE_URL (defaults to the local scratch database). Tests skip
    /// gracefully when no database is available.
    struct CheckoutTestContext {
        pool: sqlx::PgPool,
        cart_id: Uuid,
        product_id: Uuid,
        location_id: Uuid,
    }

    async fn checkout_setup() -> Option<(
        Arc<ControllableGateway>,
        CheckoutService,
        CheckoutTestContext,
    )> {
        use crate::repository::{CartRepository, CouponRepository};
        use crate::repository::{Database, PgCartRepository, PgCouponRepository};

        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string());
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping checkout idempotency tests: cannot connect to {url}: {e}");
                return None;
            }
        };

        if let Err(e) = crate::Migrator::new(pool.clone()).migrate().await {
            eprintln!("Skipping checkout idempotency tests: migration failed: {e}");
            return None;
        }
        // Migration 013 is not registered in the migrator (applied by hand on
        // the live databases); apply it so stock_reservations matches the code.
        let fixup = include_str!("../../migrations/013_stock_reservations_order_id_nullable.sql");
        if let Err(e) = sqlx::raw_sql(fixup).execute(&pool).await {
            eprintln!("Skipping checkout idempotency tests: 013 fixup failed: {e}");
            return None;
        }

        let db = Database::new(pool.clone());

        // Seed a $50 product with stock at the DEFAULT inventory location
        // (order creation reserves at the oldest active location), and the
        // default tax zone for the test address country (pre-created so the
        // two parallel tests don't race the service's zone creation).
        let product_id = Uuid::new_v4();
        sqlx::query("DELETE FROM products WHERE id = $1")
            .bind(product_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO products (id, title, slug, price) VALUES ($1, 'Idempotency Test', $2, 50.00)",
        )
        .bind(product_id)
        .bind(format!("idem-test-{}", product_id))
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO tax_zones (name, code, country_code, zone_type) \
             VALUES ('Default HK', 'HK', 'HK', 'country') ON CONFLICT (code) DO NOTHING",
        )
        .execute(&pool)
        .await
        .unwrap();

        let existing_location: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM inventory_locations WHERE is_active = true ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        let location_id = match existing_location {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO inventory_locations (id, name, code) VALUES ($1, 'Idem WH', $2)",
                )
                .bind(id)
                .bind(format!("IDEM-{}", &id.to_string()[..8]))
                .execute(&pool)
                .await
                .unwrap();
                id
            }
        };
        sqlx::query(
            "INSERT INTO inventory_levels (product_id, location_id, available_quantity) \
             VALUES ($1, $2, 100)",
        )
        .bind(product_id)
        .bind(location_id)
        .execute(&pool)
        .await
        .unwrap();

        // Cart with one unit of the product.
        let cart_repo: Arc<dyn CartRepository> = Arc::new(PgCartRepository::new(pool.clone()));
        let coupon_repo: Arc<dyn CouponRepository> =
            Arc::new(PgCouponRepository::new(pool.clone()));
        let coupon_service = Arc::new(CouponService::new(coupon_repo.clone(), cart_repo.clone()));
        let cart_service = Arc::new(CartService::new(
            cart_repo.clone(),
            coupon_repo.clone(),
            coupon_service.clone(),
        ));
        let cart = cart_service
            .get_or_create_cart(
                crate::models::CartIdentifier::Session(format!("sess_{}", Uuid::new_v4())),
                "USD",
            )
            .await
            .unwrap();
        cart_service
            .add_item(
                cart.id,
                crate::models::AddToCartInput {
                    product_id,
                    variant_id: None,
                    quantity: 1,
                    custom_attributes: None,
                },
                ProductDetails {
                    unit_price: dec!(50.00),
                    original_price: dec!(50.00),
                    sku: Some("IDEM-1".to_string()),
                    title: "Idempotency Test".to_string(),
                    variant_title: None,
                    image_url: None,
                    requires_shipping: true,
                    is_gift_card: false,
                },
            )
            .await
            .unwrap();

        let gateway = Arc::new(ControllableGateway::default());
        let inventory_service =
            crate::inventory::InventoryService::new(db.clone(), Default::default());
        let order_service = Arc::new(
            OrderService::new(
                db.clone(),
                Box::new(crate::payment::gateways::MockPaymentGateway::new()),
                inventory_service,
                OrderEventDispatcher::new(),
            )
            .with_store_code(None),
        );
        let tax_service = Arc::new(crate::tax::DefaultTaxService::new(pool.clone()));
        let mut factory = ShippingProviderFactory::new();
        factory.register(Box::new(crate::shipping::carriers::ManualProvider::new()));

        let service = CheckoutService::new(
            cart_service,
            tax_service,
            order_service,
            gateway.clone(),
            Arc::new(factory),
            CheckoutConfig::default(),
        );

        Some((
            gateway,
            service,
            CheckoutTestContext {
                pool,
                cart_id: cart.id,
                product_id,
                location_id,
            },
        ))
    }

    fn idem_address() -> Address {
        Address {
            id: Uuid::new_v4(),
            customer_id: Uuid::nil(),
            first_name: "Idem".to_string(),
            last_name: "Buyer".to_string(),
            company: None,
            phone: Some("+85290000000".to_string()),
            address1: "1 Test Street".to_string(),
            address2: None,
            city: "Hong Kong".to_string(),
            state: Some("HK".to_string()),
            country: "HK".to_string(),
            zip: "999077".to_string(),
            is_default_shipping: true,
            is_default_billing: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn idem_shipping_rate() -> ShippingRate {
        ShippingRate {
            provider_id: "manual".to_string(),
            carrier: "Manual".to_string(),
            service_code: "standard".to_string(),
            service_name: "Standard Shipping".to_string(),
            rate: dec!(10.00),
            currency: "USD".to_string(),
            delivery_days: Some(5),
            delivery_date: None,
            estimated: false,
            insurance_fee: None,
            fuel_surcharge: None,
            handling_fee: None,
            other_fees: Default::default(),
            total_cost: dec!(10.00),
        }
    }

    fn idem_complete_request(cart_id: Uuid) -> CompleteCheckoutRequest {
        CompleteCheckoutRequest {
            cart_id,
            shipping_address: idem_address(),
            billing_address: None,
            payment_method: PaymentMethod::StripeElements,
            customer_email: "buyer@example.com".to_string(),
            customer_id: None,
            vat_id: None,
            notes: None,
            selected_shipping_rate: idem_shipping_rate(),
            test: None,
            subscription: None,
            payment_consent_id: None,
        }
    }

    async fn idem_cleanup(ctx: &CheckoutTestContext) {
        let pool = &ctx.pool;
        sqlx::query("DELETE FROM order_items WHERE order_id IN (SELECT id FROM orders WHERE metadata->>'cart_id' = $1)")
            .bind(ctx.cart_id.to_string())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM payments WHERE order_id IN (SELECT id FROM orders WHERE metadata->>'cart_id' = $1)")
            .bind(ctx.cart_id.to_string())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM stock_reservations WHERE order_id IN (SELECT id FROM orders WHERE metadata->>'cart_id' = $1)")
            .bind(ctx.cart_id.to_string())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM stock_reservations WHERE product_id = $1")
            .bind(ctx.product_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM orders WHERE metadata->>'cart_id' = $1")
            .bind(ctx.cart_id.to_string())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM cart_items WHERE cart_id = $1")
            .bind(ctx.cart_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM carts WHERE id = $1")
            .bind(ctx.cart_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM inventory_levels WHERE product_id = $1")
            .bind(ctx.product_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM inventory_locations WHERE id = $1")
            .bind(ctx.location_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM products WHERE id = $1")
            .bind(ctx.product_id)
            .execute(pool)
            .await
            .unwrap();
    }

    /// The boa double-charge scenario: checkout completes, the page polls
    /// past its timeout, the customer retries `complete_checkout` while the
    /// first payment is still pending — the retry must reuse the same order
    /// and the same payment intent, never mint a second order.
    #[tokio::test]
    async fn test_complete_checkout_twice_uses_same_order() {
        let Some((gateway, service, ctx)) = checkout_setup().await else {
            return;
        };
        gateway.set_status(crate::payment::PaymentStatus::Pending);

        let first = service
            .complete_checkout(idem_complete_request(ctx.cart_id))
            .await
            .expect("first complete_checkout must succeed");
        assert!(
            first.requires_confirmation,
            "client-side gateway flow must require confirmation"
        );
        let order_id = first.order.order.id;
        let intent = first
            .payment_intent_id
            .clone()
            .expect("a payment intent id must be returned");

        // Replay while the payment is still pending: same order, same intent.
        let second = service
            .complete_checkout(idem_complete_request(ctx.cart_id))
            .await
            .expect("replay of complete_checkout must succeed, not error");
        assert_eq!(
            second.order.order.id, order_id,
            "replay must reuse the existing order, not create a new one"
        );
        assert_eq!(second.payment_intent_id.as_deref(), Some(intent.as_str()));
        assert!(second.requires_confirmation);

        let order_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE metadata->>'cart_id' = $1")
                .bind(ctx.cart_id.to_string())
                .fetch_one(&ctx.pool)
                .await
                .unwrap();
        assert_eq!(order_count, 1, "exactly one order must exist for the cart");

        // The first payment succeeds (customer finishes the drop-in): a later
        // retry finalizes the SAME order.
        gateway.set_status(crate::payment::PaymentStatus::Succeeded);
        let third = service
            .complete_checkout(idem_complete_request(ctx.cart_id))
            .await
            .expect("post-success replay must finalize the existing order");
        assert_eq!(third.order.order.id, order_id);
        assert!(!third.requires_confirmation);
        assert_eq!(
            third.order.order.status,
            crate::order::OrderStatus::Confirmed
        );

        let order_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE metadata->>'cart_id' = $1")
                .bind(ctx.cart_id.to_string())
                .fetch_one(&ctx.pool)
                .await
                .unwrap();
        assert_eq!(
            order_count, 1,
            "finalization must not create a second order either"
        );

        idem_cleanup(&ctx).await;
    }

    /// confirm_checkout must be replay-safe: a second confirm on an
    /// already-confirmed order (page refresh, double submit, webhook racing
    /// the confirm endpoint) returns Ok with the same order instead of
    /// failing on the already-converted cart.
    #[tokio::test]
    async fn test_confirm_checkout_replay_is_idempotent() {
        let Some((gateway, service, ctx)) = checkout_setup().await else {
            return;
        };
        gateway.set_status(crate::payment::PaymentStatus::Pending);

        let first = service
            .complete_checkout(idem_complete_request(ctx.cart_id))
            .await
            .expect("complete_checkout must succeed");
        let order_id = first.order.order.id;
        let intent = first.payment_intent_id.clone().expect("payment intent id");

        gateway.set_status(crate::payment::PaymentStatus::Succeeded);
        let confirm = ConfirmCheckoutRequest {
            order_id,
            payment_intent_id: intent.clone(),
            cart_id: ctx.cart_id,
        };
        let confirmed = service
            .confirm_checkout(confirm.clone())
            .await
            .expect("first confirm must succeed");
        assert_eq!(confirmed.order.order.id, order_id);
        assert_eq!(
            confirmed.order.order.status,
            crate::order::OrderStatus::Confirmed
        );

        // Replay: already-confirmed order + already-converted cart must still
        // be an idempotent success.
        let replay = service
            .confirm_checkout(confirm)
            .await
            .expect("confirm replay on a confirmed order must succeed");
        assert_eq!(replay.order.order.id, order_id);
        assert!(!replay.requires_confirmation);

        let order_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE metadata->>'cart_id' = $1")
                .bind(ctx.cart_id.to_string())
                .fetch_one(&ctx.pool)
                .await
                .unwrap();
        assert_eq!(order_count, 1);

        idem_cleanup(&ctx).await;
    }

    #[tokio::test]
    async fn test_confirm_checkout_rejects_an_intent_of_another_order() {
        let Some((gateway, service, ctx)) = checkout_setup().await else {
            return;
        };
        gateway.set_status(crate::payment::PaymentStatus::Pending);
        let first = service
            .complete_checkout(idem_complete_request(ctx.cart_id))
            .await
            .expect("complete_checkout must succeed");
        let order_id = first.order.order.id;

        // A succeeded payment the gateway knows, but that was never recorded for
        // this order (e.g. the intent of another, cheaper order)
        let other = gateway
            .create_payment(CreatePaymentRequest {
                amount: first.total_charged,
                currency: "USD".to_string(),
                order_id: Uuid::new_v4(),
                customer_id: None,
                customer_email: "other@example.com".to_string(),
                payment_method: crate::payment::PaymentMethod::StripeElements,
                billing_address: None,
                payment_consent_id: None,
                merchant_initiated: None,
                metadata: serde_json::json!({}),
            })
            .await
            .unwrap();
        gateway.set_status(crate::payment::PaymentStatus::Succeeded);

        let result = service
            .confirm_checkout(ConfirmCheckoutRequest {
                order_id,
                payment_intent_id: other.id,
                cart_id: ctx.cart_id,
            })
            .await;
        assert!(
            result.is_err(),
            "an intent not recorded for the order must be refused"
        );

        idem_cleanup(&ctx).await;
    }

    #[tokio::test]
    async fn test_confirm_checkout_rejects_an_underpaid_intent() {
        let Some((gateway, service, ctx)) = checkout_setup().await else {
            return;
        };
        gateway.set_status(crate::payment::PaymentStatus::Pending);
        let first = service
            .complete_checkout(idem_complete_request(ctx.cart_id))
            .await
            .expect("complete_checkout must succeed");
        let order_id = first.order.order.id;
        let intent = first.payment_intent_id.clone().expect("payment intent id");

        // The gateway reports one cent for the intent recorded for this order
        gateway
            .sessions
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|(id, _, _)| *id == intent)
            .for_each(|session| session.2 = dec!(0.01));
        gateway.set_status(crate::payment::PaymentStatus::Succeeded);

        let result = service
            .confirm_checkout(ConfirmCheckoutRequest {
                order_id,
                payment_intent_id: intent,
                cart_id: ctx.cart_id,
            })
            .await;
        assert!(
            result.is_err(),
            "a payment below the order total must be refused"
        );

        idem_cleanup(&ctx).await;
    }
}
