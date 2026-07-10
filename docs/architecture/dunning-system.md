# Dunning System Architecture

## Overview

The **Dunning System** is R Commerce's automated payment retry and failed payment recovery mechanism for subscription billing. Named after the historical term for debt collection, dunning handles the delicate process of recovering failed subscription payments while maintaining positive customer relationships.

### Why Dunning Matters

Failed payments are inevitable in subscription businesses:

- **Expired credit cards** - Most common cause (30-40% of failures)
- **Insufficient funds** - Temporary issues (25-30% of failures)
- **Bank declines** - Fraud prevention, limits reached (20-25% of failures)
- **Technical issues** - Gateway timeouts, network errors (10-15% of failures)

Without an effective dunning system:
- Revenue leakage from involuntary churn
- Poor customer experience from abrupt cancellations
- Increased support burden
- Lost lifetime value

### Key Objectives

1. **Maximize Recovery Rate** - Recover 60-80% of failed payments
2. **Minimize Involuntary Churn** - Keep customers who want to stay
3. **Preserve Customer Experience** - Professional, helpful communications
4. **Automate Everything** - No manual intervention for routine retries
5. **Maintain Compliance** - Follow card network rules and regulations

## Dunning Workflow & Lifecycle

### State Machine

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         DUNNING LIFECYCLE                               │
└─────────────────────────────────────────────────────────────────────────┘

  ┌──────────────┐
  │   Payment    │
  │   Billing    │
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐     ┌──────────────┐
  │   Payment    │────▶│   Success    │
  │   Failed     │     │   (End)      │
  └──────┬───────┘     └──────────────┘
         │
         ▼
  ┌─────────────────────────────────────────┐
  │  1. Record failed attempt               │
  │  2. Update subscription to PastDue      │
  │  3. Send First Failure email            │
  │  4. Schedule retry (Day +1)             │
  └─────────────────────────────────────────┘
         │
         ▼
  ┌──────────────┐     ┌──────────────┐
  │   Retry 1    │────▶│   Success    │
  │   (Day +1)   │     │   (End)      │
  └──────┬───────┘     └──────────────┘
         │ Failure
         ▼
  ┌─────────────────────────────────────────┐
  │  1. Record failed attempt               │
  │  2. Send Retry Failure email            │
  │  3. Schedule retry (Day +3)             │
  └─────────────────────────────────────────┘
         │
         ▼
  ┌──────────────┐     ┌──────────────┐
  │   Retry 2    │────▶│   Success    │
  │   (Day +3)   │     │   (End)      │
  └──────┬───────┘     └──────────────┘
         │ Failure
         ▼
  ┌─────────────────────────────────────────┐
  │  1. Record failed attempt               │
  │  2. Send Final Notice email             │
  │  3. Schedule retry (Day +7)             │
  └─────────────────────────────────────────┘
         │
         ▼
  ┌──────────────┐     ┌──────────────┐
  │   Retry 3    │────▶│   Success    │
  │   (Day +7)   │     │   (End)      │
  └──────┬───────┘     └──────────────┘
         │ Failure
         ▼
  ┌─────────────────────────────────────────┐
  │  1. Cancel subscription                 │
  │  2. Send Cancellation Notice            │
  │  3. Update invoice to PastDue           │
  └─────────────────────────────────────────┘
```

### Subscription Status Flow

```
Active ────────────────────────────────────────────────────────────────
  │                                                                      │
  │ Payment Failed                                                       │ Payment Recovered
  ▼                                                                      │
PastDue ────────────────────────────────────────────────────────────────
  │                                                                      │
  │ Retries Exhausted                                                    │
  ▼                                                                      │
Cancelled (reason: PaymentFailed)                                       │
```

## Core Components

### DunningConfig

Configuration structure for dunning behavior:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DunningConfig {
    /// Number of retry attempts before cancellation (default: 3)
    pub max_retries: i32,
    
    /// Retry intervals in days (default: [1, 3, 7])
    /// Retry 1: 1 day after initial failure
    /// Retry 2: 3 days after retry 1
    /// Retry 3: 7 days after retry 2
    pub retry_intervals_days: Vec<i32>,
    
    /// Grace period in days (default: 14)
    /// Subscription remains active during dunning
    pub grace_period_days: i32,
    
    /// Send email on first failure (default: true)
    pub email_on_first_failure: bool,
    
    /// Send email on final failure before cancellation (default: true)
    pub email_on_final_failure: bool,
    
    /// Apply late fees after N retries (None = no late fees)
    pub late_fee_after_retry: Option<i32>,
    
    /// Late fee amount (if late_fee_after_retry is set)
    pub late_fee_amount: Option<Decimal>,
}

impl Default for DunningConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_intervals_days: vec![1, 3, 7],
            grace_period_days: 14,
            email_on_first_failure: true,
            email_on_final_failure: true,
            late_fee_after_retry: None,
            late_fee_amount: None,
        }
    }
}
```

### DunningManager

The core dunning workflow handler:

```rust
pub struct DunningManager<G: PaymentGateway> {
    gateway: G,
    config: DunningConfig,
}

impl<G: PaymentGateway> DunningManager<G> {
    /// Main entry point for handling failed payments
    pub async fn process_failed_payment(
        &self,
        subscription: &mut Subscription,
        invoice: &mut SubscriptionInvoice,
        error_message: &str,
        error_code: Option<&str>,
    ) -> Result<PaymentRecoveryResult>;

    /// Attempt to retry a failed payment (called by scheduled job)
    pub async fn retry_payment(
        &self,
        subscription: &mut Subscription,
        invoice: &mut SubscriptionInvoice,
    ) -> Result<PaymentRecoveryResult>;

    /// Calculate next retry date based on attempt number
    fn calculate_next_retry_date(&self, attempt_number: i32) -> DateTime<Utc>;

    /// Cancel subscription when all retries exhausted
    async fn cancel_subscription_for_non_payment(
        &self,
        subscription: &mut Subscription,
        invoice: &mut SubscriptionInvoice,
    ) -> Result<()>;
}
```

### PaymentRetryAttempt

Tracks each retry attempt for audit and analytics:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PaymentRetryAttempt {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub invoice_id: Uuid,
    pub attempt_number: i32,
    pub attempted_at: DateTime<Utc>,
    pub succeeded: bool,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub payment_method_id: Option<String>,
    pub gateway_transaction_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

### DunningEmail

Tracks customer communications:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DunningEmail {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub invoice_id: Uuid,
    pub email_type: DunningEmailType,
    pub subject: String,
    pub body_html: String,
    pub body_text: String,
    pub sent_at: DateTime<Utc>,
    pub opened_at: Option<DateTime<Utc>>,
    pub clicked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "dunning_email_type", rename_all = "snake_case")]
pub enum DunningEmailType {
    FirstFailure,       // Initial failure notification
    RetryFailure,       // Subsequent retry failure
    FinalNotice,        // Last warning before cancellation
    CancellationNotice, // Subscription cancelled
    PaymentRecovered,   // Success confirmation
}
```

## Configuration Options

### Retry Schedule

The default retry schedule follows industry best practices:

| Attempt | Days After Previous | Cumulative Days | Email Sent |
|---------|--------------------|-----------------|------------|
| Initial | - | 0 | First Failure |
| Retry 1 | 1 day | 1 day | - |
| Retry 2 | 3 days | 4 days | Retry Failure |
| Retry 3 | 7 days | 11 days | Final Notice |
| Cancel | - | 11+ days | Cancellation |

**Why this schedule works:**
- **1 day**: Quick first retry catches temporary issues (insufficient funds, network errors)
- **3 days**: Allows time for customer to update payment method
- **7 days**: Final attempt after extended grace period

### Custom Schedules

```toml
# Aggressive recovery (faster retries)
[dunning]
max_retries = 5
retry_intervals_days = [1, 2, 3, 5, 7]
grace_period_days = 18

# Gentle recovery (slower, more customer-friendly)
[dunning]
max_retries = 3
retry_intervals_days = [3, 7, 14]
grace_period_days = 24

# Minimal (fewer attempts)
[dunning]
max_retries = 2
retry_intervals_days = [3, 7]
grace_period_days = 10
```

### Grace Periods

The grace period determines how long a subscription remains active during dunning:

```toml
[dunning]
grace_period_days = 14  # Subscription stays active for 14 days after first failure
```

During the grace period:
- Customer retains access to subscription benefits
- Subscription status is `PastDue` (not `Cancelled`)
- No new invoices are generated
- Customer can update payment method anytime

### Late Fees (Optional)

```toml
[dunning]
late_fee_after_retry = 2      # Apply late fee after 2nd retry
late_fee_amount = "5.00"      # $5.00 late fee
```

Late fees are added to the invoice total on subsequent retry attempts.

## Dunning Email Types & Templates

### Email Sequence

```
Day 0:  First Failure      → "Payment Failed - Please Update Your Payment Method"
Day 1:  Retry 1            → (no email, silent retry)
Day 4:  Retry 2            → "Payment Failed Again - Action Required"
Day 11: Retry 3            → "Final Notice: Subscription Cancellation Pending"
Day 11+: Cancelled         → "Subscription Cancelled Due to Non-Payment"
```

### Template Variables

All dunning emails support these variables:

| Variable | Description | Example |
|----------|-------------|---------|
| `{{customer_name}}` | Customer's first name | "John" |
| `{{subscription_id}}` | Subscription identifier | "550e8400-e29b-41d4-a716-446655440000" |
| `{{product_name}}` | Subscription product name | "Premium Coffee Subscription" |
| `{{amount}}` | Amount due | "$29.99" |
| `{{currency}}` | Currency code | "USD" |
| `{{attempt_number}}` | Current retry attempt | "2" |
| `{{max_attempts}}` | Maximum retry attempts | "3" |
| `{{next_retry_date}}` | Next scheduled retry | "January 15, 2026" |
| `{{update_payment_url}}` | Payment method update link | "https://..." |
| `{{account_url}}` | Customer account portal | "https://..." |

### First Failure Email

**Tone**: Friendly, helpful, assumptive
**Goal**: Get customer to update payment method

```html
Subject: Payment Failed - Please Update Your Payment Method

Hi {{customer_name}},

We were unable to process your subscription payment for {{product_name}}.

Amount Due: {{amount}}
Next Retry: {{next_retry_date}}

Don't worry - your subscription is still active! Please update your payment 
method to avoid any interruption:

[Update Payment Method]({{update_payment_url}})

Common reasons for payment failure:
• Expired credit card
• Insufficient funds
• Bank security block

Questions? Reply to this email or contact support.

Thanks,
The Team
```

### Retry Failure Email

**Tone**: More urgent, emphasizing action needed
**Goal**: Create urgency without being threatening

```html
Subject: Payment Failed Again - Action Required

Hi {{customer_name}},

Your subscription payment failed again (Attempt {{attempt_number}} of {{max_attempts}}).

Amount Due: {{amount}}
Next Retry: {{next_retry_date}}

To keep your {{product_name}} active, please update your payment method:

[Update Payment Method Now]({{update_payment_url}})

If you don't update your payment method, we'll try again on {{next_retry_date}}.
After {{max_attempts}} attempts, your subscription will be cancelled.

[Update Payment Method]({{update_payment_url}})
```

### Final Notice Email

**Tone**: Serious, final warning
**Goal**: Last chance to prevent cancellation

```html
Subject: Final Notice: Subscription Cancellation Pending

Hi {{customer_name}},

This is your final notice. We've attempted to process your payment 
{{max_attempts}} times without success.

Amount Due: {{amount}}
Final Retry: {{next_retry_date}}

Your subscription will be cancelled if payment is not received by {{next_retry_date}}.

[Update Payment Method Immediately]({{update_payment_url}})

We'd hate to see you go! Please update your payment method to keep your 
{{product_name}} subscription active.

Need help? Contact our support team immediately.
```

### Payment Recovered Email

**Tone**: Positive, celebratory
**Goal**: Confirm success and rebuild confidence

```html
Subject: Payment Successful - Subscription Active ✓

Hi {{customer_name}},

Great news! Your payment was successfully processed.

Amount Charged: {{amount}}
Next Billing: {{next_billing_date}}

Your {{product_name}} subscription is now active and will continue without interruption.

Thank you for being a valued customer!

[View Account]({{account_url}})
```

### Cancellation Notice Email

**Tone**: Professional, regretful, offers reactivation
**Goal**: Clean closure, leaves door open

```html
Subject: Subscription Cancelled Due to Non-Payment

Hi {{customer_name}},

Your {{product_name}} subscription has been cancelled due to non-payment.

We attempted to process payment {{max_attempts}} times over the past {{grace_period_days}} days.

We'd love to have you back! To reactivate your subscription:

[Reactivate Subscription]({{reactivate_url}})

If you have any questions or believe this was an error, please contact our support team.

Thank you for your past business.
```

## Payment Retry Logic

### Retry Execution

```rust
pub async fn retry_payment(
    &self,
    subscription: &mut Subscription,
    invoice: &mut SubscriptionInvoice,
) -> Result<PaymentRecoveryResult> {
    // Attempt payment through gateway
    let payment_result = self.charge_subscription(subscription, invoice).await;

    match payment_result {
        Ok(payment) => {
            // Success - update subscription and invoice
            invoice.status = InvoiceStatus::Paid;
            invoice.paid_at = Some(Utc::now());
            invoice.payment_id = Some(payment.id);
            invoice.failed_attempts = 0;
            
            subscription.status = SubscriptionStatus::Active;
            subscription.last_billing_at = Some(Utc::now());
            subscription.next_billing_at = self.calculate_next_billing_date(subscription);
            subscription.current_cycle += 1;

            // Send recovery email
            self.send_payment_recovered_email(subscription, invoice).await?;

            Ok(PaymentRecoveryResult::Success)
        }
        Err(e) => {
            // Failed - process as new failure
            self.process_failed_payment(
                subscription,
                invoice,
                &e.to_string(),
                None,
            ).await
        }
    }
}
```

### Smart Retry Logic (Future Enhancement)

Different retry strategies based on failure type:

```rust
pub enum FailureType {
    InsufficientFunds,    // Retry quickly (funds may be available soon)
    ExpiredCard,          // Wait longer (customer needs to update card)
    BankDecline,          // Standard retry schedule
    FraudSuspicion,       // Don't retry (requires manual review)
    NetworkError,         // Retry immediately
    GatewayError,         // Retry with backoff
}

impl FailureType {
    fn retry_interval(&self) -> Duration {
        match self {
            FailureType::InsufficientFunds => Duration::hours(6),  // Same day retry
            FailureType::ExpiredCard => Duration::days(3),         // Wait for update
            FailureType::NetworkError => Duration::minutes(30),    // Quick retry
            _ => Duration::days(1),                                // Default
        }
    }
}
```

## Cancellation Policies

### Automatic Cancellation

When all retries are exhausted:

```rust
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

    Ok(())
}
```

### Cancellation Behaviors

| Setting | Behavior |
|---------|----------|
| Immediate | Subscription cancelled immediately, access revoked |
| End of Period | Subscription remains active until period end |
| Grace Period | Subscription active during dunning, cancelled after |

### Reactivation

Cancelled subscriptions can be reactivated:

```bash
POST /api/v1/subscriptions/{id}/reactivate
{
  "payment_method_id": "pm_new_card_123"
}
```

Reactivation creates a new subscription with:
- Same product and terms
- New billing cycle starting immediately
- No historical retry attempts

## Database Schema

### Core Tables

```sql
-- Dunning configuration (per-tenant or global)
CREATE TABLE dunning_configs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID REFERENCES tenants(id),
    max_retries INTEGER NOT NULL DEFAULT 3,
    retry_intervals_days INTEGER[] NOT NULL DEFAULT '{1, 3, 7}',
    grace_period_days INTEGER NOT NULL DEFAULT 14,
    email_on_first_failure BOOLEAN NOT NULL DEFAULT true,
    email_on_final_failure BOOLEAN NOT NULL DEFAULT true,
    late_fee_after_retry INTEGER,
    late_fee_amount DECIMAL(20, 2),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id)
);

-- Payment retry attempts
CREATE TABLE payment_retry_attempts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES subscription_invoices(id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    succeeded BOOLEAN NOT NULL DEFAULT false,
    error_message TEXT,
    error_code VARCHAR(100),
    next_retry_at TIMESTAMPTZ,
    payment_method_id VARCHAR(255),
    gateway_transaction_id VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_payment_retry_attempts_subscription_id 
    ON payment_retry_attempts(subscription_id);
CREATE INDEX idx_payment_retry_attempts_invoice_id 
    ON payment_retry_attempts(invoice_id);
CREATE INDEX idx_payment_retry_attempts_next_retry_at 
    ON payment_retry_attempts(next_retry_at) 
    WHERE next_retry_at IS NOT NULL;

-- Dunning emails sent
CREATE TABLE dunning_emails (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES subscription_invoices(id) ON DELETE CASCADE,
    email_type dunning_email_type NOT NULL,
    subject VARCHAR(500) NOT NULL,
    body_html TEXT NOT NULL,
    body_text TEXT NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    opened_at TIMESTAMPTZ,
    clicked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_dunning_emails_subscription_id 
    ON dunning_emails(subscription_id);
CREATE INDEX idx_dunning_emails_invoice_id 
    ON dunning_emails(invoice_id);
CREATE INDEX idx_dunning_emails_sent_at 
    ON dunning_emails(sent_at);

-- Dunning email type enum
CREATE TYPE dunning_email_type AS ENUM (
    'first_failure',
    'retry_failure', 
    'final_notice',
    'cancellation_notice',
    'payment_recovered'
);
```

### Subscription Invoice Updates

```sql
-- Existing subscription_invoices table additions
ALTER TABLE subscription_invoices 
    ADD COLUMN IF NOT EXISTS failed_attempts INTEGER DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_failed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS failure_reason TEXT,
    ADD COLUMN IF NOT EXISTS next_retry_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS retry_count INTEGER DEFAULT 0;

CREATE INDEX idx_subscription_invoices_next_retry_at 
    ON subscription_invoices(next_retry_at) 
    WHERE next_retry_at IS NOT NULL AND status = 'failed';
```

## Integration with Payment Gateways

### Gateway-Agnostic Design

The dunning system uses the existing `PaymentGateway` trait:

```rust
#[async_trait]
pub trait PaymentGateway: Send + Sync {
    async fn create_payment(&self, request: CreatePaymentRequest) -> Result<PaymentSession>;
    async fn confirm_payment(&self, session_id: &str) -> Result<Payment>;
    async fn create_customer(&self, email: &str, name: &str) -> Result<Customer>;
    // ... other methods
}
```

### Gateway-Specific Considerations

#### Stripe

```rust
impl DunningManager<StripeGateway> {
    async fn retry_with_stripe(&self, subscription: &Subscription, invoice: &SubscriptionInvoice) -> Result<Payment> {
        // Use Stripe's PaymentIntent for retries
        let payment_intent = stripe::PaymentIntent::create(
            &self.gateway.client,
            stripe::CreatePaymentIntent {
                amount: (invoice.total * 100).to_i64(), // Convert to cents
                currency: subscription.currency.to_string(),
                customer: Some(subscription.customer_id.to_string()),
                payment_method: subscription.payment_method_id.clone(),
                off_session: Some(true), // Off-session for retries
                confirm: Some(true),
                ..Default::default()
            }
        ).await?;

        // Handle requires_action (3D Secure, etc.)
        if payment_intent.status == stripe::PaymentIntentStatus::RequiresAction {
            // Send email with authentication link
            self.send_authentication_required_email(subscription, invoice, &payment_intent.id).await?;
        }

        Ok(Payment::from(payment_intent))
    }
}
```

#### Card Network Rules

Following card network best practices:

- **Visa**: Maximum 15 retries in 30 days per card
- **Mastercard**: Maximum 10 retries in 24 hours
- **Amex**: Retry after 24 hours minimum

Our default schedule (1, 3, 7 days) complies with all networks.

## Webhook Handling for Failed Payments

### Stripe Webhook Handler

```rust
async fn handle_stripe_webhook(
    &self,
    event: stripe::Event,
) -> Result<()> {
    match event.type_ {
        stripe::EventType::InvoicePaymentFailed => {
            let invoice = event.data.object.as_invoice()
                .ok_or_else(|| Error::validation("Invalid invoice data"))?;
            
            // Extract subscription and failure info
            let subscription_id = invoice.subscription
                .ok_or_else(|| Error::validation("No subscription in invoice"))?;
            
            let payment_intent = invoice.payment_intent
                .and_then(|pi| pi.as_str())
                .map(|id| id.to_string());
            
            // Get failure message from charge
            let error_message = invoice.charge
                .as_ref()
                .and_then(|c| c.as_object())
                .and_then(|c| c.failure_message.clone())
                .unwrap_or_else(|| "Payment failed".to_string());
            
            // Load subscription and invoice
            let mut subscription = self.subscription_repo
                .find_by_gateway_id(&subscription_id)
                .await?
                .ok_or_else(|| Error::not_found("Subscription"))?;
            
            let mut invoice = self.invoice_repo
                .find_by_gateway_id(&invoice.id)
                .await?
                .ok_or_else(|| Error::not_found("Invoice"))?;
            
            // Process failed payment
            self.dunning_manager
                .process_failed_payment(
                    &mut subscription,
                    &mut invoice,
                    &error_message,
                    None,
                )
                .await?;
        }
        // ... handle other event types
    }
    
    Ok(())
}
```

### Airwallex Webhook Handler

```rust
async fn handle_airwallex_webhook(
    &self,
    event: airwallex::WebhookEvent,
) -> Result<()> {
    match event.name {
        "payment_attempt_failed" => {
            let payment_id = event.data.payment_id;
            let error_code = event.data.error_code;
            let error_message = event.data.error_message;
            
            // Load and process
            self.process_gateway_failure(&payment_id, &error_message, Some(&error_code))
                .await?;
        }
        // ... other events
    }
    
    Ok(())
}
```

## Job Scheduling

### Retry Job Queue

Retries are scheduled using the job system:

```rust
pub struct DunningRetryJob {
    subscription_id: Uuid,
    invoice_id: Uuid,
    scheduled_at: DateTime<Utc>,
}

#[async_trait]
impl Job for DunningRetryJob {
    async fn execute(&self, ctx: &JobContext) -> Result<JobResult> {
        // Load subscription and invoice
        let mut subscription = ctx.subscription_repo
            .find_by_id(self.subscription_id)
            .await?
            .ok_or_else(|| Error::not_found("Subscription"))?;
        
        let mut invoice = ctx.invoice_repo
            .find_by_id(self.invoice_id)
            .await?
            .ok_or_else(|| Error::not_found("Invoice"))?;
        
        // Execute retry
        let result = ctx.dunning_manager
            .retry_payment(&mut subscription, &mut invoice)
            .await?;
        
        match result {
            PaymentRecoveryResult::Success => {
                info!("Payment recovered for subscription {}", self.subscription_id);
                Ok(JobResult::Success)
            }
            PaymentRecoveryResult::RetryScheduled { next_retry_at, .. } => {
                info!("Retry scheduled for {}", next_retry_at);
                Ok(JobResult::Success)
            }
            PaymentRecoveryResult::FailedPermanent { .. } => {
                warn!("Subscription {} cancelled due to non-payment", self.subscription_id);
                Ok(JobResult::Success)
            }
        }
    }
}
```

### Scheduled Job Configuration

```rust
// Schedule retry jobs
pub async fn schedule_dunning_retry(
    &self,
    subscription_id: Uuid,
    invoice_id: Uuid,
    retry_at: DateTime<Utc>,
) -> Result<()> {
    let job = DunningRetryJob {
        subscription_id,
        invoice_id,
        scheduled_at: retry_at,
    };
    
    self.job_queue.schedule(job, retry_at).await?;
    
    Ok(())
}
```

## Analytics & Reporting

### Dunning Metrics

Key metrics to track:

```rust
pub struct DunningMetrics {
    // Recovery rates
    pub recovery_rate: f64,              // % of failed payments recovered
    pub recovery_by_attempt: Vec<f64>,   // Recovery rate per attempt number
    
    // Volume metrics
    pub total_failures: i64,
    pub total_recoveries: i64,
    pub total_cancellations: i64,
    
    // Time metrics
    pub avg_recovery_time_hours: f64,    // Average time to recover
    pub avg_attempts_to_recovery: f64,   // Average attempts before recovery
    
    // Revenue metrics
    pub recovered_revenue: Decimal,
    pub lost_revenue: Decimal,
    pub recovery_roi: f64,
}
```

### Sample Queries

```sql
-- Recovery rate by month
SELECT 
    DATE_TRUNC('month', attempted_at) as month,
    COUNT(*) FILTER (WHERE succeeded) as recoveries,
    COUNT(*) as total_attempts,
    ROUND(
        COUNT(*) FILTER (WHERE succeeded)::numeric / 
        NULLIF(COUNT(*)::numeric, 0) * 100, 
        2
    ) as recovery_rate
FROM payment_retry_attempts
GROUP BY DATE_TRUNC('month', attempted_at)
ORDER BY month DESC;

-- Recovery by attempt number
SELECT 
    attempt_number,
    COUNT(*) FILTER (WHERE succeeded) as recoveries,
    COUNT(*) as attempts,
    ROUND(
        COUNT(*) FILTER (WHERE succeeded)::numeric / 
        NULLIF(COUNT(*)::numeric, 0) * 100, 
        2
    ) as recovery_rate
FROM payment_retry_attempts
GROUP BY attempt_number
ORDER BY attempt_number;

-- Email effectiveness
SELECT 
    de.email_type,
    COUNT(*) as sent,
    COUNT(de.opened_at) as opened,
    COUNT(de.clicked_at) as clicked,
    ROUND(COUNT(de.opened_at)::numeric / COUNT(*)::numeric * 100, 2) as open_rate,
    ROUND(COUNT(de.clicked_at)::numeric / COUNT(*)::numeric * 100, 2) as click_rate
FROM dunning_emails de
GROUP BY de.email_type;
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::MockPaymentGateway;

    #[tokio::test]
    async fn test_dunning_processes_first_failure() {
        let gateway = MockPaymentGateway::new();
        let config = DunningConfig::default();
        let manager = DunningManager::new(gateway, config);
        
        let mut subscription = create_test_subscription();
        let mut invoice = create_test_invoice();
        
        let result = manager
            .process_failed_payment(
                &mut subscription,
                &mut invoice,
                "Card declined",
                Some("insufficient_funds"),
            )
            .await
            .unwrap();
        
        assert!(matches!(result, PaymentRecoveryResult::RetryScheduled { .. }));
        assert_eq!(subscription.status, SubscriptionStatus::PastDue);
        assert_eq!(invoice.failed_attempts, 1);
    }

    #[tokio::test]
    async fn test_dunning_cancels_after_max_retries() {
        let gateway = MockPaymentGateway::new();
        let config = DunningConfig::default();
        let manager = DunningManager::new(gateway, config);
        
        let mut subscription = create_test_subscription();
        let mut invoice = create_test_invoice();
        invoice.failed_attempts = 2; // Already had 2 failures
        
        let result = manager
            .process_failed_payment(
                &mut subscription,
                &mut invoice,
                "Card declined",
                None,
            )
            .await
            .unwrap();
        
        assert!(matches!(result, PaymentRecoveryResult::FailedPermanent { .. }));
        assert_eq!(subscription.status, SubscriptionStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_retry_payment_success() {
        let mut gateway = MockPaymentGateway::new();
        gateway.expect_charge_subscription()
            .return_once(|_, _| Ok(create_test_payment()));
        
        let config = DunningConfig::default();
        let manager = DunningManager::new(gateway, config);
        
        let mut subscription = create_test_subscription();
        subscription.status = SubscriptionStatus::PastDue;
        let mut invoice = create_test_invoice();
        invoice.failed_attempts = 1;
        
        let result = manager
            .retry_payment(&mut subscription, &mut invoice)
            .await
            .unwrap();
        
        assert_eq!(result, PaymentRecoveryResult::Success);
        assert_eq!(subscription.status, SubscriptionStatus::Active);
        assert_eq!(invoice.status, InvoiceStatus::Paid);
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_full_dunning_workflow() {
    // Setup test environment
    let (db, gateway) = setup_test_env().await;
    
    // Create subscription with failing payment method
    let subscription = create_subscription_with_failing_card(&db).await;
    
    // Simulate initial billing failure
    let invoice = simulate_billing_failure(&db, &subscription).await;
    
    // Process failure
    let dunning = DunningManager::new(gateway.clone(), DunningConfig::default());
    dunning.process_failed_payment(
        &mut subscription.clone(),
        &mut invoice.clone(),
        "Card declined",
        None,
    ).await.unwrap();
    
    // Verify retry scheduled
    let retry_job = db.get_next_dunning_retry().await.unwrap();
    assert!(retry_job.is_some());
    
    // Simulate successful retry
    gateway.set_next_payment_result(Ok(create_test_payment()));
    
    // Execute retry job
    execute_job(retry_job.unwrap()).await;
    
    // Verify subscription active
    let updated = db.get_subscription(subscription.id).await.unwrap();
    assert_eq!(updated.status, SubscriptionStatus::Active);
}
```

## Future Enhancements

### 1. Smart Retry Logic

Machine learning-based retry optimization:

```rust
pub struct SmartRetryEngine {
    model: RetryPredictionModel,
}

impl SmartRetryEngine {
    pub fn optimal_retry_time(
        &self,
        failure_history: &[PaymentRetryAttempt],
        customer_segment: CustomerSegment,
    ) -> DateTime<Utc> {
        // ML model predicts best retry time based on:
        // - Historical success patterns
        // - Customer segment (payday timing, etc.)
        // - Failure type
        // - Time of day/week
        self.model.predict_optimal_time(failure_history, customer_segment)
    }
}
```

### 2. SMS Notifications

Additional channel for urgent notices:

```rust
pub enum DunningChannel {
    Email,
    Sms,
    Push,
    InApp,
}

pub struct MultiChannelDunning {
    channels: Vec<Box<dyn DunningChannel>>,
}
```

### 3. Payment Method Auto-Update

Integration with card account updater services:

```rust
pub trait AccountUpdater {
    async fn update_expired_card(&self, payment_method_id: &str) -> Result<Option<CardDetails>>;
}

// Visa Account Updater
// Mastercard Automatic Billing Updater
// Amex Cardrefresher
```

### 4. Grace Period Extensions

Manual extensions for high-value customers:

```rust
pub async fn extend_grace_period(
    &self,
    subscription_id: Uuid,
    extension_days: i32,
    reason: &str,
) -> Result<()> {
    // Add extra retry attempt
    // Extend grace period
    // Log reason for audit
}
```

### 5. Dunning Analytics Dashboard

Real-time metrics and insights:

- Recovery funnel visualization
- Revenue at risk
- Email performance metrics
- A/B testing framework for email templates

---

## Related Documentation

- [Product Types and Subscriptions](09-product-types-and-subscriptions.md)
- [Payment Architecture](05-payment-architecture.md)
- [Notifications](10-notifications.md)
- [User Guide: Dunning](../../docs-website/docs/guides/dunning.md)
