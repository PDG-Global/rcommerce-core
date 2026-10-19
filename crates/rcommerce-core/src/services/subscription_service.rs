//! Subscription Service
//!
//! Business logic for subscription management including:
//! - Creating and managing subscriptions
//! - Billing cycle processing
//! - Payment retry handling (dunning)
//! - Subscription lifecycle management

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::models::{
    CancelSubscriptionRequest, CancellationReason, CreateSubscriptionRequest, DunningConfig,
    DunningEmail, DunningEmailType, InvoiceStatus, PaymentRecoveryResult, PaymentRetryAttempt,
    Subscription, SubscriptionFilter, SubscriptionInterval, SubscriptionInvoice,
    SubscriptionStatus, SubscriptionSummary, UpdateSubscriptionRequest,
};
use crate::repository::SubscriptionRepository;
use crate::{Error, Result};

/// Calculate the next billing date for a subscription interval.
pub fn next_billing_at(
    interval: SubscriptionInterval,
    interval_count: i32,
    current: DateTime<Utc>,
) -> DateTime<Utc> {
    let count = interval_count as i64;

    match interval {
        SubscriptionInterval::Daily => current + Duration::days(count),
        SubscriptionInterval::Weekly => current + Duration::weeks(count),
        SubscriptionInterval::BiWeekly => current + Duration::weeks(count * 2),
        SubscriptionInterval::Monthly => {
            // Add months properly
            current
                .checked_add_months(chrono::Months::new(count as u32))
                .unwrap_or(current + Duration::days(30 * count))
        }
        SubscriptionInterval::Quarterly => current
            .checked_add_months(chrono::Months::new(count as u32 * 3))
            .unwrap_or(current + Duration::days(90 * count)),
        SubscriptionInterval::BiAnnually => current
            .checked_add_months(chrono::Months::new(count as u32 * 6))
            .unwrap_or(current + Duration::days(180 * count)),
        SubscriptionInterval::Annually => current
            .checked_add_months(chrono::Months::new(count as u32 * 12))
            .unwrap_or(current + Duration::days(365 * count)),
    }
}

/// Subscription service
#[derive(Clone)]
pub struct SubscriptionService<R: SubscriptionRepository> {
    repository: R,
    dunning_config: DunningConfig,
}

impl<R: SubscriptionRepository> SubscriptionService<R> {
    pub fn new(repository: R) -> Self {
        Self {
            repository,
            dunning_config: DunningConfig::default(),
        }
    }

    pub fn with_dunning_config(repository: R, config: DunningConfig) -> Self {
        Self {
            repository,
            dunning_config: config,
        }
    }

    /// Get the dunning configuration
    pub fn config(&self) -> &DunningConfig {
        &self.dunning_config
    }

    // --- Subscription Management ---

    /// Create a new subscription
    pub async fn create_subscription(
        &self,
        request: CreateSubscriptionRequest,
    ) -> Result<Subscription> {
        // Validate min/max cycles
        if let (Some(min), Some(max)) = (request.min_cycles, request.max_cycles) {
            if min > max {
                return Err(Error::validation(
                    "min_cycles cannot be greater than max_cycles",
                ));
            }
        }

        // Validate interval count
        if request.interval_count < 1 || request.interval_count > 12 {
            return Err(Error::validation("interval_count must be between 1 and 12"));
        }

        self.repository.create(request).await
    }

    /// Get subscription by ID
    pub async fn get_subscription(&self, id: Uuid) -> Result<Subscription> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::not_found("Subscription not found"))
    }

    /// List subscriptions with filtering
    pub async fn list_subscriptions(
        &self,
        filter: SubscriptionFilter,
        page: i64,
        per_page: i64,
    ) -> Result<(Vec<Subscription>, i64)> {
        let subscriptions = self.repository.list(&filter, page, per_page).await?;
        let total = self.repository.count(&filter).await?;
        Ok((subscriptions, total))
    }

    /// Get subscriptions for a customer
    pub async fn get_customer_subscriptions(
        &self,
        customer_id: Uuid,
        active_only: bool,
    ) -> Result<Vec<Subscription>> {
        let status = if active_only {
            Some(SubscriptionStatus::Active)
        } else {
            None
        };
        self.repository.list_by_customer(customer_id, status).await
    }

    /// Update subscription
    pub async fn update_subscription(
        &self,
        id: Uuid,
        request: UpdateSubscriptionRequest,
    ) -> Result<Subscription> {
        let subscription = self.get_subscription(id).await?;

        // Prevent updates to cancelled/expired subscriptions
        if matches!(
            subscription.status,
            SubscriptionStatus::Cancelled | SubscriptionStatus::Expired
        ) {
            return Err(Error::validation(
                "Cannot update cancelled or expired subscription",
            ));
        }

        self.repository.update(id, request).await
    }

    /// Cancel a subscription
    pub async fn cancel_subscription(
        &self,
        id: Uuid,
        request: CancelSubscriptionRequest,
    ) -> Result<Subscription> {
        let subscription = self.get_subscription(id).await?;

        // Check if already cancelled
        if subscription.status == SubscriptionStatus::Cancelled {
            return Err(Error::validation("Subscription is already cancelled"));
        }

        // Check minimum cycles
        if let Some(min_cycles) = subscription.min_cycles {
            if subscription.current_cycle < min_cycles && !request.cancel_at_end {
                return Err(Error::validation(format!(
                    "Minimum {} billing cycles required before cancellation",
                    min_cycles
                )));
            }
        }

        self.repository.cancel(id, request).await
    }

    /// Pause a subscription
    pub async fn pause_subscription(&self, id: Uuid) -> Result<Subscription> {
        let subscription = self.get_subscription(id).await?;

        if subscription.status != SubscriptionStatus::Active {
            return Err(Error::validation("Only active subscriptions can be paused"));
        }

        self.repository.pause(id).await
    }

    /// Resume a paused subscription
    pub async fn resume_subscription(&self, id: Uuid) -> Result<Subscription> {
        let subscription = self.get_subscription(id).await?;

        if subscription.status != SubscriptionStatus::Paused {
            return Err(Error::validation(
                "Only paused subscriptions can be resumed",
            ));
        }

        self.repository.resume(id).await
    }

    /// Create a subscription from a successfully paid initial order.
    ///
    /// This is the checkout bridge: once the subscription initial order's
    /// payment succeeds, the order metadata's subscription options plus the
    /// verified stored-card consent become the billing subscription.
    pub async fn create_from_order(
        &self,
        input: CreateSubscriptionFromOrder,
    ) -> Result<Subscription> {
        if input.interval_count < 1 || input.interval_count > 12 {
            return Err(Error::validation("interval_count must be between 1 and 12"));
        }

        self.repository
            .create(CreateSubscriptionRequest {
                customer_id: input.customer_id,
                order_id: input.order_id,
                product_id: input.product_id,
                variant_id: input.variant_id,
                interval: input.interval,
                interval_count: input.interval_count,
                currency: input.currency,
                amount: input.amount,
                setup_fee: None,
                quantity: Some(input.quantity.max(1)),
                shipping_address: input.shipping_address,
                trial_days: None,
                min_cycles: None,
                max_cycles: None,
                payment_method_id: input.payment_consent_id,
                gateway: input.gateway,
                notes: None,
            })
            .await
    }

    // --- Billing Cycle Management ---

    /// Process billing for subscriptions due
    pub async fn process_due_subscriptions(&self) -> Result<Vec<SubscriptionInvoice>> {
        let now = Utc::now();
        let due_subscriptions = self.repository.get_due_for_billing(now).await?;

        let mut invoices = Vec::new();

        for subscription in due_subscriptions {
            match self.process_billing_cycle(&subscription).await {
                Ok(invoice) => invoices.push(invoice),
                Err(e) => {
                    tracing::error!(
                        "Failed to process billing for subscription {}: {}",
                        subscription.id,
                        e
                    );
                    // Continue processing other subscriptions
                }
            }
        }

        Ok(invoices)
    }

    /// Process a single billing cycle
    async fn process_billing_cycle(
        &self,
        subscription: &Subscription,
    ) -> Result<SubscriptionInvoice> {
        let now = Utc::now();
        let cycle_number = subscription.current_cycle + 1;

        // Calculate billing period
        let (period_start, period_end) = self.calculate_billing_period(
            subscription.interval,
            subscription.interval_count,
            subscription.next_billing_at,
        );

        // Check if max cycles reached
        if let Some(max_cycles) = subscription.max_cycles {
            if cycle_number > max_cycles {
                // Expire subscription
                self.repository
                    .update(
                        subscription.id,
                        UpdateSubscriptionRequest {
                            status: Some(SubscriptionStatus::Expired),
                            ..Default::default()
                        },
                    )
                    .await?;
                return Err(Error::validation("Maximum billing cycles reached"));
            }
        }

        // Create invoice
        let invoice = SubscriptionInvoice {
            id: Uuid::new_v4(),
            subscription_id: subscription.id,
            order_id: None,
            cycle_number,
            period_start,
            period_end,
            subtotal: subscription.amount,
            tax_total: Decimal::ZERO, // TODO: Calculate tax
            total: subscription.amount,
            status: InvoiceStatus::Billed,
            paid_at: None,
            payment_id: None,
            failed_attempts: 0,
            last_failed_at: None,
            failure_reason: None,
            next_retry_at: None,
            retry_count: 0,
            created_at: now,
            updated_at: now,
        };

        let invoice = self.repository.create_invoice(invoice).await?;

        // Update subscription
        let next_billing = self.calculate_next_billing_date(
            subscription.interval,
            subscription.interval_count,
            subscription.next_billing_at,
        );

        self.repository.increment_cycle(subscription.id).await?;
        self.repository
            .update_next_billing(subscription.id, next_billing)
            .await?;

        Ok(invoice)
    }

    /// Calculate billing period dates
    fn calculate_billing_period(
        &self,
        interval: SubscriptionInterval,
        interval_count: i32,
        next_billing: DateTime<Utc>,
    ) -> (DateTime<Utc>, DateTime<Utc>) {
        let period_start = next_billing;
        let period_end = self.calculate_next_billing_date(interval, interval_count, next_billing);
        (period_start, period_end)
    }

    /// Calculate next billing date
    fn calculate_next_billing_date(
        &self,
        interval: SubscriptionInterval,
        interval_count: i32,
        current: DateTime<Utc>,
    ) -> DateTime<Utc> {
        next_billing_at(interval, interval_count, current)
    }

    // --- Payment Processing ---

    /// Record successful payment for an invoice
    pub async fn record_payment(
        &self,
        subscription_id: Uuid,
        invoice_id: Uuid,
        payment_id: String,
    ) -> Result<()> {
        // Mark invoice as paid
        self.repository
            .mark_invoice_paid(invoice_id, payment_id.clone())
            .await?;

        // Update subscription status if needed
        let subscription = self.get_subscription(subscription_id).await?;
        if matches!(
            subscription.status,
            SubscriptionStatus::PastDue | SubscriptionStatus::Trialing
        ) {
            self.repository
                .record_payment(subscription_id, payment_id)
                .await?;
        }

        Ok(())
    }

    /// Handle failed payment
    pub async fn handle_failed_payment(
        &self,
        subscription_id: Uuid,
        invoice_id: Uuid,
        error_message: String,
        error_code: Option<String>,
    ) -> Result<PaymentRecoveryResult> {
        let subscription = self.get_subscription(subscription_id).await?;
        let invoice = self
            .repository
            .get_invoice(invoice_id)
            .await?
            .ok_or_else(|| Error::not_found("Invoice not found"))?;

        // Mark invoice as failed
        self.repository
            .mark_invoice_failed(invoice_id, error_message.clone())
            .await?;

        // Record retry attempt
        let attempt_number = invoice.failed_attempts + 1;
        let retry_attempt = PaymentRetryAttempt {
            id: Uuid::new_v4(),
            subscription_id,
            invoice_id,
            attempt_number,
            attempted_at: Utc::now(),
            succeeded: false,
            error_message: Some(error_message),
            error_code,
            next_retry_at: None,
            payment_method_id: subscription.payment_method_id.clone(),
            gateway_transaction_id: None,
            created_at: Utc::now(),
        };

        self.repository.record_retry_attempt(retry_attempt).await?;

        // Determine next action based on retry count
        if attempt_number >= self.dunning_config.max_retries {
            // All retries exhausted - cancel subscription
            self.cancel_subscription(
                subscription_id,
                CancelSubscriptionRequest {
                    reason: CancellationReason::PaymentFailed,
                    reason_details: Some("All payment retry attempts failed".to_string()),
                    cancel_at_end: false,
                },
            )
            .await?;

            // Send final notice email
            self.send_dunning_email(
                subscription_id,
                invoice_id,
                DunningEmailType::CancellationNotice,
            )
            .await?;

            return Ok(PaymentRecoveryResult::FailedPermanent {
                cancelled_at: Utc::now(),
                reason: "All payment retry attempts failed".to_string(),
            });
        }

        // Schedule next retry
        let retry_days = self
            .dunning_config
            .retry_intervals_days
            .get(attempt_number as usize - 1)
            .copied()
            .unwrap_or(7); // Default to 7 days if not configured

        let next_retry = Utc::now() + Duration::days(retry_days as i64);

        // Update subscription status to past_due
        self.repository
            .record_failed_payment(subscription_id, "Payment failed".to_string())
            .await?;

        // Send retry notification email
        let email_type = if attempt_number == 1 {
            DunningEmailType::FirstFailure
        } else {
            DunningEmailType::RetryFailure
        };
        self.send_dunning_email(subscription_id, invoice_id, email_type)
            .await?;

        Ok(PaymentRecoveryResult::RetryScheduled {
            next_retry_at: next_retry,
            attempt_number,
            max_attempts: self.dunning_config.max_retries,
        })
    }

    /// Send dunning email
    async fn send_dunning_email(
        &self,
        subscription_id: Uuid,
        invoice_id: Uuid,
        email_type: DunningEmailType,
    ) -> Result<()> {
        // TODO: Integrate with email service
        // For now, just record that email was sent

        let email = DunningEmail {
            id: Uuid::new_v4(),
            subscription_id,
            invoice_id,
            email_type,
            subject: format!("Payment {:?} Notification", email_type),
            body_html: String::new(), // TODO: Generate HTML body
            body_text: String::new(), // TODO: Generate text body
            sent_at: Utc::now(),
            opened_at: None,
            clicked_at: None,
            created_at: Utc::now(),
        };

        self.repository.record_dunning_email(email).await?;

        Ok(())
    }

    // --- Statistics ---

    /// Get subscription summary statistics
    pub async fn get_summary(&self) -> Result<SubscriptionSummary> {
        let status_counts = self.repository.get_status_counts().await?;

        let mut total_active = 0i64;
        let mut total_cancelled = 0i64;
        let mut total_expired = 0i64;
        let mut total_past_due = 0i64;

        for (status, count) in status_counts {
            match status {
                SubscriptionStatus::Active | SubscriptionStatus::Trialing => total_active += count,
                SubscriptionStatus::Cancelled => total_cancelled += count,
                SubscriptionStatus::Expired => total_expired += count,
                SubscriptionStatus::PastDue => total_past_due += count,
                _ => {}
            }
        }

        let mrr = self.repository.calculate_mrr().await?;
        let arr = self.repository.calculate_arr().await?;

        Ok(SubscriptionSummary {
            total_active,
            total_cancelled,
            total_expired,
            total_past_due,
            monthly_recurring_revenue: mrr,
            annual_recurring_revenue: arr,
        })
    }

    /// Calculate Monthly Recurring Revenue
    pub async fn calculate_mrr(&self) -> Result<Decimal> {
        self.repository.calculate_mrr().await
    }

    /// Calculate Annual Recurring Revenue
    pub async fn calculate_arr(&self) -> Result<Decimal> {
        self.repository.calculate_arr().await
    }
}

/// Input for creating a subscription from a paid initial order
#[derive(Debug, Clone)]
pub struct CreateSubscriptionFromOrder {
    pub customer_id: Uuid,
    pub order_id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    /// Units of the product per billing cycle
    pub quantity: i32,
    /// Amount charged per billing cycle (order total at signup)
    pub amount: Decimal,
    pub currency: crate::models::Currency,
    pub interval: SubscriptionInterval,
    pub interval_count: i32,
    /// Verified stored-card consent id (billed for renewals)
    pub payment_consent_id: String,
    pub gateway: String,
    /// Shipping address snapshot for renewals
    pub shipping_address: Option<serde_json::Value>,
}

/// Summary of a billing-loop run
#[derive(Debug, Clone, Default)]
pub struct BillingReport {
    pub processed: usize,
    pub charged: usize,
    pub failed: usize,
    /// Renewal orders that were successfully charged (used by the API layer
    /// to send order-confirmation emails)
    pub charged_order_ids: Vec<Uuid>,
}

/// Postgres-backed money movement: renewal order creation + consent charging.
impl SubscriptionService<crate::repository::PostgresSubscriptionRepository> {
    /// Process every due subscription: create a renewal order, charge the
    /// stored-card consent, and record the outcome (invoice, payment, dunning).
    ///
    /// Per-subscription errors are logged and counted; one bad subscription
    /// never aborts the run.
    pub async fn process_due_subscriptions_with_charging(
        &self,
        payment_gateway: &std::sync::Arc<dyn crate::payment::PaymentGateway>,
        order_service: &std::sync::Arc<crate::order::OrderService>,
    ) -> Result<BillingReport> {
        let now = Utc::now();
        let due_subscriptions = self.repository.get_due_for_billing(now).await?;

        let mut report = BillingReport {
            processed: due_subscriptions.len(),
            ..Default::default()
        };

        for subscription in due_subscriptions {
            match self
                .charge_billing_cycle(&subscription, payment_gateway, order_service)
                .await
            {
                Ok(order_id) => {
                    report.charged += 1;
                    report.charged_order_ids.push(order_id);
                }
                Err(e) => {
                    report.failed += 1;
                    tracing::error!("Failed to bill subscription {}: {}", subscription.id, e);
                }
            }
        }

        Ok(report)
    }

    /// Bills one subscription. Returns the renewal order id on success.
    async fn charge_billing_cycle(
        &self,
        subscription: &Subscription,
        payment_gateway: &std::sync::Arc<dyn crate::payment::PaymentGateway>,
        order_service: &std::sync::Arc<crate::order::OrderService>,
    ) -> Result<Uuid> {
        let now = Utc::now();
        let cycle_number = subscription.current_cycle + 1;

        // Max cycles reached: expire and stop billing.
        if let Some(max_cycles) = subscription.max_cycles {
            if cycle_number > max_cycles {
                self.repository
                    .update(
                        subscription.id,
                        UpdateSubscriptionRequest {
                            status: Some(SubscriptionStatus::Expired),
                            ..Default::default()
                        },
                    )
                    .await?;
                return Err(Error::validation("Maximum billing cycles reached"));
            }
        }

        let consent_id = subscription
            .payment_method_id
            .clone()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                Error::validation(format!(
                    "Subscription {} has no stored-card consent to bill",
                    subscription.id
                ))
            })?;

        let customer_email = self
            .repository
            .customer_email(subscription.customer_id)
            .await?
            .unwrap_or_default();

        // 1. Renewal order (reserves inventory like a normal order).
        let quantity = subscription.quantity.max(1);
        let unit_price =
            crate::services::checkout_service::renewal_unit_price(subscription.amount, quantity);
        let line_subtotal = unit_price * Decimal::from(quantity);

        let order = order_service
            .create_order(crate::order::CreateOrderRequest {
                customer_id: Some(subscription.customer_id),
                customer_email: customer_email.clone(),
                billing_address_id: None,
                shipping_address_id: None,
                billing_address: None,
                shipping_address: subscription.shipping_address.clone(),
                items: vec![crate::order::CreateOrderItem {
                    product_id: subscription.product_id,
                    variant_id: subscription.variant_id,
                    quantity,
                    price: unit_price,
                    tax_amount: Decimal::ZERO,
                    skip_inventory_reservation: false,
                }],
                currency: subscription.currency.to_string(),
                subtotal: line_subtotal,
                tax_total: Decimal::ZERO,
                shipping_total: Decimal::ZERO,
                discount_total: Decimal::ZERO,
                total: line_subtotal,
                notes: Some(format!("Subscription renewal — cycle {}", cycle_number)),
                tags: None,
                metadata: serde_json::json!({
                    "subscription_id": subscription.id,
                    "billing_cycle": cycle_number,
                }),
                coupon_code: None,
                coupon_id: None,
                test: false,
            })
            .await?;

        // Annotate the order as a subscription renewal (OrderService creates
        // everything as one-time; the renewal link lives on the order row).
        sqlx::query(
            "UPDATE orders SET order_type = 'subscription_renewal', subscription_id = $1, \
             billing_cycle = $2 WHERE id = $3",
        )
        .bind(subscription.id)
        .bind(cycle_number)
        .bind(order.id)
        .execute(self.repository.pool())
        .await
        .map_err(Error::Database)?;

        // 2. Invoice for this cycle.
        let (period_start, period_end) = (
            subscription.next_billing_at,
            next_billing_at(
                subscription.interval,
                subscription.interval_count,
                subscription.next_billing_at,
            ),
        );
        let invoice = SubscriptionInvoice {
            id: Uuid::new_v4(),
            subscription_id: subscription.id,
            order_id: Some(order.id),
            cycle_number,
            period_start,
            period_end,
            subtotal: subscription.amount,
            tax_total: Decimal::ZERO, // TODO: Calculate tax
            total: subscription.amount,
            status: InvoiceStatus::Billed,
            paid_at: None,
            payment_id: None,
            failed_attempts: 0,
            last_failed_at: None,
            failure_reason: None,
            next_retry_at: None,
            retry_count: 0,
            created_at: now,
            updated_at: now,
        };
        let invoice = self.repository.create_invoice(invoice).await?;

        // 3. Charge the stored-card consent (merchant-initiated).
        let charge_result = payment_gateway
            .create_payment(crate::payment::CreatePaymentRequest {
                amount: subscription.amount,
                currency: subscription.currency.to_string(),
                order_id: order.id,
                customer_id: Some(subscription.customer_id),
                customer_email,
                payment_method: crate::payment::PaymentMethod::SavedCard,
                billing_address: None,
                payment_consent_id: Some(consent_id.clone()),
                merchant_initiated: Some(true),
                metadata: serde_json::json!({
                    "order_id": order.invoice_number.clone().unwrap_or_else(|| order.order_number.clone()),
                    "subscription_id": subscription.id,
                    "billing_cycle": cycle_number,
                }),
            })
            .await;

        let session = match charge_result {
            Ok(session) => session,
            Err(e) => {
                return self
                    .handle_cycle_failure(
                        subscription,
                        &invoice,
                        order.id,
                        &format!("Gateway charge failed: {}", e),
                        payment_gateway,
                        order_service,
                    )
                    .await;
            }
        };

        // The consent charge can succeed synchronously or stay pending; ask
        // the gateway for the authoritative status.
        let payment = match payment_gateway.get_payment(&session.id).await {
            Ok(payment) => payment,
            Err(e) => {
                return self
                    .handle_cycle_failure(
                        subscription,
                        &invoice,
                        order.id,
                        &format!("Failed to retrieve charge status: {}", e),
                        payment_gateway,
                        order_service,
                    )
                    .await;
            }
        };

        if payment.status != crate::payment::PaymentStatus::Succeeded {
            return self
                .handle_cycle_failure(
                    subscription,
                    &invoice,
                    order.id,
                    &format!("Charge not succeeded: {:?}", payment.status),
                    payment_gateway,
                    order_service,
                )
                .await;
        }

        // 4. Success: record payment, confirm order, close the invoice,
        //    advance the billing schedule.
        let succeeded_payment = crate::payment::Payment {
            id: session.id.clone(),
            gateway: payment_gateway.id().to_string(),
            amount: subscription.amount,
            currency: subscription.currency.to_string(),
            status: crate::payment::PaymentStatus::Succeeded,
            order_id: order.id,
            customer_id: Some(subscription.customer_id),
            payment_method: "card".to_string(),
            transaction_id: session.id.clone(),
            client_secret: None,
            card_brand: payment.card_brand,
            card_last_four: payment.card_last_four,
            captured_at: Some(now),
            created_at: now,
        };
        if let Err(e) = order_service
            .record_payment(order.id, &succeeded_payment)
            .await
        {
            tracing::warn!(
                "Failed to record renewal payment for order {}: {}",
                order.id,
                e
            );
        }
        if let Err(e) = order_service.confirm_order_after_payment(order.id).await {
            tracing::warn!("Failed to confirm renewal order {}: {}", order.id, e);
        }

        self.repository
            .mark_invoice_paid(invoice.id, session.id.clone())
            .await?;
        self.repository.increment_cycle(subscription.id).await?;
        self.repository
            .update_next_billing(subscription.id, period_end)
            .await?;

        tracing::info!(
            "Subscription {} billed: cycle {} charged {} {} (order {})",
            subscription.id,
            cycle_number,
            subscription.amount,
            subscription.currency,
            order.id
        );

        Ok(order.id)
    }

    /// A renewal charge failed: mark the order/payment/invoice failed, put the
    /// subscription past_due, and hand the invoice to the dunning machinery.
    async fn handle_cycle_failure(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
        order_id: Uuid,
        reason: &str,
        payment_gateway: &std::sync::Arc<dyn crate::payment::PaymentGateway>,
        order_service: &std::sync::Arc<crate::order::OrderService>,
    ) -> Result<Uuid> {
        tracing::warn!(
            "Renewal charge failed for subscription {} (invoice {}): {}",
            subscription.id,
            invoice.id,
            reason
        );

        // Order + payment rows reflect the failure.
        sqlx::query(
            "UPDATE orders SET payment_status = 'failed', updated_at = NOW() WHERE id = $1",
        )
        .bind(order_id)
        .execute(self.repository.pool())
        .await
        .map_err(Error::Database)?;

        let failed_payment = crate::payment::Payment {
            id: format!("renewal_failed_{}", invoice.id),
            gateway: payment_gateway.id().to_string(),
            amount: invoice.total,
            currency: subscription.currency.to_string(),
            status: crate::payment::PaymentStatus::Failed,
            order_id,
            customer_id: Some(subscription.customer_id),
            payment_method: "card".to_string(),
            transaction_id: String::new(),
            client_secret: None,
            card_brand: None,
            card_last_four: None,
            captured_at: None,
            created_at: Utc::now(),
        };
        let _ = order_service
            .record_payment(order_id, &failed_payment)
            .await;

        // Invoice + subscription state for dunning.
        self.repository
            .mark_invoice_failed(invoice.id, reason.to_string())
            .await?;
        self.repository
            .record_failed_payment(subscription.id, reason.to_string())
            .await?;

        let attempt_number = invoice.failed_attempts + 1;
        let retry_days = self
            .dunning_config
            .retry_intervals_days
            .get(attempt_number as usize - 1)
            .copied()
            .unwrap_or(7);
        let next_retry = Utc::now() + Duration::days(retry_days as i64);

        self.repository
            .record_retry_attempt(PaymentRetryAttempt {
                id: Uuid::new_v4(),
                subscription_id: subscription.id,
                invoice_id: invoice.id,
                attempt_number,
                attempted_at: Utc::now(),
                succeeded: false,
                error_message: Some(reason.to_string()),
                error_code: None,
                next_retry_at: Some(next_retry),
                payment_method_id: subscription.payment_method_id.clone(),
                gateway_transaction_id: None,
                created_at: Utc::now(),
            })
            .await?;
        self.repository
            .schedule_invoice_retry(invoice.id, next_retry)
            .await?;

        Err(Error::payment_error(format!(
            "Renewal charge failed (dunning attempt {} of {}): {}",
            attempt_number, self.dunning_config.max_retries, reason
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock repository for testing
    struct MockSubscriptionRepository;

    #[async_trait::async_trait]
    impl SubscriptionRepository for MockSubscriptionRepository {
        async fn create(&self, _request: CreateSubscriptionRequest) -> Result<Subscription> {
            unimplemented!()
        }

        async fn find_by_id(&self, _id: Uuid) -> Result<Option<Subscription>> {
            unimplemented!()
        }

        async fn list(
            &self,
            _filter: &SubscriptionFilter,
            _page: i64,
            _per_page: i64,
        ) -> Result<Vec<Subscription>> {
            unimplemented!()
        }

        async fn count(&self, _filter: &SubscriptionFilter) -> Result<i64> {
            unimplemented!()
        }

        async fn update(
            &self,
            _id: Uuid,
            _request: UpdateSubscriptionRequest,
        ) -> Result<Subscription> {
            unimplemented!()
        }

        async fn cancel(
            &self,
            _id: Uuid,
            _request: CancelSubscriptionRequest,
        ) -> Result<Subscription> {
            unimplemented!()
        }

        async fn pause(&self, _id: Uuid) -> Result<Subscription> {
            unimplemented!()
        }

        async fn resume(&self, _id: Uuid) -> Result<Subscription> {
            unimplemented!()
        }

        async fn list_by_customer(
            &self,
            _customer_id: Uuid,
            _status: Option<SubscriptionStatus>,
        ) -> Result<Vec<Subscription>> {
            unimplemented!()
        }

        async fn list_by_product(&self, _product_id: Uuid) -> Result<Vec<Subscription>> {
            unimplemented!()
        }

        async fn get_due_for_billing(&self, _before: DateTime<Utc>) -> Result<Vec<Subscription>> {
            unimplemented!()
        }

        async fn update_next_billing(
            &self,
            _id: Uuid,
            _next_billing_at: DateTime<Utc>,
        ) -> Result<()> {
            unimplemented!()
        }

        async fn increment_cycle(&self, _id: Uuid) -> Result<()> {
            unimplemented!()
        }

        async fn record_payment(&self, _id: Uuid, _payment_id: String) -> Result<()> {
            unimplemented!()
        }

        async fn record_failed_payment(&self, _id: Uuid, _failure_reason: String) -> Result<()> {
            unimplemented!()
        }

        async fn create_invoice(
            &self,
            _invoice: SubscriptionInvoice,
        ) -> Result<SubscriptionInvoice> {
            unimplemented!()
        }

        async fn get_invoice(&self, _invoice_id: Uuid) -> Result<Option<SubscriptionInvoice>> {
            unimplemented!()
        }

        async fn list_invoices(&self, _subscription_id: Uuid) -> Result<Vec<SubscriptionInvoice>> {
            unimplemented!()
        }

        async fn mark_invoice_paid(&self, _invoice_id: Uuid, _payment_id: String) -> Result<()> {
            unimplemented!()
        }

        async fn mark_invoice_failed(
            &self,
            _invoice_id: Uuid,
            _failure_reason: String,
        ) -> Result<()> {
            unimplemented!()
        }

        async fn get_pending_invoices(&self) -> Result<Vec<SubscriptionInvoice>> {
            unimplemented!()
        }

        async fn record_retry_attempt(&self, _attempt: PaymentRetryAttempt) -> Result<()> {
            unimplemented!()
        }

        async fn get_retry_attempts(&self, _invoice_id: Uuid) -> Result<Vec<PaymentRetryAttempt>> {
            unimplemented!()
        }

        async fn record_dunning_email(&self, _email: DunningEmail) -> Result<()> {
            unimplemented!()
        }

        async fn get_dunning_emails(&self, _subscription_id: Uuid) -> Result<Vec<DunningEmail>> {
            unimplemented!()
        }

        async fn get_status_counts(&self) -> Result<Vec<(SubscriptionStatus, i64)>> {
            unimplemented!()
        }

        async fn calculate_mrr(&self) -> Result<Decimal> {
            Ok(Decimal::new(10000, 2)) // $100.00
        }

        async fn calculate_arr(&self) -> Result<Decimal> {
            Ok(Decimal::new(120000, 2)) // $1200.00
        }
    }

    #[test]
    fn test_calculate_next_billing_date() {
        let repo = MockSubscriptionRepository;
        let service = SubscriptionService::new(repo);

        let base = Utc::now();

        // Test monthly
        let next = service.calculate_next_billing_date(SubscriptionInterval::Monthly, 1, base);
        assert!(next > base);

        // Test yearly
        let next = service.calculate_next_billing_date(SubscriptionInterval::Annually, 1, base);
        assert!(next > base);
    }

    #[test]
    fn test_next_billing_at_bi_weekly_and_monthly() {
        let base = Utc::now();

        // Bi-weekly = 14 days
        let next = next_billing_at(SubscriptionInterval::BiWeekly, 1, base);
        assert_eq!(next, base + Duration::weeks(2));

        let next = next_billing_at(SubscriptionInterval::BiWeekly, 2, base);
        assert_eq!(next, base + Duration::weeks(4));

        // Monthly uses calendar months
        let next = next_billing_at(SubscriptionInterval::Monthly, 1, base);
        let expected = base.checked_add_months(chrono::Months::new(1)).unwrap();
        assert_eq!(next, expected);

        // Monthly x3 = quarterly
        let next = next_billing_at(SubscriptionInterval::Monthly, 3, base);
        let expected = base.checked_add_months(chrono::Months::new(3)).unwrap();
        assert_eq!(next, expected);

        // Interval count respected for weekly
        let next = next_billing_at(SubscriptionInterval::Weekly, 3, base);
        assert_eq!(next, base + Duration::weeks(3));
    }
}
