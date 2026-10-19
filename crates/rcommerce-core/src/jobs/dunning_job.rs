//! Dunning Background Job
//!
//! Periodic job to process due payment retries and manage dunning workflows.
//! This job runs on a configurable interval and:
//! - Checks for invoices needing retry
//! - Executes scheduled retries
//! - Updates subscription statuses
//! - Sends notifications

use chrono::Utc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::Result;

use crate::config::DunningConfig;
use crate::payment::PaymentGateway;
use crate::repository::SubscriptionRepository;
use crate::services::DunningService;

/// Dunning job for background processing
pub struct DunningJob<R: SubscriptionRepository + Clone> {
    dunning_service: DunningService<R>,
    config: DunningConfig,
    job_id: Uuid,
    /// Payment gateway used to retry consent charges (None = reschedule only)
    gateway: Option<std::sync::Arc<dyn PaymentGateway>>,
    /// Order service used to confirm recovered renewal orders
    order_service: Option<std::sync::Arc<crate::order::OrderService>>,
}

impl<R: SubscriptionRepository + Clone> DunningJob<R> {
    /// Create a new dunning job
    pub fn new(dunning_service: DunningService<R>, config: DunningConfig) -> Self {
        Self {
            dunning_service,
            config,
            job_id: Uuid::new_v4(),
            gateway: None,
            order_service: None,
        }
    }

    /// Attach a payment gateway so retries actually charge the stored consent
    pub fn with_gateway(mut self, gateway: std::sync::Arc<dyn PaymentGateway>) -> Self {
        self.gateway = Some(gateway);
        self
    }

    /// Attach the order service so recovered renewals confirm their order
    pub fn with_order_service(
        mut self,
        order_service: std::sync::Arc<crate::order::OrderService>,
    ) -> Self {
        self.order_service = Some(order_service);
        self
    }

    /// Get the job ID
    pub fn job_id(&self) -> Uuid {
        self.job_id
    }

    /// Run the dunning job once
    ///
    /// This method processes all due retries and returns a summary of the results.
    pub async fn run(&self) -> Result<DunningJobResult> {
        if !self.config.enabled {
            info!("Dunning job {} skipped: dunning is disabled", self.job_id);
            return Ok(DunningJobResult::skipped());
        }

        info!("Starting dunning job {}", self.job_id);
        let start_time = Utc::now();

        // Process all due retries. Without a configured gateway the job only
        // reschedules (legacy behaviour).
        let result = match self.gateway.as_ref() {
            Some(gateway) => {
                self.dunning_service
                    .process_all_due_retries(gateway.as_ref(), self.order_service.as_ref())
                    .await?
            }
            None => {
                warn!(
                    "Dunning job {} has no payment gateway; retries only rescheduled",
                    self.job_id
                );
                crate::services::RetryProcessingResult {
                    processed: 0,
                    succeeded: 0,
                    failed: 0,
                    pending: 0,
                }
            }
        };

        // Check for subscriptions that should be cancelled (grace period expired)
        let cancelled = self.process_expired_grace_periods().await?;

        let duration = Utc::now() - start_time;

        info!(
            "Dunning job {} completed in {}ms: processed={}, succeeded={}, failed={}, cancelled={}",
            self.job_id,
            duration.num_milliseconds(),
            result.processed,
            result.succeeded,
            result.failed,
            cancelled
        );

        Ok(DunningJobResult {
            job_id: self.job_id,
            processed: result.processed,
            succeeded: result.succeeded,
            failed: result.failed,
            cancelled,
            duration_ms: duration.num_milliseconds() as u64,
            ..Default::default()
        })
    }

    /// Process subscriptions whose grace period has expired
    ///
    /// Finds subscriptions that have been in past_due status beyond the grace period
    /// and cancels them if they haven't already been cancelled.
    async fn process_expired_grace_periods(&self) -> Result<usize> {
        // Get all past_due subscriptions
        let past_due = self.dunning_service.get_invoices_for_retry().await?;

        let mut cancelled = 0;
        let grace_period = chrono::Duration::days(self.config.grace_period_days as i64);

        for retryable in past_due {
            // Check if the subscription has been past_due beyond grace period
            if let Some(last_failed) = retryable.invoice.last_failed_at {
                let elapsed = Utc::now() - last_failed;

                if elapsed > grace_period
                    && retryable.invoice.failed_attempts >= self.config.max_retries
                {
                    warn!(
                        "Subscription {} grace period expired ({} days). Cancelling.",
                        retryable.subscription.id,
                        elapsed.num_days()
                    );

                    match self
                        .dunning_service
                        .cancel_after_retries(retryable.subscription.id)
                        .await
                    {
                        Ok(_) => {
                            cancelled += 1;
                        }
                        Err(e) => {
                            error!(
                                "Failed to cancel subscription {}: {}",
                                retryable.subscription.id, e
                            );
                        }
                    }
                }
            }
        }

        if cancelled > 0 {
            info!(
                "Cancelled {} subscriptions due to expired grace period",
                cancelled
            );
        }

        Ok(cancelled)
    }

    /// Get job statistics
    ///
    /// Returns current statistics about pending retries and dunning state
    pub async fn get_stats(&self) -> Result<DunningJobStats> {
        let pending_retries = self.dunning_service.get_pending_retries().await?;
        let invoices_for_retry = self.dunning_service.get_invoices_for_retry().await?;

        Ok(DunningJobStats {
            pending_retries: pending_retries.len(),
            invoices_due: invoices_for_retry.len(),
            next_run_in_minutes: self.config.job_interval_minutes,
        })
    }
}

/// Result of a dunning job run
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DunningJobResult {
    pub job_id: Uuid,
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub skipped: bool,
    pub duration_ms: u64,
    pub errors: Vec<String>,
}

impl DunningJobResult {
    /// Create a result indicating the job was skipped
    fn skipped() -> Self {
        Self {
            job_id: Uuid::nil(),
            processed: 0,
            succeeded: 0,
            failed: 0,
            cancelled: 0,
            skipped: true,
            duration_ms: 0,
            errors: vec![],
        }
    }
}

impl Default for DunningJobResult {
    fn default() -> Self {
        Self {
            job_id: Uuid::nil(),
            processed: 0,
            succeeded: 0,
            failed: 0,
            cancelled: 0,
            skipped: false,
            duration_ms: 0,
            errors: vec![],
        }
    }
}

/// Dunning job statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DunningJobStats {
    pub pending_retries: usize,
    pub invoices_due: usize,
    pub next_run_in_minutes: i32,
}

/// Dunning job scheduler
///
/// Manages the periodic execution of dunning jobs
pub struct DunningJobScheduler<R: SubscriptionRepository + Clone> {
    dunning_service: DunningService<R>,
    config: DunningConfig,
    running: bool,
}

impl<R: SubscriptionRepository + Clone> DunningJobScheduler<R> {
    /// Create a new dunning job scheduler
    pub fn new(dunning_service: DunningService<R>, config: DunningConfig) -> Self {
        Self {
            dunning_service,
            config,
            running: false,
        }
    }

    /// Start the scheduler
    ///
    /// This will spawn a background task that runs the dunning job
    /// at the configured interval.
    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            info!("Dunning job scheduler not started: dunning is disabled");
            return Ok(());
        }

        if self.running {
            return Err(Error::validation(
                "Dunning job scheduler is already running",
            ));
        }

        self.running = true;
        info!(
            "Dunning job scheduler started with {} minute interval",
            self.config.job_interval_minutes
        );

        // In a real implementation, this would spawn a background task
        // that runs the job at the configured interval
        // For now, we just mark it as running

        Ok(())
    }

    /// Stop the scheduler
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }

        self.running = false;
        info!("Dunning job scheduler stopped");

        Ok(())
    }

    /// Check if the scheduler is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Run the job immediately (manual trigger)
    pub async fn run_now(&self) -> Result<DunningJobResult> {
        let job = DunningJob::new(self.dunning_service.clone(), self.config.clone());
        job.run().await
    }
}

use crate::Error;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CancelSubscriptionRequest, CreateSubscriptionRequest, DunningEmail, PaymentRetryAttempt,
        SubscriptionFilter, UpdateSubscriptionRequest,
    };
    use crate::models::{Subscription, SubscriptionInvoice, SubscriptionStatus};
    use crate::repository::SubscriptionRepository;
    use async_trait::async_trait;
    use rust_decimal::Decimal;

    #[derive(Clone)]
    struct MockSubscriptionRepository;

    #[async_trait]
    impl SubscriptionRepository for MockSubscriptionRepository {
        async fn create(&self, _request: CreateSubscriptionRequest) -> Result<Subscription> {
            unimplemented!()
        }

        async fn find_by_id(&self, _id: Uuid) -> Result<Option<Subscription>> {
            Ok(None)
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

        async fn get_due_for_billing(
            &self,
            _before: chrono::DateTime<Utc>,
        ) -> Result<Vec<Subscription>> {
            Ok(vec![])
        }

        async fn update_next_billing(
            &self,
            _id: Uuid,
            _next_billing_at: chrono::DateTime<Utc>,
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
            Ok(None)
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
    fn test_dunning_job_result_default() {
        let result = DunningJobResult::default();
        assert_eq!(result.processed, 0);
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed, 0);
        assert!(!result.skipped);
    }

    #[test]
    fn test_dunning_job_result_skipped() {
        let result = DunningJobResult::skipped();
        assert!(result.skipped);
        assert_eq!(result.processed, 0);
    }

    #[tokio::test]
    async fn test_dunning_job_disabled() {
        let repo = MockSubscriptionRepository;
        let dunning_service = DunningService::new(repo);
        let config = DunningConfig {
            enabled: false,
            ..Default::default()
        };

        let job = DunningJob::new(dunning_service, config);
        let result = job.run().await.unwrap();

        assert!(result.skipped);
    }

    #[test]
    fn test_dunning_job_stats() {
        let stats = DunningJobStats {
            pending_retries: 5,
            invoices_due: 3,
            next_run_in_minutes: 60,
        };

        assert_eq!(stats.pending_retries, 5);
        assert_eq!(stats.invoices_due, 3);
        assert_eq!(stats.next_run_in_minutes, 60);
    }
}
