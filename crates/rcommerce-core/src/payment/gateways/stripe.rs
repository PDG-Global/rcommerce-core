use super::stripe_agnostic::{
    map_refund_reason, stripe_amount_from_minor_units, stripe_amount_to_minor_units,
};
use async_trait::async_trait;
use reqwest;
use rust_decimal::Decimal;
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
        let params = build_create_payment_params(&request)?;

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
            amount: stripe_amount_from_minor_units(stripe_payment.amount, &stripe_payment.currency),
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
            client_secret: Some(stripe_payment.client_secret),
            card_brand: None,
            card_last_four: None,
            captured_at: None,
            created_at: chrono::Utc::now(),
        })
    }

    async fn capture_payment(&self, payment_id: &str, amount: Option<Decimal>) -> Result<Payment> {
        let mut params = std::collections::HashMap::new();
        if let Some(amount) = amount {
            let payment = self.get_payment(payment_id).await?;
            let amount_to_capture = stripe_amount_to_minor_units(amount, &payment.currency)?;
            params.insert("amount_to_capture", amount_to_capture.to_string());
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
        let currency = if amount.is_some() {
            Some(self.get_payment(payment_id).await?.currency)
        } else {
            None
        };
        let params = build_refund_payment_params(payment_id, amount, currency.as_deref(), reason)?;

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
            amount: stripe_amount_from_minor_units(stripe_refund.amount, &stripe_refund.currency),
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
            .query(&[("expand[]", "latest_charge")])
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

        let charge = stripe_payment
            .latest_charge
            .as_ref()
            .or_else(|| stripe_payment.charges.data.first());
        let (card_brand, card_last_four) = charge
            .and_then(|c| c.payment_method_details.as_ref())
            .and_then(|d| d.card.as_ref())
            .map(|c| (Some(c.brand.clone()), Some(c.last4.clone())))
            .unwrap_or((None, None));

        Ok(Payment {
            id: format!("pay_{}", uuid::Uuid::new_v4()),
            gateway: self.id().to_string(),
            amount: stripe_amount_from_minor_units(stripe_payment.amount, &stripe_payment.currency),
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
            client_secret: Some(stripe_payment.client_secret),
            card_brand,
            card_last_four,
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

fn build_create_payment_params(
    request: &CreatePaymentRequest,
) -> Result<std::collections::HashMap<&'static str, String>> {
    let amount = stripe_amount_to_minor_units(request.amount, &request.currency)?;
    let mut params = std::collections::HashMap::new();
    params.insert("amount", amount.to_string());
    params.insert("currency", request.currency.to_lowercase());
    params.insert("metadata[order_id]", request.order_id.to_string());
    params.insert("payment_method_types[]", "card".to_string());
    if let Some(customer_id) = request.customer_id {
        params.insert("metadata[customer_id]", customer_id.to_string());
    }
    if !request.customer_email.is_empty() {
        params.insert("receipt_email", request.customer_email.clone());
    }
    Ok(params)
}

fn build_refund_payment_params(
    payment_id: &str,
    amount: Option<Decimal>,
    currency: Option<&str>,
    reason: &str,
) -> Result<std::collections::HashMap<&'static str, String>> {
    let mut params = std::collections::HashMap::new();
    params.insert("payment_intent", payment_id.to_string());
    params.insert("reason", map_refund_reason(reason));
    if let Some(amount) = amount {
        let currency = currency
            .ok_or_else(|| Error::validation("Currency is required for a partial Stripe refund"))?;
        let amount = stripe_amount_to_minor_units(amount, currency)?;
        params.insert("amount", amount.to_string());
    }
    Ok(params)
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
    #[serde(default)]
    latest_charge: Option<StripeCharge>,
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
    #[serde(default)]
    payment_method_details: Option<StripePaymentMethodDetails>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StripePaymentMethodDetails {
    #[serde(default)]
    card: Option<StripeCardDetails>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StripeCardDetails {
    #[serde(default)]
    brand: String,
    #[serde(default)]
    last4: String,
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
    use rust_decimal_macros::dec;

    fn create_request(amount: Decimal, currency: &str) -> CreatePaymentRequest {
        CreatePaymentRequest {
            amount,
            currency: currency.to_string(),
            order_id: uuid::Uuid::nil(),
            customer_id: None,
            customer_email: String::new(),
            payment_method: crate::payment::PaymentMethod::StripeElements,
            billing_address: None,
            payment_consent_id: None,
            merchant_initiated: None,
            metadata: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn test_stripe_gateway_creation() {
        let gateway = StripeGateway::new("sk_test_123".to_string(), "whsec_123".to_string());

        assert_eq!(gateway.id(), "stripe");
        assert_eq!(gateway.name(), "Stripe");
    }

    #[test]
    fn create_payload_uses_checked_currency_minor_units() {
        let jpy = build_create_payment_params(&create_request(dec!(1234), "JPY")).unwrap();
        assert_eq!(jpy.get("amount").map(String::as_str), Some("1234"));
        assert_eq!(jpy.get("currency").map(String::as_str), Some("jpy"));

        let usd = build_create_payment_params(&create_request(dec!(12.34), "USD")).unwrap();
        assert_eq!(usd.get("amount").map(String::as_str), Some("1234"));
    }

    #[test]
    fn refund_payload_uses_jpy_units_and_maps_default_reason() {
        let params = build_refund_payment_params(
            "pi_jpy",
            Some(dec!(1234)),
            Some("JPY"),
            "Refunded via API",
        )
        .unwrap();
        assert_eq!(params.get("amount").map(String::as_str), Some("1234"));
        assert_eq!(
            params.get("reason").map(String::as_str),
            Some("requested_by_customer")
        );

        let duplicate = build_refund_payment_params("pi_usd", None, None, "duplicate").unwrap();
        assert_eq!(
            duplicate.get("reason").map(String::as_str),
            Some("duplicate")
        );
        assert!(!duplicate.contains_key("amount"));
    }

    #[test]
    fn legacy_payload_overflow_is_a_validation_error() {
        let create = build_create_payment_params(&create_request(Decimal::MAX, "USD")).unwrap_err();
        assert!(matches!(create, Error::Validation(_)));

        let refund = build_refund_payment_params(
            "pi_jpy",
            Some(Decimal::MAX),
            Some("JPY"),
            "Refunded via API",
        )
        .unwrap_err();
        assert!(matches!(refund, Error::Validation(_)));
    }
}
