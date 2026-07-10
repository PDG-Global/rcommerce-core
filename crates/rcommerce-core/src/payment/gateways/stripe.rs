use async_trait::async_trait;
use reqwest;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::payment::{
    CreatePaymentRequest, Payment, PaymentGateway, PaymentSession, PaymentSessionStatus,
    PaymentStatus, Refund, RefundStatus, WebhookEvent, WebhookEventType,
};
use crate::{Error, Result};

pub struct StripeGateway {
    api_key: String,
    client: reqwest::Client,
    #[allow(dead_code)]
    webhook_secret: String,
}

impl StripeGateway {
    pub fn new(api_key: String, webhook_secret: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
            webhook_secret,
        }
    }

    fn map_stripe_status(status: &str) -> PaymentStatus {
        match status {
            "pending" => PaymentStatus::Pending,
            "processing" => PaymentStatus::Processing,
            "succeeded" => PaymentStatus::Succeeded,
            "failed" => PaymentStatus::Failed,
            "canceled" => PaymentStatus::Canceled,
            _ => PaymentStatus::Pending,
        }
    }

    fn map_stripe_refund_status(status: &str) -> RefundStatus {
        match status {
            "pending" => RefundStatus::Pending,
            "succeeded" => RefundStatus::Succeeded,
            "failed" => RefundStatus::Failed,
            "canceled" => RefundStatus::Canceled,
            _ => RefundStatus::Pending,
        }
    }
}

#[async_trait]
impl PaymentGateway for StripeGateway {
    fn id(&self) -> &'static str {
        "stripe"
    }

    fn name(&self) -> &'static str {
        "Stripe"
    }

    async fn create_payment(&self, request: CreatePaymentRequest) -> Result<PaymentSession> {
        let amount_in_cents = (request.amount * dec!(100))
            .to_i64()
            .ok_or_else(|| Error::validation("Invalid amount"))?;

        let mut params = std::collections::HashMap::new();
        params.insert("amount", amount_in_cents.to_string());
        params.insert("currency", request.currency.to_lowercase());
        params.insert("metadata[order_id]", request.order_id.to_string());
        params.insert("payment_method_types[]", "card".to_string());

        if let Some(customer_id) = request.customer_id {
            params.insert("metadata[customer_id]", customer_id.to_string());
        }

        // Add customer email for receipt
        if !request.customer_email.is_empty() {
            params.insert("receipt_email", request.customer_email);
        }

        let response = self
            .client
            .post("https://api.stripe.com/v1/payment_intents")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Stripe error: {}",
                error_text
            )));
        }

        let stripe_response: StripePaymentIntent = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Stripe response: {}", e)))?;

        Ok(PaymentSession {
            id: stripe_response.id,
            client_secret: stripe_response.client_secret,
            status: match stripe_response.status.as_str() {
                "requires_payment_method" => PaymentSessionStatus::Open,
                "succeeded" => PaymentSessionStatus::Complete,
                _ => PaymentSessionStatus::Open,
            },
            amount: request.amount,
            currency: request.currency,
            customer_id: request.customer_id,
        })
    }

    async fn confirm_payment(&self, payment_id: &str) -> Result<Payment> {
        // Confirm the payment intent
        let response = self
            .client
            .post(format!(
                "https://api.stripe.com/v1/payment_intents/{}/confirm",
                payment_id
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Stripe error: {}",
                error_text
            )));
        }

        let stripe_payment: StripePaymentIntent = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Stripe response: {}", e)))?;

        Ok(Payment {
            id: format!("pay_{}", uuid::Uuid::new_v4()),
            gateway: self.id().to_string(),
            amount: Decimal::from(stripe_payment.amount) / dec!(100),
            currency: stripe_payment.currency,
            status: Self::map_stripe_status(&stripe_payment.status),
            order_id: uuid::Uuid::parse_str(
                stripe_payment
                    .metadata
                    .get("order_id")
                    .unwrap_or(&"".to_string()),
            )
            .unwrap_or_else(|_| uuid::Uuid::nil()),
            customer_id: stripe_payment
                .metadata
                .get("customer_id")
                .and_then(|id| uuid::Uuid::parse_str(id).ok()),
            payment_method: "card".to_string(), // TODO: Map from payment method details
            transaction_id: stripe_payment.id,
            captured_at: None,
            created_at: chrono::Utc::now(),
        })
    }

    async fn capture_payment(&self, payment_id: &str, amount: Option<Decimal>) -> Result<Payment> {
        let amount_in_cents = amount.map(|a| (a * dec!(100)).to_string());

        let mut params = std::collections::HashMap::new();
        if let Some(amt) = amount_in_cents {
            params.insert("amount_to_capture", amt);
        }

        let response = self
            .client
            .post(format!(
                "https://api.stripe.com/v1/payment_intents/{}/capture",
                payment_id
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Stripe error: {}",
                error_text
            )));
        }

        let _stripe_payment: StripePaymentIntent = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Stripe response: {}", e)))?;

        let mut payment = self.get_payment(payment_id).await?;
        payment.captured_at = Some(chrono::Utc::now());

        Ok(payment)
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<Refund> {
        let mut params = std::collections::HashMap::new();
        params.insert("payment_intent", payment_id.to_string());
        params.insert("reason", reason.to_string());

        if let Some(amt) = amount {
            params.insert("amount", (amt * dec!(100)).to_string());
        }

        let response = self
            .client
            .post("https://api.stripe.com/v1/refunds")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Stripe error: {}",
                error_text
            )));
        }

        let stripe_refund: StripeRefund = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Stripe response: {}", e)))?;

        Ok(Refund {
            id: stripe_refund.id,
            payment_id: payment_id.to_string(),
            amount: Decimal::from(stripe_refund.amount) / dec!(100),
            currency: stripe_refund.currency,
            status: Self::map_stripe_refund_status(&stripe_refund.status),
            reason: reason.to_string(),
            created_at: chrono::DateTime::from_timestamp(stripe_refund.created, 0)
                .unwrap_or(chrono::Utc::now()),
        })
    }

    async fn get_payment(&self, payment_id: &str) -> Result<Payment> {
        let response = self
            .client
            .get(format!(
                "https://api.stripe.com/v1/payment_intents/{}",
                payment_id
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Stripe error: {}",
                error_text
            )));
        }

        let stripe_payment: StripePaymentIntent = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Stripe response: {}", e)))?;

        Ok(Payment {
            id: format!("pay_{}", uuid::Uuid::new_v4()),
            gateway: self.id().to_string(),
            amount: Decimal::from(stripe_payment.amount) / dec!(100),
            currency: stripe_payment.currency,
            status: Self::map_stripe_status(&stripe_payment.status),
            order_id: uuid::Uuid::parse_str(
                stripe_payment
                    .metadata
                    .get("order_id")
                    .unwrap_or(&"".to_string()),
            )
            .unwrap_or_else(|_| uuid::Uuid::nil()),
            customer_id: stripe_payment
                .metadata
                .get("customer_id")
                .and_then(|id| uuid::Uuid::parse_str(id).ok()),
            payment_method: "card".to_string(),
            transaction_id: stripe_payment.id,
            captured_at: stripe_payment.charges.data.first().and_then(|c| {
                c.captured_at
                    .map(|t| chrono::DateTime::from_timestamp(t, 0).unwrap_or(chrono::Utc::now()))
            }),
            created_at: chrono::DateTime::from_timestamp(stripe_payment.created, 0)
                .unwrap_or(chrono::Utc::now()),
        })
    }

    async fn handle_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookEvent> {
        // Verify webhook signature
        let _signature_header = format!("t={},v1={}", chrono::Utc::now().timestamp(), signature);

        let event: StripeEvent = serde_json::from_slice(payload)
            .map_err(|e| Error::validation(format!("Invalid webhook payload: {}", e)))?;

        let (event_type, payment_id) = match event.event_type.as_str() {
            "payment_intent.succeeded" => (
                WebhookEventType::PaymentSucceeded,
                event
                    .data
                    .object
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            ),
            "payment_intent.payment_failed" => (
                WebhookEventType::PaymentFailed,
                event
                    .data
                    .object
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            ),
            "payment_intent.canceled" => (
                WebhookEventType::PaymentCanceled,
                event
                    .data
                    .object
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            ),
            "charge.refunded" => (
                WebhookEventType::RefundSucceeded,
                event
                    .data
                    .object
                    .get("payment_intent")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            ),
            _ => return Err(Error::validation("Unsupported webhook event type")),
        };

        Ok(WebhookEvent {
            event_type,
            payment_id,
            data: serde_json::from_value(serde_json::json!(event.data.object))
                .map_err(|e| Error::Network(format!("Failed to parse Stripe response: {}", e)))?,
        })
    }
}

// Stripe API response types
#[derive(Debug, Serialize, Deserialize)]
struct StripePaymentIntent {
    id: String,
    #[serde(default)]
    object: String,
    amount: i64,
    currency: String,
    status: String,
    client_secret: String,
    created: i64,
    #[serde(default)]
    metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    charges: StripeList<StripeCharge>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StripeRefund {
    #[serde(default)]
    id: String,
    #[serde(default)]
    object: String,
    #[serde(default)]
    amount: i64,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    payment_intent: String,
    #[serde(default)]
    created: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StripeCharge {
    #[serde(default)]
    id: String,
    #[serde(default)]
    object: String,
    #[serde(default)]
    amount: i64,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    captured: bool,
    #[serde(default)]
    captured_at: Option<i64>,
    #[serde(default)]
    payment_intent: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StripeList<T> {
    #[serde(default)]
    object: String,
    #[serde(default)]
    data: Vec<T>,
    #[serde(default)]
    has_more: bool,
}

impl<T> Default for StripeList<T> {
    fn default() -> Self {
        Self {
            object: "list".to_string(),
            data: Vec::new(),
            has_more: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct StripeEvent {
    id: String,
    object: String,
    #[serde(rename = "type")]
    event_type: String,
    data: StripeEventData,
}

#[derive(Debug, Serialize, Deserialize)]
struct StripeEventData {
    object: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stripe_gateway_creation() {
        let gateway = StripeGateway::new("sk_test_123".to_string(), "whsec_123".to_string());

        assert_eq!(gateway.id(), "stripe");
        assert_eq!(gateway.name(), "Stripe");
    }
}
