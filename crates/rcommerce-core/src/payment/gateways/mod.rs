//! Payment gateway implementations

pub mod airwallex;
pub mod airwallex_agnostic;
pub mod alipay;
pub mod alipay_agnostic;
pub mod stripe;
pub mod stripe_agnostic;
pub mod wechatpay;
pub mod wechatpay_agnostic;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::payment::agnostic::{
    AgnosticPaymentGateway, CompletePaymentActionRequest, CompletePaymentActionResponse,
    FieldDefinition, FieldType, GatewayConfig, InitiatePaymentRequest, InitiatePaymentResponse,
    PaymentMethodConfig, PaymentMethodData, PaymentMethodInfo, PaymentMethodToken,
    PaymentMethodType, RefundResponse, RefundStatus as AgnosticRefundStatus,
    WebhookEvent as AgnosticWebhookEvent, WebhookEventType as AgnosticWebhookEventType,
};
use crate::payment::{
    CreatePaymentRequest, Payment, PaymentGateway, PaymentSession, PaymentSessionStatus,
    PaymentStatus, Refund, RefundStatus, WebhookEvent, WebhookEventType,
};
use crate::Result;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Legacy payment response structure (kept for backward compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentResponse {
    pub success: bool,
    pub transaction_id: Option<String>,
    pub amount: rust_decimal::Decimal,
    pub currency: String,
    pub error_message: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Mock payment gateway for local development and testing
#[derive(Debug, Clone, Default)]
pub struct MockPaymentGateway {
    sessions:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, (Decimal, uuid::Uuid)>>>,
    saved_payment_methods:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<PaymentMethodInfo>>>>,
}

impl MockPaymentGateway {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl PaymentGateway for MockPaymentGateway {
    fn id(&self) -> &'static str {
        "mock"
    }

    fn name(&self) -> &'static str {
        "Mock Payment Gateway"
    }

    async fn create_payment(&self, request: CreatePaymentRequest) -> Result<PaymentSession> {
        let id = format!("mock_pay_{}", uuid::Uuid::new_v4());
        let amount = request.amount;
        let order_id = request.order_id;
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(id.clone(), (amount, order_id));
        }
        Ok(PaymentSession {
            id,
            client_secret: format!("mock_secret_{}", uuid::Uuid::new_v4()),
            status: PaymentSessionStatus::Open,
            amount,
            currency: request.currency,
            customer_id: request.customer_id,
        })
    }

    async fn confirm_payment(&self, payment_id: &str) -> Result<Payment> {
        let (amount, order_id) = {
            let sessions = self.sessions.lock().unwrap();
            sessions
                .get(payment_id)
                .cloned()
                .unwrap_or_else(|| (Decimal::new(5000, 2), uuid::Uuid::new_v4()))
        };
        Ok(Payment {
            id: payment_id.to_string(),
            gateway: self.id().to_string(),
            amount,
            currency: "USD".to_string(),
            status: PaymentStatus::Succeeded,
            order_id,
            customer_id: None,
            payment_method: "card".to_string(),
            transaction_id: payment_id.to_string(),
            client_secret: None,
            card_brand: Some("visa".to_string()),
            card_last_four: Some("4242".to_string()),
            captured_at: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
        })
    }

    async fn capture_payment(&self, payment_id: &str, _amount: Option<Decimal>) -> Result<Payment> {
        self.confirm_payment(payment_id).await
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<Refund> {
        Ok(Refund {
            id: format!("ref_{}", uuid::Uuid::new_v4()),
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(Decimal::new(5000, 2)),
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
            payment_id: format!("mock_pay_{}", uuid::Uuid::new_v4()),
            data: serde_json::json!({}),
        })
    }
}

#[async_trait]
impl AgnosticPaymentGateway for MockPaymentGateway {
    async fn get_config(&self) -> Result<GatewayConfig> {
        Ok(GatewayConfig {
            gateway_id: "mock".to_string(),
            gateway_name: "Mock Payment Gateway".to_string(),
            payment_methods: vec![PaymentMethodConfig {
                method_type: PaymentMethodType::Card,
                enabled: true,
                display_name: "Credit/Debit Card (Mock)".to_string(),
                requires_redirect: false,
                supports_3ds: true,
                supports_tokenization: true,
                supports_recurring: true,
                required_fields: vec![
                    FieldDefinition {
                        name: "number".to_string(),
                        label: "Card Number".to_string(),
                        field_type: FieldType::CardNumber,
                        required: true,
                        pattern: Some(r"^[\d\s]{13,19}$".to_string()),
                        placeholder: Some("1234 5678 9012 3456".to_string()),
                        help_text: None,
                    },
                    FieldDefinition {
                        name: "exp_month".to_string(),
                        label: "Expiry Month".to_string(),
                        field_type: FieldType::ExpiryDate,
                        required: true,
                        pattern: Some(r"^(0[1-9]|1[0-2])$".to_string()),
                        placeholder: Some("MM".to_string()),
                        help_text: None,
                    },
                    FieldDefinition {
                        name: "exp_year".to_string(),
                        label: "Expiry Year".to_string(),
                        field_type: FieldType::ExpiryDate,
                        required: true,
                        pattern: Some(r"^20[2-9][0-9]$".to_string()),
                        placeholder: Some("YYYY".to_string()),
                        help_text: None,
                    },
                    FieldDefinition {
                        name: "cvc".to_string(),
                        label: "CVC".to_string(),
                        field_type: FieldType::Cvc,
                        required: true,
                        pattern: Some(r"^\d{3,4}$".to_string()),
                        placeholder: Some("123".to_string()),
                        help_text: Some("3 or 4 digit code on back of card".to_string()),
                    },
                    FieldDefinition {
                        name: "name".to_string(),
                        label: "Cardholder Name".to_string(),
                        field_type: FieldType::CardholderName,
                        required: true,
                        pattern: None,
                        placeholder: Some("John Doe".to_string()),
                        help_text: None,
                    },
                ],
                optional_fields: vec![],
                supported_currencies: vec!["USD".to_string(), "EUR".to_string(), "GBP".to_string()],
                min_amount: Some(dec!(0.50)),
                max_amount: None,
            }],
            supports_3ds: true,
            supports_webhooks: true,
            supports_refunds: true,
            supports_partial_refunds: true,
            supported_currencies: vec!["USD".to_string(), "EUR".to_string(), "GBP".to_string()],
            default_currency: "USD".to_string(),
        })
    }

    async fn initiate_payment(
        &self,
        request: InitiatePaymentRequest,
    ) -> Result<InitiatePaymentResponse> {
        let payment_id = format!("mock_pay_{}", uuid::Uuid::new_v4());

        // Extract card info if available
        let (last_four, card_brand) = match &request.payment_method_data {
            PaymentMethodData::Card { number, .. } => {
                let last_four = number
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>();
                let last_four = if last_four.len() >= 4 {
                    Some(last_four[last_four.len() - 4..].to_string())
                } else {
                    Some("0000".to_string())
                };
                (last_four, Some("visa".to_string()))
            }
            _ => (Some("4242".to_string()), Some("visa".to_string())),
        };

        Ok(InitiatePaymentResponse::Success {
            payment_id: payment_id.clone(),
            transaction_id: format!("mock_txn_{}", uuid::Uuid::new_v4()),
            payment_status: crate::payment::agnostic::PaymentStatus::Succeeded,
            payment_method: PaymentMethodInfo {
                method_type: PaymentMethodType::Card,
                last_four,
                card_brand,
                exp_month: Some("12".to_string()),
                exp_year: Some("2025".to_string()),
                cardholder_name: Some("Mock User".to_string()),
                token: None,
            },
            receipt_url: Some(format!("https://mock.gateway/receipts/{}", payment_id)),
        })
    }

    async fn complete_payment_action(
        &self,
        request: CompletePaymentActionRequest,
    ) -> Result<CompletePaymentActionResponse> {
        Ok(CompletePaymentActionResponse::Success {
            payment_id: request.payment_id,
            transaction_id: format!("mock_txn_{}", uuid::Uuid::new_v4()),
            payment_status: crate::payment::agnostic::PaymentStatus::Succeeded,
            payment_method: PaymentMethodInfo {
                method_type: PaymentMethodType::Card,
                last_four: Some("4242".to_string()),
                card_brand: Some("visa".to_string()),
                exp_month: Some("12".to_string()),
                exp_year: Some("2025".to_string()),
                cardholder_name: Some("Mock User".to_string()),
                token: None,
            },
            receipt_url: Some("https://mock.gateway/receipts/123".to_string()),
        })
    }

    async fn get_payment_status(
        &self,
        _payment_id: &str,
    ) -> Result<crate::payment::agnostic::PaymentStatus> {
        Ok(crate::payment::agnostic::PaymentStatus::Succeeded)
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<RefundResponse> {
        Ok(RefundResponse {
            refund_id: format!("mock_ref_{}", uuid::Uuid::new_v4()),
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(dec!(10.00)),
            currency: "USD".to_string(),
            status: AgnosticRefundStatus::Succeeded,
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn handle_webhook(
        &self,
        _payload: &[u8],
        _headers: &[(String, String)],
    ) -> Result<AgnosticWebhookEvent> {
        Ok(AgnosticWebhookEvent {
            event_type: AgnosticWebhookEventType::PaymentSucceeded,
            payment_id: format!("mock_pay_{}", uuid::Uuid::new_v4()),
            transaction_id: None,
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn tokenize_payment_method(
        &self,
        _payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken> {
        Ok(PaymentMethodToken {
            token: format!("mock_tok_{}", uuid::Uuid::new_v4()),
            payment_method: PaymentMethodInfo {
                method_type: PaymentMethodType::Card,
                last_four: Some("4242".to_string()),
                card_brand: Some("visa".to_string()),
                exp_month: Some("12".to_string()),
                exp_year: Some("2025".to_string()),
                cardholder_name: Some("Mock User".to_string()),
                token: None,
            },
            expires_at: None,
        })
    }

    async fn create_customer(&self, _email: &str, _name: &str) -> Result<String> {
        Ok(format!("mock_customer_{}", uuid::Uuid::new_v4()))
    }

    async fn tokenize_payment_method_for_customer(
        &self,
        customer_id: &str,
        payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken> {
        let token = self.tokenize_payment_method(payment_method_data).await?;
        let mut info = token.payment_method.clone();
        info.token = Some(token.token.clone());
        self.saved_payment_methods
            .lock()
            .unwrap()
            .entry(customer_id.to_string())
            .or_default()
            .push(info);
        Ok(token)
    }

    async fn get_saved_payment_methods(&self, customer_id: &str) -> Result<Vec<PaymentMethodInfo>> {
        Ok(self
            .saved_payment_methods
            .lock()
            .unwrap()
            .get(customer_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn delete_payment_method(&self, token: &str) -> Result<()> {
        for methods in self.saved_payment_methods.lock().unwrap().values_mut() {
            methods.retain(|method| method.token.as_deref() != Some(token));
        }
        Ok(())
    }
}

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_mock_gateway_create_payment() {
        let gateway = MockPaymentGateway::new();
        let request = CreatePaymentRequest {
            amount: Decimal::new(5000, 2),
            currency: "USD".to_string(),
            order_id: Uuid::new_v4(),
            customer_id: None,
            customer_email: "test@example.com".to_string(),
            payment_method: crate::payment::PaymentMethod::Card(crate::payment::CardDetails {
                number: "4242424242424242".to_string(),
                exp_month: 12,
                exp_year: 2025,
                cvc: "123".to_string(),
                name: "Test User".to_string(),
            }),
            billing_address: None,
            payment_consent_id: None,
            merchant_initiated: None,
            metadata: serde_json::json!({}),
        };

        let result = gateway.create_payment(request).await.unwrap();
        assert_eq!(result.amount, Decimal::new(5000, 2));
        assert_eq!(result.currency, "USD");
        assert_eq!(result.status, PaymentSessionStatus::Open);
    }

    #[tokio::test]
    async fn test_mock_gateway_confirm_payment() {
        let gateway = MockPaymentGateway::new();
        let result = gateway.confirm_payment("mock_pay_123").await.unwrap();

        assert_eq!(result.status, PaymentStatus::Succeeded);
        assert_eq!(result.gateway, "mock");
    }

    #[tokio::test]
    async fn test_mock_gateway_refund() {
        use crate::payment::PaymentGateway;
        let gateway = MockPaymentGateway::new();
        let result = PaymentGateway::refund_payment(
            &gateway,
            "mock_pay_123",
            Some(Decimal::new(2500, 2)),
            "customer_request",
        )
        .await
        .unwrap();

        assert_eq!(result.status, RefundStatus::Succeeded);
        assert_eq!(result.amount, Decimal::new(2500, 2));
    }
}
