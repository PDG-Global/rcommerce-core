//! Agnostic Payment System
//!
//! Provides a unified interface for all payment operations regardless of the gateway.
//! The frontend interacts with our API only - never directly with payment providers.

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;

/// Payment method types supported by a gateway
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethodType {
    /// Credit/Debit card
    Card,
    /// Google Pay
    GooglePay,
    /// Apple Pay
    ApplePay,
    /// Alipay
    Alipay,
    /// WeChat Pay
    WechatPay,
    /// PayPal
    PayPal,
    /// Bank transfer / ACH
    BankTransfer,
    /// Buy Now Pay Later (Klarna, Afterpay, etc)
    BuyNowPayLater,
    /// Cryptocurrency
    Crypto,
    /// Cash on delivery
    CashOnDelivery,
}

impl PaymentMethodType {
    pub fn display_name(&self) -> &'static str {
        match self {
            PaymentMethodType::Card => "Credit/Debit Card",
            PaymentMethodType::GooglePay => "Google Pay",
            PaymentMethodType::ApplePay => "Apple Pay",
            PaymentMethodType::Alipay => "Alipay",
            PaymentMethodType::WechatPay => "WeChat Pay",
            PaymentMethodType::PayPal => "PayPal",
            PaymentMethodType::BankTransfer => "Bank Transfer",
            PaymentMethodType::BuyNowPayLater => "Buy Now Pay Later",
            PaymentMethodType::Crypto => "Cryptocurrency",
            PaymentMethodType::CashOnDelivery => "Cash on Delivery",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            PaymentMethodType::Card => "card",
            PaymentMethodType::GooglePay => "google_pay",
            PaymentMethodType::ApplePay => "apple_pay",
            PaymentMethodType::Alipay => "alipay",
            PaymentMethodType::WechatPay => "wechat_pay",
            PaymentMethodType::PayPal => "paypal",
            PaymentMethodType::BankTransfer => "bank",
            PaymentMethodType::BuyNowPayLater => "installments",
            PaymentMethodType::Crypto => "crypto",
            PaymentMethodType::CashOnDelivery => "cash",
        }
    }
}

/// Payment method configuration for a gateway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethodConfig {
    /// Type of payment method
    pub method_type: PaymentMethodType,
    /// Whether this method is enabled
    pub enabled: bool,
    /// Display name (can be customized per merchant)
    pub display_name: String,
    /// Whether this method requires redirect (e.g., PayPal, Alipay)
    pub requires_redirect: bool,
    /// Whether this method supports 3D Secure/SCA
    pub supports_3ds: bool,
    /// Whether this method supports tokenization (save for later)
    pub supports_tokenization: bool,
    /// Whether this method supports recurring payments
    pub supports_recurring: bool,
    /// Required fields for this payment method
    pub required_fields: Vec<FieldDefinition>,
    /// Optional fields for this payment method
    pub optional_fields: Vec<FieldDefinition>,
    /// Currencies supported by this method
    pub supported_currencies: Vec<String>,
    /// Minimum amount for this method
    pub min_amount: Option<Decimal>,
    /// Maximum amount for this method
    pub max_amount: Option<Decimal>,
}

/// Field definition for payment forms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Field name/key
    pub name: String,
    /// Display label
    pub label: String,
    /// Field type
    pub field_type: FieldType,
    /// Whether field is required
    pub required: bool,
    /// Validation regex pattern
    pub pattern: Option<String>,
    /// Placeholder text
    pub placeholder: Option<String>,
    /// Help text
    pub help_text: Option<String>,
}

/// Field types for payment forms
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// Text input
    Text,
    /// Number input
    Number,
    /// Card number (with formatting)
    CardNumber,
    /// Expiry date (MM/YY)
    ExpiryDate,
    /// CVC/CVV code
    Cvc,
    /// Cardholder name
    CardholderName,
    /// Select dropdown
    Select { options: Vec<SelectOption> },
    /// Checkbox
    Checkbox,
    /// Hidden field
    Hidden,
}

/// Select option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// Gateway capabilities and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Gateway ID
    pub gateway_id: String,
    /// Gateway name
    pub gateway_name: String,
    /// Available payment methods
    pub payment_methods: Vec<PaymentMethodConfig>,
    /// Whether 3D Secure is supported
    pub supports_3ds: bool,
    /// Whether webhooks are supported
    pub supports_webhooks: bool,
    /// Whether refunds are supported
    pub supports_refunds: bool,
    /// Whether partial refunds are supported
    pub supports_partial_refunds: bool,
    /// Supported currencies
    pub supported_currencies: Vec<String>,
    /// Default currency
    pub default_currency: String,
}

/// Request to initiate a payment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiatePaymentRequest {
    /// Amount to charge
    pub amount: Decimal,
    /// Currency code (ISO 4217)
    pub currency: String,
    /// Selected payment method type
    pub payment_method_type: PaymentMethodType,
    /// Order ID this payment is for
    pub order_id: Uuid,
    /// Customer ID (if known)
    pub customer_id: Option<Uuid>,
    /// Customer email
    pub customer_email: String,
    /// Customer IP address
    pub customer_ip: Option<String>,
    /// Billing address
    pub billing_address: Option<Address>,
    /// Shipping address
    pub shipping_address: Option<Address>,
    /// Payment method data (card details, etc.)
    pub payment_method_data: PaymentMethodData,
    /// Whether to save this payment method for future use
    pub save_payment_method: bool,
    /// Description of the purchase
    pub description: String,
    /// Metadata for the payment
    pub metadata: serde_json::Value,
}

/// Payment method data - varies by type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentMethodData {
    /// Card payment data
    Card {
        /// Card number (tokenized or raw - gateway handles security)
        number: String,
        /// Expiry month (MM)
        exp_month: String,
        /// Expiry year (YYYY)
        exp_year: String,
        /// CVC/CVV
        cvc: String,
        /// Cardholder name
        name: String,
    },
    /// Tokenized card (for returning customers)
    CardToken {
        /// Payment method token/ID
        token: String,
    },
    /// Digital wallet data
    DigitalWallet {
        /// Wallet type
        wallet_type: String,
        /// Payment token from wallet
        token: String,
    },
    /// Bank transfer data
    BankTransfer {
        /// Account number
        account_number: String,
        /// Routing number / sort code
        routing_number: String,
        /// Account holder name
        account_holder_name: String,
        /// Bank name
        bank_name: String,
    },
    /// Redirect-based payment (PayPal, Alipay, etc.)
    Redirect {
        /// Return URL after payment
        return_url: String,
        /// Cancel URL
        cancel_url: String,
    },
    /// Cash on delivery
    CashOnDelivery {
        /// Additional instructions
        instructions: Option<String>,
    },
}

/// Response from initiating a payment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum InitiatePaymentResponse {
    /// Payment requires additional action (3DS, redirect, etc.)
    RequiresAction {
        /// Payment ID
        payment_id: String,
        /// Action type
        action_type: PaymentActionType,
        /// Action data (redirect URL, 3DS data, etc.)
        action_data: serde_json::Value,
        /// Expires at
        expires_at: chrono::DateTime<chrono::Utc>,
    },
    /// Payment succeeded immediately
    Success {
        /// Payment ID
        payment_id: String,
        /// Transaction ID from gateway
        transaction_id: String,
        /// Payment status
        payment_status: PaymentStatus,
        /// Payment method used
        payment_method: PaymentMethodInfo,
        /// Receipt URL (if available)
        receipt_url: Option<String>,
    },
    /// Payment failed
    Failed {
        /// Payment ID
        payment_id: String,
        /// Error code
        error_code: String,
        /// Error message
        error_message: String,
        /// Whether retry is allowed
        retry_allowed: bool,
    },
}

/// Payment action types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentActionType {
    /// 3D Secure authentication required
    ThreeDSecure,
    /// Redirect to payment provider
    Redirect,
    /// Additional verification required
    Verification,
    /// Device fingerprinting
    DeviceFingerprint,
    /// Challenge/OTP required
    Challenge,
}

/// Payment status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    /// Payment initiated
    Pending,
    /// Payment being processed
    Processing,
    /// Payment requires action
    RequiresAction,
    /// Payment succeeded
    Succeeded,
    /// Payment failed
    Failed,
    /// Payment cancelled
    Cancelled,
    /// Payment refunded
    Refunded,
    /// Partially refunded
    PartiallyRefunded,
    /// Payment disputed
    Disputed,
}

/// Payment method information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethodInfo {
    /// Payment method type
    pub method_type: PaymentMethodType,
    /// Last 4 digits (for cards)
    pub last_four: Option<String>,
    /// Card brand (for cards)
    pub card_brand: Option<String>,
    /// Expiry month
    pub exp_month: Option<String>,
    /// Expiry year
    pub exp_year: Option<String>,
    /// Cardholder name
    pub cardholder_name: Option<String>,
    /// Payment method token (if saved)
    pub token: Option<String>,
}

/// Address structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub line1: String,
    pub line2: Option<String>,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

/// Request to complete a payment action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletePaymentActionRequest {
    /// Payment ID
    pub payment_id: String,
    /// Action type being completed
    pub action_type: PaymentActionType,
    /// Action data (3DS result, redirect result, etc.)
    pub action_data: serde_json::Value,
}

/// Response from completing a payment action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum CompletePaymentActionResponse {
    /// Payment succeeded
    Success {
        payment_id: String,
        transaction_id: String,
        payment_status: PaymentStatus,
        payment_method: PaymentMethodInfo,
        receipt_url: Option<String>,
    },
    /// Additional action required
    RequiresAction {
        payment_id: String,
        action_type: PaymentActionType,
        action_data: serde_json::Value,
    },
    /// Payment failed
    Failed {
        payment_id: String,
        error_code: String,
        error_message: String,
        retry_allowed: bool,
    },
}

/// Unified payment gateway trait
#[async_trait]
pub trait AgnosticPaymentGateway: Send + Sync {
    /// Get gateway configuration
    async fn get_config(&self) -> Result<GatewayConfig>;

    /// Initiate a payment
    async fn initiate_payment(
        &self,
        request: InitiatePaymentRequest,
    ) -> Result<InitiatePaymentResponse>;

    /// Complete a payment action (3DS, redirect return, etc.)
    async fn complete_payment_action(
        &self,
        request: CompletePaymentActionRequest,
    ) -> Result<CompletePaymentActionResponse>;

    /// Get payment status
    async fn get_payment_status(&self, payment_id: &str) -> Result<PaymentStatus>;

    /// Refund a payment
    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<RefundResponse>;

    /// Handle webhook from payment provider
    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &[(String, String)],
    ) -> Result<WebhookEvent>;

    /// Tokenize a payment method for future use
    async fn tokenize_payment_method(
        &self,
        payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken>;

    /// Get saved payment methods for a customer
    async fn get_saved_payment_methods(&self, customer_id: &str) -> Result<Vec<PaymentMethodInfo>>;

    /// Delete a saved payment method
    async fn delete_payment_method(&self, token: &str) -> Result<()>;
}

/// Refund response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundResponse {
    pub refund_id: String,
    pub payment_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub status: RefundStatus,
    pub reason: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Refund status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefundStatus {
    Pending,
    Processing,
    Succeeded,
    Failed,
}

/// Payment method token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethodToken {
    pub token: String,
    pub payment_method: PaymentMethodInfo,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Webhook event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub event_type: WebhookEventType,
    pub payment_id: String,
    pub transaction_id: Option<String>,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Webhook event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    PaymentPending,
    PaymentProcessing,
    PaymentSucceeded,
    PaymentFailed,
    PaymentCancelled,
    PaymentRefunded,
    PaymentPartiallyRefunded,
    DisputeCreated,
    DisputeResolved,
    SubscriptionCreated,
    SubscriptionCancelled,
    SubscriptionPaymentSucceeded,
    SubscriptionPaymentFailed,
}

/// Payment service - orchestrates payments across multiple gateways
pub struct PaymentService {
    gateways: std::collections::HashMap<String, Box<dyn AgnosticPaymentGateway>>,
    default_gateway: String,
}

impl PaymentService {
    pub fn new(default_gateway: String) -> Self {
        Self {
            gateways: std::collections::HashMap::new(),
            default_gateway,
        }
    }

    pub fn register_gateway(
        &mut self,
        gateway_id: String,
        gateway: Box<dyn AgnosticPaymentGateway>,
    ) {
        self.gateways.insert(gateway_id, gateway);
    }

    pub fn get_gateway(&self, gateway_id: Option<&str>) -> Option<&dyn AgnosticPaymentGateway> {
        let id = gateway_id.unwrap_or(&self.default_gateway);
        self.gateways.get(id).map(|g| g.as_ref())
    }

    /// Get available payment methods across all gateways
    pub async fn get_available_payment_methods(
        &self,
        currency: &str,
        amount: Decimal,
    ) -> Vec<(String, PaymentMethodConfig)> {
        let mut methods = Vec::new();

        for (gateway_id, gateway) in &self.gateways {
            if let Ok(config) = gateway.get_config().await {
                // Check if gateway supports this currency
                if !config.supported_currencies.contains(&currency.to_string()) {
                    continue;
                }

                for method in config.payment_methods {
                    if !method.enabled {
                        continue;
                    }

                    // Check if method supports this currency
                    if !method.supported_currencies.is_empty()
                        && !method.supported_currencies.contains(&currency.to_string())
                    {
                        continue;
                    }

                    // Check amount limits
                    if let Some(min) = method.min_amount {
                        if amount < min {
                            continue;
                        }
                    }
                    if let Some(max) = method.max_amount {
                        if amount > max {
                            continue;
                        }
                    }

                    methods.push((gateway_id.clone(), method));
                }
            }
        }

        methods
    }
}
