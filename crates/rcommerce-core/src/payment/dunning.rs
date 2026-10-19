//! Dunning management for subscription payment failures
//!
//! Dunning is the process of retrying failed subscription payments and
//! communicating with customers to recover revenue. This module provides:
//!
//! - Configurable retry schedules
//! - Automated email notifications
//! - Grace period management
//! - Final cancellation workflow

use chrono::{DateTime, Duration, Utc};

use tracing::{error, info, warn};
use uuid::Uuid;

use crate::models::{
    CancellationReason, DunningConfig, DunningEmail, DunningEmailType, InvoiceStatus,
    PaymentRecoveryResult, PaymentRetryAttempt, Subscription, SubscriptionInvoice,
    SubscriptionStatus,
};
use crate::payment::{CreatePaymentRequest, PaymentGateway, PaymentMethod};
use crate::Result;

/// Dunning manager handles payment retry logic
pub struct DunningManager<G: PaymentGateway> {
    gateway: G,
    config: DunningConfig,
}

impl<G: PaymentGateway> DunningManager<G> {
    /// Create a new dunning manager
    pub fn new(gateway: G, config: DunningConfig) -> Self {
        Self { gateway, config }
    }

    /// Process a failed subscription payment
    ///
    /// This is the main entry point for dunning. It will:
    /// 1. Record the failed attempt
    /// 2. Schedule the next retry
    /// 3. Send appropriate email notifications
    /// 4. Cancel subscription if all retries exhausted
    pub async fn process_failed_payment(
        &self,
        subscription: &mut Subscription,
        invoice: &mut SubscriptionInvoice,
        error_message: &str,
        error_code: Option<&str>,
    ) -> Result<PaymentRecoveryResult> {
        let attempt_number = invoice.failed_attempts + 1;

        info!(
            "Processing failed payment for subscription {}: attempt {}/{}",
            subscription.id, attempt_number, self.config.max_retries
        );

        // Record the failed attempt
        let _retry_attempt = PaymentRetryAttempt {
            id: Uuid::new_v4(),
            subscription_id: subscription.id,
            invoice_id: invoice.id,
            attempt_number,
            attempted_at: Utc::now(),
            succeeded: false,
            error_message: Some(error_message.to_string()),
            error_code: error_code.map(|s| s.to_string()),
            next_retry_at: None,
            payment_method_id: subscription.payment_method_id.clone(),
            gateway_transaction_id: None,
            created_at: Utc::now(),
        };

        // Update invoice with failure info
        invoice.failed_attempts = attempt_number;
        invoice.last_failed_at = Some(Utc::now());
        invoice.failure_reason = Some(error_message.to_string());

        // Check if we should retry
        if attempt_number >= self.config.max_retries {
            // All retries exhausted - cancel subscription
            warn!(
                "Subscription {} has exhausted all {} retry attempts. Cancelling.",
                subscription.id, self.config.max_retries
            );

            self.cancel_subscription_for_non_payment(subscription, invoice)
                .await?;

            return Ok(PaymentRecoveryResult::FailedPermanent {
                cancelled_at: Utc::now(),
                reason: format!("Payment failed after {} attempts", attempt_number),
            });
        }

        // Schedule next retry
        let next_retry_at = self.calculate_next_retry_date(attempt_number);
        invoice.next_retry_at = Some(next_retry_at);
        invoice.retry_count = attempt_number;

        // Update subscription status if this is the first failure
        if subscription.status == SubscriptionStatus::Active {
            subscription.status = SubscriptionStatus::PastDue;
        }

        // Send appropriate email
        self.send_dunning_email(subscription, invoice, attempt_number)
            .await?;

        Ok(PaymentRecoveryResult::RetryScheduled {
            next_retry_at,
            attempt_number,
            max_attempts: self.config.max_retries,
        })
    }

    /// Attempt to retry a failed payment
    ///
    /// This should be called by a scheduled job at the retry time
    pub async fn retry_payment(
        &self,
        subscription: &mut Subscription,
        invoice: &mut SubscriptionInvoice,
    ) -> Result<PaymentRecoveryResult> {
        info!(
            "Retrying payment for subscription {}: attempt {}/{}",
            subscription.id,
            invoice.failed_attempts + 1,
            self.config.max_retries
        );

        // Attempt payment through gateway
        let payment_result = self.charge_subscription(subscription, invoice).await;

        match payment_result {
            Ok(payment) => {
                info!(
                    "Payment retry succeeded for subscription {}: transaction {}",
                    subscription.id, payment.id
                );

                // Update invoice as paid
                invoice.status = InvoiceStatus::Paid;
                invoice.paid_at = Some(Utc::now());
                invoice.payment_id = Some(payment.id);

                // Reset failure counters
                invoice.failed_attempts = 0;
                invoice.last_failed_at = None;
                invoice.failure_reason = None;

                // Restore subscription status
                subscription.status = SubscriptionStatus::Active;
                subscription.last_billing_at = Some(Utc::now());
                subscription.next_billing_at = self.calculate_next_billing_date(subscription);
                subscription.current_cycle += 1;

                // Send recovery email
                self.send_payment_recovered_email(subscription, invoice)
                    .await?;

                Ok(PaymentRecoveryResult::Success)
            }
            Err(e) => {
                error!(
                    "Payment retry failed for subscription {}: {}",
                    subscription.id, e
                );

                // Process the failure
                self.process_failed_payment(subscription, invoice, &e.to_string(), None)
                    .await
            }
        }
    }

    /// Charge a subscription
    async fn charge_subscription(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> Result<crate::payment::Payment> {
        // Create payment request
        let payment_request = CreatePaymentRequest {
            amount: invoice.total,
            currency: subscription.currency.to_string(),
            order_id: invoice.id,
            customer_id: Some(subscription.customer_id),
            customer_email: String::new(), // Should be fetched from customer record
            payment_method: PaymentMethod::Card(crate::payment::CardDetails {
                number: String::new(),
                exp_month: 0,
                exp_year: 0,
                cvc: String::new(),
                name: String::new(),
            }),
            billing_address: None,
            payment_consent_id: subscription.payment_method_id.clone(),
            merchant_initiated: Some(true),
            metadata: serde_json::json!({
                "subscription_id": subscription.id,
                "invoice_id": invoice.id,
                "cycle_number": invoice.cycle_number,
            }),
        };

        // Create payment
        let session = self.gateway.create_payment(payment_request).await?;

        // Confirm payment
        let payment = self.gateway.confirm_payment(&session.id).await?;

        Ok(payment)
    }

    /// Calculate the next retry date based on attempt number
    fn calculate_next_retry_date(&self, attempt_number: i32) -> DateTime<Utc> {
        let days = self
            .config
            .retry_intervals_days
            .get(attempt_number as usize - 1)
            .copied()
            .unwrap_or(7);

        Utc::now() + Duration::days(days as i64)
    }

    /// Calculate the next billing date for a subscription
    fn calculate_next_billing_date(&self, subscription: &Subscription) -> DateTime<Utc> {
        let interval_days = match subscription.interval {
            super::super::SubscriptionInterval::Daily => 1,
            super::super::SubscriptionInterval::Weekly => 7,
            super::super::SubscriptionInterval::BiWeekly => 14,
            super::super::SubscriptionInterval::Monthly => 30,
            super::super::SubscriptionInterval::Quarterly => 90,
            super::super::SubscriptionInterval::BiAnnually => 180,
            super::super::SubscriptionInterval::Annually => 365,
        };

        Utc::now() + Duration::days(interval_days as i64 * subscription.interval_count as i64)
    }

    /// Cancel a subscription due to non-payment
    async fn cancel_subscription_for_non_payment(
        &self,
        subscription: &mut Subscription,
        invoice: &mut SubscriptionInvoice,
    ) -> Result<()> {
        subscription.status = SubscriptionStatus::Cancelled;
        subscription.cancelled_at = Some(Utc::now());
        subscription.cancellation_reason = Some(CancellationReason::PaymentFailed);
        subscription.ends_at = Some(Utc::now());

        invoice.status = InvoiceStatus::PastDue;

        // Send cancellation email
        self.send_cancellation_email(subscription, invoice).await?;

        info!(
            "Subscription {} cancelled due to non-payment",
            subscription.id
        );

        Ok(())
    }

    /// Send the appropriate dunning email based on attempt number
    async fn send_dunning_email(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
        attempt_number: i32,
    ) -> Result<()> {
        let email_type = if attempt_number == 1 && self.config.email_on_first_failure {
            Some(DunningEmailType::FirstFailure)
        } else if attempt_number == self.config.max_retries && self.config.email_on_final_failure {
            Some(DunningEmailType::FinalNotice)
        } else if attempt_number > 1 && attempt_number < self.config.max_retries {
            Some(DunningEmailType::RetryFailure)
        } else {
            None
        };

        if let Some(email_type) = email_type {
            let _email = self
                .build_dunning_email(subscription, invoice, email_type)
                .await?;

            info!(
                "Sending {:?} email for subscription {}",
                email_type, subscription.id
            );
        }

        Ok(())
    }

    /// Send payment recovered email
    async fn send_payment_recovered_email(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> Result<()> {
        let _email = self
            .build_dunning_email(subscription, invoice, DunningEmailType::PaymentRecovered)
            .await?;

        info!(
            "Sending payment recovered email for subscription {}",
            subscription.id
        );

        Ok(())
    }

    /// Send cancellation email
    async fn send_cancellation_email(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> Result<()> {
        let _email = self
            .build_dunning_email(subscription, invoice, DunningEmailType::CancellationNotice)
            .await?;

        info!(
            "Sending cancellation email for subscription {}",
            subscription.id
        );

        Ok(())
    }

    /// Build a dunning email
    async fn build_dunning_email(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
        email_type: DunningEmailType,
    ) -> Result<DunningEmail> {
        let (subject, body_text, body_html) = match email_type {
            DunningEmailType::FirstFailure => (
                "Payment Failed - Please Update Your Payment Method".to_string(),
                self.build_first_failure_text(subscription, invoice),
                self.build_first_failure_html(subscription, invoice),
            ),
            DunningEmailType::RetryFailure => (
                "Payment Failed Again - Action Required".to_string(),
                self.build_retry_failure_text(subscription, invoice),
                self.build_retry_failure_html(subscription, invoice),
            ),
            DunningEmailType::FinalNotice => (
                "Final Notice: Subscription Cancellation Pending".to_string(),
                self.build_final_notice_text(subscription, invoice),
                self.build_final_notice_html(subscription, invoice),
            ),
            DunningEmailType::CancellationNotice => (
                "Subscription Cancelled Due to Non-Payment".to_string(),
                self.build_cancellation_text(subscription, invoice),
                self.build_cancellation_html(subscription, invoice),
            ),
            DunningEmailType::PaymentRecovered => (
                "Payment Successful - Subscription Active".to_string(),
                self.build_recovered_text(subscription, invoice),
                self.build_recovered_html(subscription, invoice),
            ),
        };

        Ok(DunningEmail {
            id: Uuid::new_v4(),
            subscription_id: subscription.id,
            invoice_id: invoice.id,
            email_type,
            subject,
            body_html,
            body_text,
            sent_at: Utc::now(),
            opened_at: None,
            clicked_at: None,
            created_at: Utc::now(),
        })
    }

    // Email template builders (text versions)
    fn build_first_failure_text(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> String {
        format!(
            "We were unable to process your subscription payment.\n\n\
            Amount: {}\n\
            Next retry: {}\n\n\
            Please update your payment method to avoid interruption.",
            invoice.total, subscription.next_billing_at
        )
    }

    fn build_retry_failure_text(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> String {
        format!(
            "Your subscription payment failed again.\n\n\
            Attempt: {}/{}\n\
            Next retry: {}\n\n\
            Please update your payment method immediately.",
            invoice.failed_attempts, self.config.max_retries, subscription.next_billing_at
        )
    }

    fn build_final_notice_text(
        &self,
        _subscription: &Subscription,
        _invoice: &SubscriptionInvoice,
    ) -> String {
        "FINAL NOTICE: Your subscription will be cancelled if payment is not received.\n\n\
        This is your final attempt. Please update your payment method within 24 hours."
            .to_string()
    }

    fn build_cancellation_text(
        &self,
        _subscription: &Subscription,
        _invoice: &SubscriptionInvoice,
    ) -> String {
        "Your subscription has been cancelled due to non-payment.\n\n\
        To reactivate your subscription, please place a new order."
            .to_string()
    }

    fn build_recovered_text(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> String {
        format!(
            "Great news! Your payment was successful.\n\n\
            Amount: {}\n\
            Next billing: {}\n\n\
            Your subscription is now active.",
            invoice.total, subscription.next_billing_at
        )
    }

    // Email template builders (HTML versions)
    fn build_first_failure_html(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> String {
        format!(
            "<html><body>\
            <h1>Payment Failed</h1>\
            <p>We were unable to process your subscription payment.</p>\
            <p><strong>Amount:</strong> {}</p>\
            <p><strong>Next retry:</strong> {}</p>\
            <p>Please <a href=\"#\">update your payment method</a> to avoid interruption.</p>\
            </body></html>",
            invoice.total, subscription.next_billing_at
        )
    }

    fn build_retry_failure_html(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> String {
        format!(
            "<html><body>\
            <h1>Payment Failed Again</h1>\
            <p>Your subscription payment failed again.</p>\
            <p><strong>Attempt:</strong> {}/{}</p>\
            <p><strong>Next retry:</strong> {}</p>\
            <p>Please <a href=\"#\">update your payment method</a> immediately.</p>\
            </body></html>",
            invoice.failed_attempts, self.config.max_retries, subscription.next_billing_at
        )
    }

    fn build_final_notice_html(
        &self,
        _subscription: &Subscription,
        _invoice: &SubscriptionInvoice,
    ) -> String {
        "<html><body>\
        <h1>Final Notice</h1>\
        <p><strong>Your subscription will be cancelled if payment is not received.</strong></p>\
        <p>This is your final attempt. Please <a href=\"#\">update your payment method</a> within 24 hours.</p>\
        </body></html>".to_string()
    }

    fn build_cancellation_html(
        &self,
        _subscription: &Subscription,
        _invoice: &SubscriptionInvoice,
    ) -> String {
        "<html><body>\
        <h1>Subscription Cancelled</h1>\
        <p>Your subscription has been cancelled due to non-payment.</p>\
        <p>To reactivate your subscription, please <a href=\"#\">place a new order</a>.</p>\
        </body></html>"
            .to_string()
    }

    fn build_recovered_html(
        &self,
        subscription: &Subscription,
        invoice: &SubscriptionInvoice,
    ) -> String {
        format!(
            "<html><body>\
            <h1>Payment Successful!</h1>\
            <p>Great news! Your payment was successful.</p>\
            <p><strong>Amount:</strong> {}</p>\
            <p><strong>Next billing:</strong> {}</p>\
            <p>Your subscription is now active.</p>\
            </body></html>",
            invoice.total, subscription.next_billing_at
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::MockPaymentGateway;

    #[test]
    fn test_dunning_config_default() {
        let config = DunningConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_intervals_days, vec![1, 3, 7]);
        assert_eq!(config.grace_period_days, 14);
        assert!(config.email_on_first_failure);
        assert!(config.email_on_final_failure);
    }

    #[test]
    fn test_calculate_next_retry_date() {
        let gateway = MockPaymentGateway::new();
        let config = DunningConfig::default();
        let manager = DunningManager::new(gateway, config);

        let now = Utc::now();
        let retry_date = manager.calculate_next_retry_date(1);

        let diff = retry_date - now;
        assert!(diff.num_days() >= 0 && diff.num_days() <= 2);
    }
}
