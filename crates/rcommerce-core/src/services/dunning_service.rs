//! Dunning Service
//!
//! Business logic for payment retry management including:
//! - Processing failed payments
//! - Scheduling retry attempts
//! - Sending dunning emails
//! - Handling subscription status changes
//! - Managing payment recovery

use chrono::{DateTime, Duration, Utc};

use tracing::{error, info, warn};
use uuid::Uuid;

use crate::models::{
    CancelSubscriptionRequest, CancellationReason, DunningConfig, DunningEmail, DunningEmailType,
    InvoiceStatus, PaymentRecoveryResult, PaymentRetryAttempt, Subscription, SubscriptionInvoice,
    SubscriptionStatus, UpdateSubscriptionRequest,
};
use crate::repository::SubscriptionRepository;
use crate::{Error, Result};
// TODO: Integrate with email service when notification module is ready
// use crate::notification::email::{EmailService, EmailTemplate};

/// Placeholder email service
#[derive(Clone, Default)]
pub struct EmailService;

impl EmailService {
    pub fn new() -> Self {
        Self
    }
}

/// Dunning Service for managing payment retries
pub struct DunningService<R: SubscriptionRepository> {
    repository: R,
    config: DunningConfig,
    email_service: Option<EmailService>,
}

impl<R: SubscriptionRepository + Clone> Clone for DunningService<R> {
    fn clone(&self) -> Self {
        Self {
            repository: self.repository.clone(),
            config: self.config.clone(),
            email_service: self.email_service.clone(),
        }
    }
}

impl<R: SubscriptionRepository + Clone> DunningService<R> {
    /// Create a new dunning service with default configuration
    pub fn new(repository: R) -> Self {
        Self {
            repository,
            config: DunningConfig::default(),
            email_service: None,
        }
    }

    /// Create a new dunning service with custom configuration
    pub fn with_config(repository: R, config: DunningConfig) -> Self {
        Self {
            repository,
            config,
            email_service: None,
        }
    }

    /// Create a new dunning service with email support
    pub fn with_email_service(
        repository: R,
        config: DunningConfig,
        email_service: EmailService,
    ) -> Self {
        Self {
            repository,
            config,
            email_service: Some(email_service),
        }
    }

    /// Set email service
    pub fn set_email_service(&mut self, email_service: EmailService) {
        self.email_service = Some(email_service);
    }

    /// Update configuration
    pub fn set_config(&mut self, config: DunningConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn config(&self) -> &DunningConfig {
        &self.config
    }

    // --- Core Dunning Operations ---

    /// Process a failed payment
    ///
    /// This is the main entry point for dunning. It will:
    /// 1. Record the failed attempt
    /// 2. Schedule the next retry
    /// 3. Send appropriate email notifications
    /// 4. Cancel subscription if all retries exhausted
    pub async fn process_failed_payment(
        &self,
        subscription_id: Uuid,
        invoice_id: Uuid,
        error_message: &str,
    ) -> Result<PaymentRecoveryResult> {
        info!(
            "Processing failed payment for subscription {}: invoice {}",
            subscription_id, invoice_id
        );

        // Get subscription and invoice
        let subscription = self
            .repository
            .find_by_id(subscription_id)
            .await?
            .ok_or_else(|| Error::not_found("Subscription not found"))?;

        let invoice = self
            .repository
            .get_invoice(invoice_id)
            .await?
            .ok_or_else(|| Error::not_found("Invoice not found"))?;

        // Calculate attempt number
        let attempt_number = invoice.failed_attempts + 1;

        // Record the failed attempt
        let retry_attempt = PaymentRetryAttempt {
            id: Uuid::new_v4(),
            subscription_id,
            invoice_id,
            attempt_number,
            attempted_at: Utc::now(),
            succeeded: false,
            error_message: Some(error_message.to_string()),
            error_code: None,
            next_retry_at: None,
            payment_method_id: subscription.payment_method_id.clone(),
            gateway_transaction_id: None,
            created_at: Utc::now(),
        };
        self.repository.record_retry_attempt(retry_attempt).await?;

        // Mark invoice as failed
        self.repository
            .mark_invoice_failed(invoice_id, error_message.to_string())
            .await?;

        // Check if we should cancel
        if attempt_number >= self.config.max_retries {
            return self.cancel_after_retries(subscription_id).await;
        }

        // Schedule next retry
        let next_retry = self
            .schedule_retry(subscription_id, invoice_id, attempt_number)
            .await?;

        // Update subscription status to past_due
        self.repository
            .record_failed_payment(subscription_id, error_message.to_string())
            .await?;

        // Send appropriate dunning email
        let email_type = if attempt_number == 1 {
            DunningEmailType::FirstFailure
        } else if attempt_number == self.config.max_retries - 1 {
            DunningEmailType::FinalNotice
        } else {
            DunningEmailType::RetryFailure
        };

        self.send_dunning_email(subscription_id, invoice_id, email_type)
            .await?;

        Ok(PaymentRecoveryResult::RetryScheduled {
            next_retry_at: next_retry,
            attempt_number,
            max_attempts: self.config.max_retries,
        })
    }

    /// Schedule a retry attempt
    ///
    /// Calculates the next retry date based on the attempt number and configuration
    pub async fn schedule_retry(
        &self,
        subscription_id: Uuid,
        invoice_id: Uuid,
        attempt_number: i32,
    ) -> Result<DateTime<Utc>> {
        let days = self
            .config
            .retry_intervals_days
            .get(attempt_number as usize - 1)
            .copied()
            .unwrap_or(7);

        let next_retry = Utc::now() + Duration::days(days as i64);

        info!(
            "Scheduled retry for subscription {} invoice {}: attempt {} in {} days at {}",
            subscription_id,
            invoice_id,
            attempt_number + 1,
            days,
            next_retry
        );

        Ok(next_retry)
    }

    /// Execute a scheduled retry
    ///
    /// Attempts to process payment for an invoice that is due for retry.
    /// This should be called by the background job processor.
    pub async fn execute_retry(&self, invoice_id: Uuid) -> Result<PaymentRecoveryResult> {
        info!("Executing retry for invoice {}", invoice_id);

        let invoice = self
            .repository
            .get_invoice(invoice_id)
            .await?
            .ok_or_else(|| Error::not_found("Invoice not found"))?;

        // Check if invoice is still pending/billed
        if !matches!(
            invoice.status,
            InvoiceStatus::Billed | InvoiceStatus::Failed
        ) {
            return Err(Error::validation(format!(
                "Invoice {} is not in retryable status: {:?}",
                invoice_id, invoice.status
            )));
        }

        // Check if retry is due
        if let Some(next_retry) = invoice.next_retry_at {
            if next_retry > Utc::now() {
                return Err(Error::validation(format!(
                    "Retry for invoice {} is not due yet. Next retry at {}",
                    invoice_id, next_retry
                )));
            }
        }

        // Get subscription
        let subscription = self
            .repository
            .find_by_id(invoice.subscription_id)
            .await?
            .ok_or_else(|| Error::not_found("Subscription not found"))?;

        // Check if subscription is still active/past_due
        if !matches!(
            subscription.status,
            SubscriptionStatus::Active | SubscriptionStatus::PastDue
        ) {
            return Err(Error::validation(format!(
                "Subscription {} is not in retryable status: {:?}",
                subscription.id, subscription.status
            )));
        }

        // Attempt payment (this would integrate with payment gateway)
        // For now, we simulate the payment attempt
        info!(
            "Attempting payment retry for subscription {}: attempt {}/{}",
            subscription.id,
            invoice.failed_attempts + 1,
            self.config.max_retries
        );

        // TODO: Integrate with actual payment gateway
        // This is where you would call the payment gateway to retry the charge

        // For now, we return a retry scheduled result
        // In real implementation, this would either:
        // 1. Call process_recovery() on success
        // 2. Call process_failed_payment() on failure

        Ok(PaymentRecoveryResult::RetryScheduled {
            next_retry_at: Utc::now() + Duration::days(1),
            attempt_number: invoice.failed_attempts + 1,
            max_attempts: self.config.max_retries,
        })
    }

    /// Cancel subscription after max retries exhausted
    ///
    /// Permanently cancels a subscription due to non-payment
    pub async fn cancel_after_retries(
        &self,
        subscription_id: Uuid,
    ) -> Result<PaymentRecoveryResult> {
        warn!(
            "Subscription {} has exhausted all {} retry attempts. Cancelling.",
            subscription_id, self.config.max_retries
        );

        // Get the subscription's latest invoice
        let invoices = self.repository.list_invoices(subscription_id).await?;
        let latest_invoice = invoices
            .into_iter()
            .filter(|i| matches!(i.status, InvoiceStatus::Failed | InvoiceStatus::Billed))
            .max_by_key(|i| i.cycle_number);

        // Cancel the subscription
        let cancel_request = CancelSubscriptionRequest {
            reason: CancellationReason::PaymentFailed,
            reason_details: Some(format!(
                "Payment failed after {} retry attempts",
                self.config.max_retries
            )),
            cancel_at_end: false,
        };

        self.repository
            .cancel(subscription_id, cancel_request)
            .await?;

        // Send cancellation email
        if let Some(invoice) = latest_invoice {
            self.send_dunning_email(
                subscription_id,
                invoice.id,
                DunningEmailType::CancellationNotice,
            )
            .await?;
        }

        info!(
            "Subscription {} cancelled due to non-payment",
            subscription_id
        );

        Ok(PaymentRecoveryResult::FailedPermanent {
            cancelled_at: Utc::now(),
            reason: format!(
                "Payment failed after {} retry attempts",
                self.config.max_retries
            ),
        })
    }

    /// Send a dunning email
    ///
    /// Sends the appropriate email notification based on the email type
    pub async fn send_dunning_email(
        &self,
        subscription_id: Uuid,
        invoice_id: Uuid,
        email_type: DunningEmailType,
    ) -> Result<()> {
        // Check if email should be sent based on configuration
        let should_send = match email_type {
            DunningEmailType::FirstFailure => self.config.email_on_first_failure,
            DunningEmailType::FinalNotice => self.config.email_on_final_failure,
            _ => true,
        };

        if !should_send {
            info!(
                "Skipping {:?} email for subscription {} (disabled in config)",
                email_type, subscription_id
            );
            return Ok(());
        }

        // Get subscription and invoice for template data
        let subscription = self
            .repository
            .find_by_id(subscription_id)
            .await?
            .ok_or_else(|| Error::not_found("Subscription not found"))?;

        let invoice = self.repository.get_invoice(invoice_id).await?;

        // Build email content
        let (subject, body_text, body_html) = self
            .build_email_content(&subscription, invoice.as_ref(), email_type)
            .await?;

        // Record email in database
        let email = DunningEmail {
            id: Uuid::new_v4(),
            subscription_id,
            invoice_id,
            email_type,
            subject: subject.clone(),
            body_html: body_html.clone(),
            body_text: body_text.clone(),
            sent_at: Utc::now(),
            opened_at: None,
            clicked_at: None,
            created_at: Utc::now(),
        };

        self.repository.record_dunning_email(email).await?;

        // Send actual email if service is configured
        if let Some(ref _email_service) = self.email_service {
            // TODO: Get customer email from customer service
            // For now, we just log that the email would be sent
            info!(
                "Sending {:?} email for subscription {}: {}",
                email_type, subscription_id, subject
            );

            // In real implementation:
            // email_service.send_template(
            //     customer_email,
            //     subject,
            //     EmailTemplate::Dunning(email_type),
            //     template_data,
            // ).await?;
        } else {
            info!(
                "Would send {:?} email for subscription {}: {}",
                email_type, subscription_id, subject
            );
        }

        Ok(())
    }

    /// Process payment recovery
    ///
    /// Called when a retry payment succeeds. Updates subscription and invoice status,
    /// and sends recovery confirmation email.
    pub async fn process_recovery(
        &self,
        subscription_id: Uuid,
        invoice_id: Uuid,
        payment_id: String,
    ) -> Result<PaymentRecoveryResult> {
        info!(
            "Processing payment recovery for subscription {}: payment {}",
            subscription_id, payment_id
        );

        // Mark invoice as paid
        self.repository
            .mark_invoice_paid(invoice_id, payment_id.clone())
            .await?;

        // Update subscription status back to active
        let subscription = self
            .repository
            .find_by_id(subscription_id)
            .await?
            .ok_or_else(|| Error::not_found("Subscription not found"))?;

        if subscription.status == SubscriptionStatus::PastDue {
            self.repository
                .record_payment(subscription_id, payment_id)
                .await?;
        }

        // Send recovery email
        self.send_dunning_email(
            subscription_id,
            invoice_id,
            DunningEmailType::PaymentRecovered,
        )
        .await?;

        info!(
            "Payment recovery successful for subscription {}",
            subscription_id
        );

        Ok(PaymentRecoveryResult::Success)
    }

    // --- Query Operations ---

    /// Get pending retries
    ///
    /// Returns invoices that are due for retry processing
    pub async fn get_pending_retries(&self) -> Result<Vec<SubscriptionInvoice>> {
        let now = Utc::now();

        // Get all pending/billed invoices with failed attempts
        let invoices = self.repository.get_pending_invoices().await?;

        // Filter for those due for retry
        let pending: Vec<_> = invoices
            .into_iter()
            .filter(|i| {
                i.failed_attempts > 0
                    && i.failed_attempts < self.config.max_retries
                    && i.next_retry_at.is_some_and(|t| t <= now)
            })
            .collect();

        Ok(pending)
    }

    /// Get dunning history for a subscription
    ///
    /// Returns all retry attempts and emails for a subscription
    pub async fn get_dunning_history(&self, subscription_id: Uuid) -> Result<DunningHistory> {
        // Get subscription
        let subscription = self
            .repository
            .find_by_id(subscription_id)
            .await?
            .ok_or_else(|| Error::not_found("Subscription not found"))?;

        // Get all invoices
        let invoices = self.repository.list_invoices(subscription_id).await?;

        // Get retry attempts for all invoices
        let mut all_attempts = Vec::new();
        for invoice in &invoices {
            let attempts = self.repository.get_retry_attempts(invoice.id).await?;
            all_attempts.extend(attempts);
        }

        // Sort by attempt number and time
        all_attempts.sort_by_key(|a| (a.attempt_number, a.attempted_at));

        // Get dunning emails
        let emails = self.repository.get_dunning_emails(subscription_id).await?;

        let total_attempts = all_attempts.len() as i32;

        Ok(DunningHistory {
            subscription_id,
            subscription_status: subscription.status,
            retry_attempts: all_attempts,
            emails_sent: emails,
            total_attempts,
            is_cancelled: subscription.status == SubscriptionStatus::Cancelled
                && subscription.cancellation_reason == Some(CancellationReason::PaymentFailed),
        })
    }

    /// Get invoices requiring retry
    ///
    /// Returns all invoices that have failed and need retry processing
    pub async fn get_invoices_for_retry(&self) -> Result<Vec<RetryableInvoice>> {
        let now = Utc::now();
        let pending = self.repository.get_pending_invoices().await?;

        let mut retryable = Vec::new();

        for invoice in pending {
            if invoice.failed_attempts >= self.config.max_retries {
                continue; // Max retries reached
            }

            let is_due = invoice.next_retry_at.map_or(
                invoice.failed_attempts > 0, // If no next_retry set but has failures, it's due
                |t| t <= now,
            );

            if is_due {
                let subscription = self.repository.find_by_id(invoice.subscription_id).await?;
                if let Some(sub) = subscription {
                    if matches!(
                        sub.status,
                        SubscriptionStatus::Active | SubscriptionStatus::PastDue
                    ) {
                        retryable.push(RetryableInvoice {
                            invoice: invoice.clone(),
                            subscription: sub,
                            next_retry_at: invoice.next_retry_at,
                            attempt_number: invoice.failed_attempts + 1,
                        });
                    }
                }
            }
        }

        Ok(retryable)
    }

    /// Process all due retries
    ///
    /// This is the main method for the background job to process all pending retries
    pub async fn process_all_due_retries(&self) -> Result<RetryProcessingResult> {
        let due_invoices = self.get_invoices_for_retry().await?;

        let mut processed = 0;
        let mut succeeded = 0;
        let mut failed = 0;

        for retryable in due_invoices {
            processed += 1;

            match self.execute_retry(retryable.invoice.id).await {
                Ok(result) => {
                    match result {
                        PaymentRecoveryResult::Success => succeeded += 1,
                        PaymentRecoveryResult::RetryScheduled { .. } => {
                            // Retry scheduled, will be processed later
                        }
                        PaymentRecoveryResult::FailedPermanent { .. } => failed += 1,
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to process retry for invoice {}: {}",
                        retryable.invoice.id, e
                    );
                    failed += 1;
                }
            }
        }

        Ok(RetryProcessingResult {
            processed,
            succeeded,
            failed,
            pending: processed - succeeded - failed,
        })
    }

    // --- Email Template Builders ---

    /// Build email content based on type
    async fn build_email_content(
        &self,
        subscription: &Subscription,
        invoice: Option<&SubscriptionInvoice>,
        email_type: DunningEmailType,
    ) -> Result<(String, String, String)> {
        let amount = invoice.map(|i| i.total).unwrap_or(subscription.amount);
        let currency = subscription.currency;

        let (subject, body_text, body_html) = match email_type {
            DunningEmailType::FirstFailure => (
                "Payment Failed - Please Update Your Payment Method".to_string(),
                format!(
                    "We were unable to process your subscription payment.\n\n\
                    Amount: {} {}\n\
                    Subscription ID: {}\n\n\
                    We will retry your payment in {} days.\n\n\
                    Please update your payment method to avoid interruption.",
                    currency, amount, subscription.id,
                    self.config.retry_intervals_days.first().copied().unwrap_or(1)
                ),
                format!(
                    "<html><body>\
                    <h1>Payment Failed</h1>\
                    <p>We were unable to process your subscription payment.</p>\
                    <p><strong>Amount:</strong> {} {}</p>\
                    <p><strong>Subscription ID:</strong> {}</p>\
                    <p>We will retry your payment in {} days.</p>\
                    <p>Please <a href=\"#\">update your payment method</a> to avoid interruption.</p>\
                    </body></html>",
                    currency, amount, subscription.id,
                    self.config.retry_intervals_days.first().copied().unwrap_or(1)
                ),
            ),
            DunningEmailType::RetryFailure => (
                "Payment Failed Again - Action Required".to_string(),
                format!(
                    "Your subscription payment failed again.\n\n\
                    Amount: {} {}\n\
                    Subscription ID: {}\n\
                    Attempt: {}/{}\n\n\
                    We will retry again soon. Please update your payment method immediately.",
                    currency, amount, subscription.id,
                    invoice.map(|i| i.failed_attempts).unwrap_or(1),
                    self.config.max_retries
                ),
                format!(
                    "<html><body>\
                    <h1>Payment Failed Again</h1>\
                    <p>Your subscription payment failed again.</p>\
                    <p><strong>Amount:</strong> {} {}</p>\
                    <p><strong>Subscription ID:</strong> {}</p>\
                    <p><strong>Attempt:</strong> {}/{}</p>\
                    <p>Please <a href=\"#\">update your payment method</a> immediately.</p>\
                    </body></html>",
                    currency, amount, subscription.id,
                    invoice.map(|i| i.failed_attempts).unwrap_or(1),
                    self.config.max_retries
                ),
            ),
            DunningEmailType::FinalNotice => (
                "Final Notice: Subscription Cancellation Pending".to_string(),
                format!(
                    "FINAL NOTICE: Your subscription will be cancelled if payment is not received.\n\n\
                    Amount: {} {}\n\
                    Subscription ID: {}\n\n\
                    This is your final attempt. Please update your payment method within 24 hours \
                    to avoid cancellation."
                    , currency, amount, subscription.id
                ),
                format!(
                    "<html><body>\
                    <h1>Final Notice</h1>\
                    <p><strong>Your subscription will be cancelled if payment is not received.</strong></p>\
                    <p><strong>Amount:</strong> {} {}</p>\
                    <p><strong>Subscription ID:</strong> {}</p>\
                    <p>This is your final attempt. Please <a href=\"#\">update your payment method</a> \
                    within 24 hours to avoid cancellation.</p>\
                    </body></html>",
                    currency, amount, subscription.id
                ),
            ),
            DunningEmailType::CancellationNotice => (
                "Subscription Cancelled Due to Non-Payment".to_string(),
                format!(
                    "Your subscription has been cancelled due to non-payment.\n\n\
                    Subscription ID: {}\n\n\
                    To reactivate your subscription, please place a new order."
                    , subscription.id
                ),
                format!(
                    "<html><body>\
                    <h1>Subscription Cancelled</h1>\
                    <p>Your subscription has been cancelled due to non-payment.</p>\
                    <p><strong>Subscription ID:</strong> {}</p>\
                    <p>To reactivate your subscription, please <a href=\"#\">place a new order</a>.</p>\
                    </body></html>",
                    subscription.id
                ),
            ),
            DunningEmailType::PaymentRecovered => (
                "Payment Successful - Subscription Active".to_string(),
                format!(
                    "Great news! Your payment was successful.\n\n\
                    Amount: {} {}\n\
                    Subscription ID: {}\n\n\
                    Your subscription is now active. Thank you for your business!"
                    , currency, amount, subscription.id
                ),
                format!(
                    "<html><body>\
                    <h1>Payment Successful!</h1>\
                    <p>Great news! Your payment was successful.</p>\
                    <p><strong>Amount:</strong> {} {}</p>\
                    <p><strong>Subscription ID:</strong> {}</p>\
                    <p>Your subscription is now active. Thank you for your business!</p>\
                    </body></html>",
                    currency, amount, subscription.id
                ),
            ),
        };

        Ok((subject, body_text, body_html))
    }

    // --- Manual Operations ---

    /// Manually trigger a retry for an invoice
    ///
    /// This bypasses the scheduled retry time and attempts payment immediately
    pub async fn manual_retry(&self, invoice_id: Uuid) -> Result<PaymentRecoveryResult> {
        info!("Manual retry triggered for invoice {}", invoice_id);

        let invoice = self
            .repository
            .get_invoice(invoice_id)
            .await?
            .ok_or_else(|| Error::not_found("Invoice not found"))?;

        // Check if invoice is retryable
        if !matches!(
            invoice.status,
            InvoiceStatus::Billed | InvoiceStatus::Failed
        ) {
            return Err(Error::validation(format!(
                "Invoice {} is not in retryable status",
                invoice_id
            )));
        }

        // Check if max retries reached
        if invoice.failed_attempts >= self.config.max_retries {
            return Err(Error::validation(format!(
                "Invoice {} has reached maximum retry attempts",
                invoice_id
            )));
        }

        // Execute the retry
        self.execute_retry(invoice_id).await
    }

    /// Reset dunning state for a subscription
    ///
    /// This can be used when a customer updates their payment method
    pub async fn reset_dunning_state(&self, subscription_id: Uuid) -> Result<()> {
        info!(
            "Resetting dunning state for subscription {}",
            subscription_id
        );

        let subscription = self
            .repository
            .find_by_id(subscription_id)
            .await?
            .ok_or_else(|| Error::not_found("Subscription not found"))?;

        // Only reset if subscription is past_due
        if subscription.status == SubscriptionStatus::PastDue {
            // Update subscription status back to active
            let update = UpdateSubscriptionRequest {
                status: Some(SubscriptionStatus::Active),
                ..Default::default()
            };
            self.repository.update(subscription_id, update).await?;
        }

        Ok(())
    }
}

/// Dunning history for a subscription
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DunningHistory {
    pub subscription_id: Uuid,
    pub subscription_status: SubscriptionStatus,
    pub retry_attempts: Vec<PaymentRetryAttempt>,
    pub emails_sent: Vec<DunningEmail>,
    pub total_attempts: i32,
    pub is_cancelled: bool,
}

/// Retryable invoice with subscription info
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetryableInvoice {
    pub invoice: SubscriptionInvoice,
    pub subscription: Subscription,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub attempt_number: i32,
}

/// Result of processing retries
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetryProcessingResult {
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub pending: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateSubscriptionRequest, SubscriptionFilter, SubscriptionInterval};
    use crate::Currency;
    use rust_decimal::Decimal;

    // Mock repository for testing
    #[derive(Clone)]
    struct MockSubscriptionRepository;

    #[async_trait::async_trait]
    impl SubscriptionRepository for MockSubscriptionRepository {
        async fn create(&self, _request: CreateSubscriptionRequest) -> Result<Subscription> {
            unimplemented!()
        }

        async fn find_by_id(&self, id: Uuid) -> Result<Option<Subscription>> {
            Ok(Some(Subscription {
                id,
                customer_id: Uuid::new_v4(),
                order_id: Uuid::new_v4(),
                product_id: Uuid::new_v4(),
                variant_id: None,
                status: SubscriptionStatus::PastDue,
                interval: SubscriptionInterval::Monthly,
                interval_count: 1,
                currency: Currency::USD,
                amount: Decimal::new(1000, 2),
                setup_fee: None,
                trial_days: 0,
                trial_ends_at: None,
                current_cycle: 1,
                min_cycles: None,
                max_cycles: None,
                starts_at: Utc::now(),
                next_billing_at: Utc::now(),
                last_billing_at: None,
                ends_at: None,
                cancelled_at: None,
                cancellation_reason: None,
                payment_method_id: Some("pm_test".to_string()),
                gateway: "stripe".to_string(),
                notes: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }))
        }

        async fn list(
            &self,
            _filter: &SubscriptionFilter,
            _page: i64,
            _per_page: i64,
        ) -> Result<Vec<Subscription>> {
            Ok(vec![])
        }

        async fn count(&self, _filter: &SubscriptionFilter) -> Result<i64> {
            Ok(0)
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
            Ok(vec![])
        }

        async fn list_by_product(&self, _product_id: Uuid) -> Result<Vec<Subscription>> {
            Ok(vec![])
        }

        async fn get_due_for_billing(&self, _before: DateTime<Utc>) -> Result<Vec<Subscription>> {
            Ok(vec![])
        }

        async fn update_next_billing(
            &self,
            _id: Uuid,
            _next_billing_at: DateTime<Utc>,
        ) -> Result<()> {
            Ok(())
        }

        async fn increment_cycle(&self, _id: Uuid) -> Result<()> {
            Ok(())
        }

        async fn record_payment(&self, _id: Uuid, _payment_id: String) -> Result<()> {
            Ok(())
        }

        async fn record_failed_payment(&self, _id: Uuid, _failure_reason: String) -> Result<()> {
            Ok(())
        }

        async fn create_invoice(
            &self,
            invoice: SubscriptionInvoice,
        ) -> Result<SubscriptionInvoice> {
            Ok(invoice)
        }

        async fn get_invoice(&self, _invoice_id: Uuid) -> Result<Option<SubscriptionInvoice>> {
            Ok(Some(SubscriptionInvoice {
                id: Uuid::new_v4(),
                subscription_id: Uuid::new_v4(),
                order_id: None,
                cycle_number: 1,
                period_start: Utc::now(),
                period_end: Utc::now(),
                subtotal: Decimal::new(1000, 2),
                tax_total: Decimal::ZERO,
                total: Decimal::new(1000, 2),
                status: InvoiceStatus::Failed,
                paid_at: None,
                payment_id: None,
                failed_attempts: 1,
                last_failed_at: Some(Utc::now()),
                failure_reason: Some("Card declined".to_string()),
                next_retry_at: None,
                retry_count: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }))
        }

        async fn list_invoices(&self, _subscription_id: Uuid) -> Result<Vec<SubscriptionInvoice>> {
            Ok(vec![])
        }

        async fn mark_invoice_paid(&self, _invoice_id: Uuid, _payment_id: String) -> Result<()> {
            Ok(())
        }

        async fn mark_invoice_failed(
            &self,
            _invoice_id: Uuid,
            _failure_reason: String,
        ) -> Result<()> {
            Ok(())
        }

        async fn get_pending_invoices(&self) -> Result<Vec<SubscriptionInvoice>> {
            Ok(vec![])
        }

        async fn record_retry_attempt(&self, _attempt: PaymentRetryAttempt) -> Result<()> {
            Ok(())
        }

        async fn get_retry_attempts(&self, _invoice_id: Uuid) -> Result<Vec<PaymentRetryAttempt>> {
            Ok(vec![])
        }

        async fn record_dunning_email(&self, _email: DunningEmail) -> Result<()> {
            Ok(())
        }

        async fn get_dunning_emails(&self, _subscription_id: Uuid) -> Result<Vec<DunningEmail>> {
            Ok(vec![])
        }

        async fn get_status_counts(&self) -> Result<Vec<(SubscriptionStatus, i64)>> {
            Ok(vec![])
        }

        async fn calculate_mrr(&self) -> Result<Decimal> {
            Ok(Decimal::ZERO)
        }

        async fn calculate_arr(&self) -> Result<Decimal> {
            Ok(Decimal::ZERO)
        }
    }

    #[test]
    fn test_dunning_service_creation() {
        let repo = MockSubscriptionRepository;
        let service = DunningService::new(repo);

        assert_eq!(service.config().max_retries, 3);
        assert_eq!(service.config().retry_intervals_days, vec![1, 3, 7]);
    }

    #[test]
    fn test_dunning_service_with_config() {
        let repo = MockSubscriptionRepository;
        let config = DunningConfig {
            max_retries: 5,
            retry_intervals_days: vec![1, 2, 3, 5, 7],
            grace_period_days: 21,
            email_on_first_failure: true,
            email_on_final_failure: true,
            late_fee_after_retry: None,
            late_fee_amount: None,
        };

        let service = DunningService::with_config(repo, config.clone());

        assert_eq!(service.config().max_retries, 5);
        assert_eq!(service.config().retry_intervals_days, vec![1, 2, 3, 5, 7]);
    }

    #[tokio::test]
    async fn test_schedule_retry() {
        let repo = MockSubscriptionRepository;
        let service = DunningService::new(repo);

        let subscription_id = Uuid::new_v4();
        let invoice_id = Uuid::new_v4();

        let next_retry = service
            .schedule_retry(subscription_id, invoice_id, 1)
            .await
            .unwrap();

        let expected_days = 1; // First retry interval
        let diff = next_retry - Utc::now();
        assert!(diff.num_days() >= expected_days - 1 && diff.num_days() <= expected_days + 1);
    }
}
