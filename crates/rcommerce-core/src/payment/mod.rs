pub mod agnostic;
pub mod dunning;
pub mod gateways;

#[cfg(test)]
mod tests;

/// Mock payment gateway for testing
#[derive(Debug, Clone)]
pub struct MockPaymentGateway;

impl Default for MockPaymentGateway {
    fn default() -> Self {
        Self
    }
}

impl MockPaymentGateway {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PaymentGateway for MockPaymentGateway {
    fn id(&self) -> &'static str {
        "mock"
    }

    fn name(&self) -> &'static str {
        "Mock Gateway"
    }

    async fn create_payment(&self, _request: CreatePaymentRequest) -> Result<PaymentSession> {
        Ok(PaymentSession {
            id: "mock_payment_123".to_string(),
            client_secret: "mock_secret".to_string(),
            status: PaymentSessionStatus::Open,
            amount: rust_decimal::Decimal::ONE,
            currency: "USD".to_string(),
            customer_id: None,
        })
    }

    async fn confirm_payment(&self, payment_id: &str) -> Result<Payment> {
        Ok(Payment {
            id: payment_id.to_string(),
            gateway: "mock".to_string(),
            amount: rust_decimal::Decimal::ONE,
            currency: "USD".to_string(),
            status: PaymentStatus::Succeeded,
            order_id: Uuid::new_v4(),
            customer_id: None,
            payment_method: "card".to_string(),
            transaction_id: "mock_txn_123".to_string(),
            client_secret: Some("mock_secret".to_string()),
            card_brand: Some("visa".to_string()),
            card_last_four: Some("4242".to_string()),
            captured_at: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
        })
    }

    async fn capture_payment(
        &self,
        payment_id: &str,
        _amount: Option<rust_decimal::Decimal>,
    ) -> Result<Payment> {
        self.confirm_payment(payment_id).await
    }

    async fn refund_payment(
        &self,
        _payment_id: &str,
        _amount: Option<rust_decimal::Decimal>,
        reason: &str,
    ) -> Result<Refund> {
        Ok(Refund {
            id: "mock_refund_123".to_string(),
            payment_id: "mock_payment_123".to_string(),
            amount: rust_decimal::Decimal::ONE,
            currency: "USD".to_string(),
            status: RefundStatus::Succeeded,
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn get_payment(&self, payment_id: &str) -> Result<Payment> {
        self.confirm_payment(payment_id).await
    }

    async fn handle_webhook(&self, _payload: &[u8], _signature: &str) -> Result<WebhookEvent> {
        Ok(WebhookEvent {
            event_type: WebhookEventType::PaymentSucceeded,
            payment_id: "mock_payment_123".to_string(),
            data: serde_json::json!({}),
        })
    }

    async fn create_customer(&self, email: &str, _name: &str) -> Result<String> {
        // Deterministic fake id derived from the email so tests are stable.
        let hash = email.bytes().map(u64::from).sum::<u64>();
        Ok(format!("mock_customer_{:016x}", hash))
    }

    async fn create_payment_consent(
        &self,
        gateway_customer_id: &str,
        _card: CardDetails,
        merchant_trigger_reason: &str,
    ) -> Result<ConsentCreation> {
        Ok(ConsentCreation {
            consent_id: format!("mock_consent_{}", gateway_customer_id),
            client_secret: format!("mock_consent_secret_{}", merchant_trigger_reason),
        })
    }
}

use async_trait::async_trait;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::models::Address;
use crate::Error;
use crate::Result;
use serde::{Deserialize, Serialize};

/// Payment gateway trait
#[async_trait]
pub trait PaymentGateway: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;

    /// Create a payment intent/session
    async fn create_payment(&self, request: CreatePaymentRequest) -> Result<PaymentSession>;

    /// Confirm a payment
    async fn confirm_payment(&self, payment_id: &str) -> Result<Payment>;

    /// Capture a payment
    async fn capture_payment(&self, payment_id: &str, amount: Option<Decimal>) -> Result<Payment>;

    /// Refund a payment
    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<Refund>;

    /// Get payment details
    async fn get_payment(&self, payment_id: &str) -> Result<Payment>;

    /// Handle webhook
    async fn handle_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookEvent>;

    /// Create a customer at the gateway (e.g. an Airwallex customer to hold
    /// stored-card consents). Returns the gateway's customer id.
    async fn create_customer(&self, _email: &str, _name: &str) -> Result<String> {
        Err(Error::payment_error(
            "create_customer not supported by this gateway",
        ))
    }

    /// Create a stored-card payment consent for merchant-initiated charges.
    async fn create_payment_consent(
        &self,
        _gateway_customer_id: &str,
        _card: CardDetails,
        _merchant_trigger_reason: &str,
    ) -> Result<ConsentCreation> {
        Err(Error::payment_error(
            "create_payment_consent not supported by this gateway",
        ))
    }
}

/// Result of creating a stored-card payment consent
#[derive(Debug, Clone)]
pub struct ConsentCreation {
    pub consent_id: String,
    pub client_secret: String,
}

/// Create payment request
#[derive(Debug, Clone)]
pub struct CreatePaymentRequest {
    pub amount: Decimal,
    pub currency: String,
    pub order_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub customer_email: String,
    pub payment_method: PaymentMethod,
    pub billing_address: Option<Address>,
    /// Stored-card consent id (Airwallex payment_consent) for merchant-
    /// initiated charges such as subscription renewals.
    pub payment_consent_id: Option<String>,
    /// True when the charge is merchant-initiated on a stored consent
    /// (skips customer-present authentication at the gateway).
    pub merchant_initiated: Option<bool>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum PaymentMethod {
    Card(CardDetails),
    StripeElements,
    GooglePay,
    ApplePay,
    WeChatPay,
    AliPay,
    BankTransfer,
    CashOnDelivery,
    /// Charge a previously saved payment method (stored-card consent); the
    /// consent id travels in `CreatePaymentRequest::payment_consent_id`.
    SavedCard,
}

#[derive(Debug, Clone)]
pub struct CardDetails {
    pub number: String,
    pub exp_month: u32,
    pub exp_year: u32,
    pub cvc: String,
    pub name: String,
}
/// Payment session (for client-side checkout)
#[derive(Debug, Clone)]
pub struct PaymentSession {
    pub id: String,
    pub client_secret: String,
    pub status: PaymentSessionStatus,
    pub amount: Decimal,
    pub currency: String,
    pub customer_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaymentSessionStatus {
    Open,
    Complete,
    Expired,
}

/// Payment record
#[derive(Debug, Clone)]
pub struct Payment {
    pub id: String,
    pub gateway: String,
    pub amount: Decimal,
    pub currency: String,
    pub status: PaymentStatus,
    pub order_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub payment_method: String,
    pub transaction_id: String,
    /// Gateway client secret for resuming client-side confirmation
    /// (Stripe/Airwallex), when the retrieve response carries one.
    pub client_secret: Option<String>,
    pub card_brand: Option<String>,
    pub card_last_four: Option<String>,
    pub captured_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaymentStatus {
    Pending,
    Processing,
    Succeeded,
    Failed,
    Canceled,
    Disputed,
    Refunded,
}

/// Refund record
#[derive(Debug, Clone)]
pub struct Refund {
    pub id: String,
    pub payment_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub status: RefundStatus,
    pub reason: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RefundStatus {
    Pending,
    Succeeded,
    Failed,
    Canceled,
}

/// Webhook event
#[derive(Debug, Clone)]
pub struct WebhookEvent {
    pub event_type: WebhookEventType,
    pub payment_id: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WebhookEventType {
    PaymentSucceeded,
    PaymentFailed,
    PaymentCanceled,
    RefundSucceeded,
    DisputeCreated,
    /// A stored-card consent was verified by the gateway (log-only; consent
    /// verification primarily happens synchronously at checkout).
    PaymentConsentVerified,
}
