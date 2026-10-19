//! Subscription Repository
//!
//! Database operations for subscription management including:
//! - Creating and managing subscriptions
//! - Billing cycle tracking
//! - Invoice generation
//! - Payment retry handling

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::{
    CancelSubscriptionRequest, CreateSubscriptionRequest, DunningEmail, PaymentRetryAttempt,
    Subscription, SubscriptionFilter, SubscriptionInvoice, SubscriptionStatus,
    UpdateSubscriptionRequest,
};
use crate::{Error, Result};

/// Subscription repository trait for database operations
#[async_trait]
pub trait SubscriptionRepository: Send + Sync + 'static {
    /// Create a new subscription
    async fn create(&self, request: CreateSubscriptionRequest) -> Result<Subscription>;

    /// Find subscription by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Subscription>>;

    /// List subscriptions with filtering
    async fn list(
        &self,
        filter: &SubscriptionFilter,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<Subscription>>;

    /// Count subscriptions by filter
    async fn count(&self, filter: &SubscriptionFilter) -> Result<i64>;

    /// Update subscription
    async fn update(&self, id: Uuid, request: UpdateSubscriptionRequest) -> Result<Subscription>;

    /// Cancel subscription
    async fn cancel(&self, id: Uuid, request: CancelSubscriptionRequest) -> Result<Subscription>;

    /// Pause subscription
    async fn pause(&self, id: Uuid) -> Result<Subscription>;

    /// Resume subscription
    async fn resume(&self, id: Uuid) -> Result<Subscription>;

    /// Get subscriptions by customer
    async fn list_by_customer(
        &self,
        customer_id: Uuid,
        status: Option<SubscriptionStatus>,
    ) -> Result<Vec<Subscription>>;

    /// Get subscriptions by product
    async fn list_by_product(&self, product_id: Uuid) -> Result<Vec<Subscription>>;

    /// Get subscriptions due for billing
    async fn get_due_for_billing(&self, before: DateTime<Utc>) -> Result<Vec<Subscription>>;

    /// Update next billing date
    async fn update_next_billing(&self, id: Uuid, next_billing_at: DateTime<Utc>) -> Result<()>;

    /// Increment billing cycle
    async fn increment_cycle(&self, id: Uuid) -> Result<()>;

    /// Record successful payment
    async fn record_payment(&self, id: Uuid, payment_id: String) -> Result<()>;

    /// Record failed payment
    async fn record_failed_payment(&self, id: Uuid, failure_reason: String) -> Result<()>;

    // Invoice operations

    /// Create subscription invoice
    async fn create_invoice(&self, invoice: SubscriptionInvoice) -> Result<SubscriptionInvoice>;

    /// Get invoice by ID
    async fn get_invoice(&self, invoice_id: Uuid) -> Result<Option<SubscriptionInvoice>>;

    /// List invoices for subscription
    async fn list_invoices(&self, subscription_id: Uuid) -> Result<Vec<SubscriptionInvoice>>;

    /// Mark invoice as paid
    async fn mark_invoice_paid(&self, invoice_id: Uuid, payment_id: String) -> Result<()>;

    /// Mark invoice as failed
    async fn mark_invoice_failed(&self, invoice_id: Uuid, failure_reason: String) -> Result<()>;

    /// Get pending invoices
    async fn get_pending_invoices(&self) -> Result<Vec<SubscriptionInvoice>>;

    // Dunning/payment retry operations

    /// Record payment retry attempt
    async fn record_retry_attempt(&self, attempt: PaymentRetryAttempt) -> Result<()>;

    /// Get retry attempts for invoice
    async fn get_retry_attempts(&self, invoice_id: Uuid) -> Result<Vec<PaymentRetryAttempt>>;

    /// Record dunning email sent
    async fn record_dunning_email(&self, email: DunningEmail) -> Result<()>;

    /// Get dunning emails for subscription
    async fn get_dunning_emails(&self, subscription_id: Uuid) -> Result<Vec<DunningEmail>>;

    // Statistics

    /// Get subscription counts by status
    async fn get_status_counts(&self) -> Result<Vec<(SubscriptionStatus, i64)>>;

    /// Calculate Monthly Recurring Revenue (MRR)
    async fn calculate_mrr(&self) -> Result<rust_decimal::Decimal>;

    /// Calculate Annual Recurring Revenue (ARR)
    async fn calculate_arr(&self) -> Result<rust_decimal::Decimal>;

    /// Schedule the next retry attempt for a failed invoice (dunning).
    ///
    /// Default: not supported (in-memory repositories).
    async fn schedule_invoice_retry(
        &self,
        _invoice_id: Uuid,
        _next_retry_at: DateTime<Utc>,
    ) -> Result<()> {
        Err(Error::validation(
            "schedule_invoice_retry not supported by this repository",
        ))
    }

    /// Look up a customer's email address (needed to charge renewals).
    ///
    /// Default: not supported (in-memory repositories).
    async fn customer_email(&self, _customer_id: Uuid) -> Result<Option<String>> {
        Ok(None)
    }
}

/// PostgreSQL implementation of subscription repository
#[derive(Clone)]
pub struct PostgresSubscriptionRepository {
    pool: sqlx::Pool<sqlx::Postgres>,
}

impl PostgresSubscriptionRepository {
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }

    /// Direct pool access for the Postgres-specialized billing routines
    /// (renewal order annotation, charge persistence).
    pub fn pool(&self) -> &sqlx::Pool<sqlx::Postgres> {
        &self.pool
    }
}

#[async_trait]
impl SubscriptionRepository for PostgresSubscriptionRepository {
    async fn create(&self, request: CreateSubscriptionRequest) -> Result<Subscription> {
        let now = Utc::now();
        let trial_days = request.trial_days.unwrap_or(0);
        let trial_ends_at = if trial_days > 0 {
            Some(now + chrono::Duration::days(trial_days as i64))
        } else {
            None
        };

        let status = if trial_days > 0 {
            SubscriptionStatus::Trialing
        } else {
            SubscriptionStatus::Active
        };

        let subscription = sqlx::query_as::<_, Subscription>(
            r#"
            INSERT INTO subscriptions (
                customer_id, order_id, product_id, variant_id,
                status, interval, interval_count,
                currency, amount, setup_fee,
                trial_days, trial_ends_at,
                current_cycle, min_cycles, max_cycles,
                starts_at, next_billing_at,
                payment_method_id, gateway, notes,
                quantity, shipping_address,
                created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $23)
            RETURNING *
            "#
        )
        .bind(request.customer_id)
        .bind(request.order_id)
        .bind(request.product_id)
        .bind(request.variant_id)
        .bind(status)
        .bind(request.interval)
        .bind(request.interval_count)
        .bind(request.currency)
        .bind(request.amount)
        .bind(request.setup_fee)
        .bind(trial_days)
        .bind(trial_ends_at)
        .bind(0) // current_cycle
        .bind(request.min_cycles)
        .bind(request.max_cycles)
        .bind(now) // starts_at
        .bind(trial_ends_at.unwrap_or(now)) // next_billing_at
        .bind(request.payment_method_id)
        .bind(request.gateway)
        .bind(request.notes)
        .bind(request.quantity.unwrap_or(1))
        .bind(request.shipping_address)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(subscription)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Subscription>> {
        let subscription =
            sqlx::query_as::<_, Subscription>("SELECT * FROM subscriptions WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?;

        Ok(subscription)
    }

    async fn list(
        &self,
        filter: &SubscriptionFilter,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<Subscription>> {
        let offset = (page - 1) * per_page;

        let mut query = String::from("SELECT * FROM subscriptions WHERE 1=1");

        if let Some(customer_id) = filter.customer_id {
            query.push_str(&format!(" AND customer_id = '{}'", customer_id));
        }
        if let Some(product_id) = filter.product_id {
            query.push_str(&format!(" AND product_id = '{}'", product_id));
        }
        if let Some(status) = filter.status {
            query.push_str(&format!(" AND status = '{:?}'", status));
        }
        if let Some(gateway) = &filter.gateway {
            query.push_str(&format!(" AND gateway = '{}'", gateway));
        }
        if let Some(before) = filter.billing_before {
            query.push_str(&format!(" AND next_billing_at <= '{}'", before));
        }
        if let Some(after) = filter.billing_after {
            query.push_str(&format!(" AND next_billing_at >= '{}'", after));
        }

        query.push_str(&format!(
            " ORDER BY created_at DESC LIMIT {} OFFSET {}",
            per_page, offset
        ));

        let subscriptions = sqlx::query_as::<_, Subscription>(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(subscriptions)
    }

    async fn count(&self, filter: &SubscriptionFilter) -> Result<i64> {
        let mut query = String::from("SELECT COUNT(*) FROM subscriptions WHERE 1=1");

        if let Some(customer_id) = filter.customer_id {
            query.push_str(&format!(" AND customer_id = '{}'", customer_id));
        }
        if let Some(status) = filter.status {
            query.push_str(&format!(" AND status = '{:?}'", status));
        }

        let count: i64 = sqlx::query_scalar(&query)
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(count)
    }

    async fn update(&self, id: Uuid, request: UpdateSubscriptionRequest) -> Result<Subscription> {
        let subscription = sqlx::query_as::<_, Subscription>(
            r#"
            UPDATE subscriptions 
            SET 
                status = COALESCE($1, status),
                interval = COALESCE($2, interval),
                interval_count = COALESCE($3, interval_count),
                amount = COALESCE($4, amount),
                next_billing_at = COALESCE($5, next_billing_at),
                max_cycles = COALESCE($6, max_cycles),
                payment_method_id = COALESCE($7, payment_method_id),
                notes = COALESCE($8, notes),
                updated_at = NOW()
            WHERE id = $9
            RETURNING *
            "#,
        )
        .bind(request.status)
        .bind(request.interval)
        .bind(request.interval_count)
        .bind(request.amount)
        .bind(request.next_billing_at)
        .bind(request.max_cycles)
        .bind(request.payment_method_id)
        .bind(request.notes)
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(subscription)
    }

    async fn cancel(&self, id: Uuid, request: CancelSubscriptionRequest) -> Result<Subscription> {
        let now = Utc::now();

        let subscription = sqlx::query_as::<_, Subscription>(
            r#"
            UPDATE subscriptions 
            SET 
                status = 'cancelled',
                cancelled_at = $1,
                cancellation_reason = $2,
                ends_at = CASE WHEN $3 THEN next_billing_at ELSE $1 END,
                updated_at = NOW()
            WHERE id = $4
            RETURNING *
            "#,
        )
        .bind(now)
        .bind(request.reason)
        .bind(request.cancel_at_end)
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(subscription)
    }

    async fn pause(&self, id: Uuid) -> Result<Subscription> {
        let subscription = sqlx::query_as::<_, Subscription>(
            r#"
            UPDATE subscriptions 
            SET status = 'paused', updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(subscription)
    }

    async fn resume(&self, id: Uuid) -> Result<Subscription> {
        let subscription = sqlx::query_as::<_, Subscription>(
            r#"
            UPDATE subscriptions 
            SET status = 'active', updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(subscription)
    }

    async fn list_by_customer(
        &self,
        customer_id: Uuid,
        status: Option<SubscriptionStatus>,
    ) -> Result<Vec<Subscription>> {
        let mut query = String::from("SELECT * FROM subscriptions WHERE customer_id = $1");

        if let Some(s) = status {
            query.push_str(&format!(" AND status = '{:?}'", s));
        }

        query.push_str(" ORDER BY created_at DESC");

        let subscriptions = sqlx::query_as::<_, Subscription>(&query)
            .bind(customer_id)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(subscriptions)
    }

    async fn list_by_product(&self, product_id: Uuid) -> Result<Vec<Subscription>> {
        let subscriptions = sqlx::query_as::<_, Subscription>(
            "SELECT * FROM subscriptions WHERE product_id = $1 ORDER BY created_at DESC",
        )
        .bind(product_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(subscriptions)
    }

    async fn get_due_for_billing(&self, before: DateTime<Utc>) -> Result<Vec<Subscription>> {
        let subscriptions = sqlx::query_as::<_, Subscription>(
            r#"
            SELECT * FROM subscriptions 
            WHERE next_billing_at <= $1 
            AND status IN ('active', 'trialing')
            ORDER BY next_billing_at ASC
            "#,
        )
        .bind(before)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(subscriptions)
    }

    async fn update_next_billing(&self, id: Uuid, next_billing_at: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "UPDATE subscriptions SET next_billing_at = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(next_billing_at)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn increment_cycle(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE subscriptions 
            SET current_cycle = current_cycle + 1, 
                last_billing_at = NOW(),
                updated_at = NOW() 
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn record_payment(&self, id: Uuid, _payment_id: String) -> Result<()> {
        sqlx::query("UPDATE subscriptions SET status = 'active', updated_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn record_failed_payment(&self, id: Uuid, _failure_reason: String) -> Result<()> {
        sqlx::query(
            "UPDATE subscriptions SET status = 'past_due', updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    // Invoice operations

    async fn create_invoice(&self, invoice: SubscriptionInvoice) -> Result<SubscriptionInvoice> {
        let invoice = sqlx::query_as::<_, SubscriptionInvoice>(
            r#"
            INSERT INTO subscription_invoices (
                subscription_id, order_id, cycle_number, period_start, period_end,
                subtotal, tax_total, total, status, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(invoice.subscription_id)
        .bind(invoice.order_id)
        .bind(invoice.cycle_number)
        .bind(invoice.period_start)
        .bind(invoice.period_end)
        .bind(invoice.subtotal)
        .bind(invoice.tax_total)
        .bind(invoice.total)
        .bind(invoice.status)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(invoice)
    }

    async fn get_invoice(&self, invoice_id: Uuid) -> Result<Option<SubscriptionInvoice>> {
        let invoice = sqlx::query_as::<_, SubscriptionInvoice>(
            "SELECT * FROM subscription_invoices WHERE id = $1",
        )
        .bind(invoice_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(invoice)
    }

    async fn list_invoices(&self, subscription_id: Uuid) -> Result<Vec<SubscriptionInvoice>> {
        let invoices = sqlx::query_as::<_, SubscriptionInvoice>(
            "SELECT * FROM subscription_invoices WHERE subscription_id = $1 ORDER BY cycle_number DESC"
        )
        .bind(subscription_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(invoices)
    }

    async fn mark_invoice_paid(&self, invoice_id: Uuid, payment_id: String) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE subscription_invoices 
            SET status = 'paid', paid_at = NOW(), payment_id = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(payment_id)
        .bind(invoice_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn mark_invoice_failed(&self, invoice_id: Uuid, failure_reason: String) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE subscription_invoices 
            SET status = 'failed', failure_reason = $1, last_failed_at = NOW(), 
                failed_attempts = failed_attempts + 1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(failure_reason)
        .bind(invoice_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn get_pending_invoices(&self) -> Result<Vec<SubscriptionInvoice>> {
        let invoices = sqlx::query_as::<_, SubscriptionInvoice>(
            "SELECT * FROM subscription_invoices WHERE status IN ('pending', 'billed') ORDER BY created_at ASC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(invoices)
    }

    // Dunning operations

    async fn record_retry_attempt(&self, attempt: PaymentRetryAttempt) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO payment_retry_attempts (
                id, subscription_id, invoice_id, attempt_number, attempted_at,
                succeeded, error_message, error_code, next_retry_at, payment_method_id,
                gateway_transaction_id, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(attempt.id)
        .bind(attempt.subscription_id)
        .bind(attempt.invoice_id)
        .bind(attempt.attempt_number)
        .bind(attempt.attempted_at)
        .bind(attempt.succeeded)
        .bind(attempt.error_message)
        .bind(attempt.error_code)
        .bind(attempt.next_retry_at)
        .bind(attempt.payment_method_id)
        .bind(attempt.gateway_transaction_id)
        .bind(attempt.created_at)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn get_retry_attempts(&self, invoice_id: Uuid) -> Result<Vec<PaymentRetryAttempt>> {
        let attempts = sqlx::query_as::<_, PaymentRetryAttempt>(
            "SELECT * FROM payment_retry_attempts WHERE invoice_id = $1 ORDER BY attempt_number ASC"
        )
        .bind(invoice_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(attempts)
    }

    async fn record_dunning_email(&self, email: DunningEmail) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO dunning_emails (
                id, subscription_id, invoice_id, email_type, subject, body_html, body_text,
                sent_at, opened_at, clicked_at, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(email.id)
        .bind(email.subscription_id)
        .bind(email.invoice_id)
        .bind(email.email_type)
        .bind(email.subject)
        .bind(email.body_html)
        .bind(email.body_text)
        .bind(email.sent_at)
        .bind(email.opened_at)
        .bind(email.clicked_at)
        .bind(email.created_at)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn get_dunning_emails(&self, subscription_id: Uuid) -> Result<Vec<DunningEmail>> {
        let emails = sqlx::query_as::<_, DunningEmail>(
            "SELECT * FROM dunning_emails WHERE subscription_id = $1 ORDER BY sent_at DESC",
        )
        .bind(subscription_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(emails)
    }

    // Statistics

    async fn get_status_counts(&self) -> Result<Vec<(SubscriptionStatus, i64)>> {
        let results = sqlx::query_as::<_, (SubscriptionStatus, i64)>(
            "SELECT status, COUNT(*) FROM subscriptions GROUP BY status",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(results)
    }

    async fn calculate_mrr(&self) -> Result<rust_decimal::Decimal> {
        let mrr: Option<rust_decimal::Decimal> = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(
                CASE interval
                    WHEN 'daily' THEN amount * 30
                    WHEN 'weekly' THEN amount * 4
                    WHEN 'bi_weekly' THEN amount * 2
                    WHEN 'monthly' THEN amount
                    WHEN 'quarterly' THEN amount / 3
                    WHEN 'bi_annually' THEN amount / 6
                    WHEN 'annually' THEN amount / 12
                    ELSE amount
                END
            ), 0)
            FROM subscriptions 
            WHERE status = 'active'
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(mrr.unwrap_or_default())
    }

    async fn calculate_arr(&self) -> Result<rust_decimal::Decimal> {
        let mrr = self.calculate_mrr().await?;
        Ok(mrr * rust_decimal::Decimal::from(12))
    }

    async fn schedule_invoice_retry(
        &self,
        invoice_id: Uuid,
        next_retry_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE subscription_invoices SET next_retry_at = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(next_retry_at)
        .bind(invoice_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn customer_email(&self, customer_id: Uuid) -> Result<Option<String>> {
        let email: Option<String> = sqlx::query_scalar("SELECT email FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(email)
    }
}
