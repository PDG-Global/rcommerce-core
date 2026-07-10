# Dunning and Payment Retry Architecture

## Overview

The dunning system provides automated payment retry logic for failed subscription payments, with escalating customer communications and graceful subscription cancellation when recovery fails.

## Components

### DunningConfig

Configurable retry schedule and notification settings:

```rust
pub struct DunningConfig {
    pub max_retries: i32,                    // Default: 3
    pub retry_intervals_days: Vec<i32>,      // Default: [1, 3, 7]
    pub grace_period_days: i32,              // Default: 14
    pub email_on_first_failure: bool,        // Default: true
    pub email_on_final_failure: bool,        // Default: true
    pub late_fee_after_retry: Option<i32>,   // Optional
    pub late_fee_amount: Option<Decimal>,    // Optional
}
```

### DunningManager

Core dunning workflow handler:

- **`process_failed_payment()`** - Entry point for handling failed payments
- **`retry_payment()`** - Attempts to charge a subscription again
- **`charge_subscription()`** - Creates and confirms payment through gateway
- **`cancel_subscription_for_non_payment()`** - Final cancellation when retries exhausted

### PaymentRetryAttempt

Tracks each retry attempt:

```rust
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
}
```

### DunningEmail

Customer communication tracking:

```rust
pub struct DunningEmail {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub invoice_id: Uuid,
    pub email_type: DunningEmailType,  // FirstFailure, RetryFailure, FinalNotice, etc.
    pub subject: String,
    pub body_html: String,
    pub body_text: String,
    pub sent_at: DateTime<Utc>,
    pub opened_at: Option<DateTime<Utc>>,
    pub clicked_at: Option<DateTime<Utc>>,
}
```

## Workflow

### Failed Payment Handling

1. Payment fails during subscription billing
2. `process_failed_payment()` is called
3. Failed attempt is recorded
4. If retries exhausted → cancel subscription
5. Otherwise → schedule next retry
6. Update subscription status to `PastDue`
7. Send appropriate dunning email

### Retry Scheduling

Default retry schedule: **Day 1, Day 3, Day 7**

| Attempt | Retry After | Email Sent |
|---------|-------------|------------|
| 1st failure | 1 day | First Failure Notice |
| 2nd failure | 3 days | Retry Failure Notice |
| 3rd failure | 7 days | Final Notice |
| Exhausted | Cancel | Cancellation Notice |

### Email Templates

- **FirstFailure**: Friendly reminder to update payment method
- **RetryFailure**: Urgent notice with attempt count
- **FinalNotice**: Last warning before cancellation
- **CancellationNotice**: Subscription cancelled notification
- **PaymentRecovered**: Success confirmation

### Subscription Status Flow

```
Active → PastDue (on first failure)
  ↓
PastDue → Active (on successful retry)
  ↓
PastDue → Cancelled (on retry exhaustion)
```

## Integration Points

### With Subscription Model

- `SubscriptionInvoice.failed_attempts` - tracks retry count
- `SubscriptionInvoice.next_retry_at` - scheduled retry time
- `Subscription.status` - reflects payment health

### With Payment Gateway

- Uses existing `PaymentGateway` trait
- `create_payment()` and `confirm_payment()` for retries
- Records gateway transaction IDs

### With Job System

Retry scheduling integrates with the job queue:
- Failed payments create scheduled retry jobs
- Jobs run at calculated retry times
- Retry results trigger notifications

## Configuration Example

```toml
[dunning]
max_retries = 3
retry_intervals_days = [1, 3, 7]
grace_period_days = 14
email_on_first_failure = true
email_on_final_failure = true
late_fee_after_retry = 2
late_fee_amount = "5.00"
```

## Testing

Unit tests for dunning logic:

```rust
#[test]
fn test_dunning_config_default() {
    let config = DunningConfig::default();
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.retry_intervals_days, vec![1, 3, 7]);
}

#[test]
fn test_calculate_next_retry_date() {
    let manager = DunningManager::new(gateway, config);
    let retry_date = manager.calculate_next_retry_date(1);
    // Should be ~1 day in the future
}
```

## Future Enhancements

1. **Smart Retry Logic**: Different retry schedules based on failure type
2. **SMS Notifications**: Additional channel for urgent notices
3. **In-App Notifications**: Dashboard alerts for payment failures
4. **Payment Method Auto-Update**: Integration with account updater services
5. **Grace Period Extensions**: Manual extension for high-value customers
6. **Dunning Analytics**: Recovery rate tracking and optimization
