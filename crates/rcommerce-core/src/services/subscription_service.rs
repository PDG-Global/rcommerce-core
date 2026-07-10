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
}
