//! Airwallex payment gateway implementation
//!
//! Airwallex is a global financial platform that offers multi-currency
//! payment processing with competitive FX rates.

use async_trait::async_trait;
use reqwest;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::payment::{
    CreatePaymentRequest, Payment, PaymentGateway, PaymentSession, PaymentSessionStatus,
    PaymentStatus, Refund, RefundStatus, WebhookEvent, WebhookEventType,
};
use crate::{Error, Result};

/// Airwallex API base URLs
const AIRWALLEX_API_BASE_PROD: &str = "https://api.airwallex.com/api/v1";
const AIRWALLEX_API_BASE_DEMO: &str = "https://api.sandbox.airwallex.com/api/v1";

/// Airwallex payment gateway
pub struct AirwallexGateway {
    client_id: String,
    api_key: String,
    webhook_secret: String,
    client: reqwest::Client,
    access_token: std::sync::Mutex<Option<AirwallexAccessToken>>,
    base_url: String,
}

/// Airwallex access token for authentication
#[derive(Debug, Clone)]
struct AirwallexAccessToken {
    token: String,
    expires_at: u64, // Unix timestamp
}

impl AirwallexGateway {
    /// Create a new Airwallex gateway
    pub fn new(client_id: String, api_key: String, webhook_secret: String) -> Self {
        // Check for demo environment via env var
        let base_url = if std::env::var("AIRWALLEX_USE_DEMO").unwrap_or_default() == "1" {
            AIRWALLEX_API_BASE_DEMO.to_string()
        } else {
            AIRWALLEX_API_BASE_PROD.to_string()
        };

        Self {
            client_id,
            api_key,
            webhook_secret,
            client: reqwest::Client::new(),
            access_token: std::sync::Mutex::new(None),
            base_url,
        }
    }

    /// Create with default (for factory registration)
    pub fn create_default() -> Self {
        Self::new(String::new(), String::new(), String::new())
    }

    /// Create a new Airwallex gateway with explicit demo mode
    pub fn with_demo(client_id: String, api_key: String, webhook_secret: String) -> Self {
        Self {
            client_id,
            api_key,
            webhook_secret,
            client: reqwest::Client::new(),
            access_token: std::sync::Mutex::new(None),
            base_url: AIRWALLEX_API_BASE_DEMO.to_string(),
        }
    }

    /// Get access token for API authentication
    async fn get_access_token(&self) -> Result<String> {
        // Check if we have a valid cached token
        {
            let cached = self.access_token.lock().unwrap();
            if let Some(token) = cached.as_ref() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                if token.expires_at > now + 60 {
                    // Token valid for at least 60 more seconds
                    return Ok(token.token.clone());
                }
            }
        }

        // Request new token - Airwallex requires an empty JSON body {}
        let response = self
            .client
            .post(format!("{}/authentication/login", self.base_url))
            .header("Content-Type", "application/json")
            .header("x-client-id", &self.client_id)
            .header("x-api-key", &self.api_key)
            .json(&serde_json::json!({})) // Empty JSON object as body
            .send()
            .await
            .map_err(|e| Error::network(format!("Airwallex auth error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Airwallex auth failed: {}",
                error_text
            )));
        }

        let auth_response: AirwallexAuthResponse = response.json().await.map_err(|e| {
            Error::Network(format!("Failed to parse Airwallex auth response: {}", e))
        })?;

        // Parse ISO 8601 expires_at to Unix timestamp
        let expires_at =
            chrono::DateTime::parse_from_str(&auth_response.expires_at, "%Y-%m-%dT%H:%M:%S%z")
                .map_err(|e| Error::validation(format!("Invalid expires_at format: {}", e)))?
                .timestamp() as u64;

        let token = AirwallexAccessToken {
            token: auth_response.token,
            expires_at,
        };

        // Cache the token
        {
            let mut cached = self.access_token.lock().unwrap();
            *cached = Some(token.clone());
        }

        Ok(token.token)
    }

    /// Map Airwallex payment status to our PaymentStatus
    fn map_status(status: &str) -> PaymentStatus {
        match status {
            "REQUIRES_ACTION" | "PENDING" => PaymentStatus::Pending,
            "PROCESSING" => PaymentStatus::Processing,
            "SUCCEEDED" | "CAPTURED" => PaymentStatus::Succeeded,
            "FAILED" | "CANCELLED" => PaymentStatus::Failed,
            "REFUNDED" | "PARTIALLY_REFUNDED" => PaymentStatus::Refunded,
            _ => PaymentStatus::Pending,
        }
    }

    /// Map Airwallex refund status
    fn map_refund_status(status: &str) -> RefundStatus {
        match status {
            "PENDING" => RefundStatus::Pending,
            "SUCCEEDED" => RefundStatus::Succeeded,
            "FAILED" => RefundStatus::Failed,
            _ => RefundStatus::Pending,
        }
    }

    /// Map an Airwallex webhook event name to our WebhookEventType
    fn map_event_type(event_name: &str) -> Result<WebhookEventType> {
        match event_name {
            "payment_intent.succeeded" => Ok(WebhookEventType::PaymentSucceeded),
            "payment_intent.failed" => Ok(WebhookEventType::PaymentFailed),
            "payment_intent.cancelled" => Ok(WebhookEventType::PaymentCanceled),
            "refund.succeeded" => Ok(WebhookEventType::RefundSucceeded),
            _ => Err(Error::validation("Unsupported webhook event type")),
        }
    }
}

#[async_trait]
impl PaymentGateway for AirwallexGateway {
    fn id(&self) -> &'static str {
        "airwallex"
    }

    fn name(&self) -> &'static str {
        "Airwallex"
    }

    async fn create_payment(&self, request: CreatePaymentRequest) -> Result<PaymentSession> {
        let token = self.get_access_token().await?;

        // Airwallex expects the amount in major currency units (e.g. 645.00 USD)
        // Generate merchant_order_id (required by Airwallex)
        let merchant_order_id = format!(
            "ORD-{}",
            request.order_id.to_string().split('-').next().unwrap_or("")
        );

        let mut metadata = serde_json::Map::new();
        // The store-prefixed reference (e.g. "88-1024") is the primary join
        // key in the Airwallex bank feed; fall back to the internal UUID only
        // if no reference was supplied (accounting spec 2.1).
        let order_reference = request
            .metadata
            .get("order_reference")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        metadata.insert(
            "order_id".to_string(),
            serde_json::Value::String(
                order_reference
                    .clone()
                    .unwrap_or_else(|| request.order_id.to_string()),
            ),
        );
        if let Some(site_url) = request.metadata.get("site_url").and_then(|v| v.as_str()) {
            metadata.insert(
                "site_url".to_string(),
                serde_json::Value::String(site_url.to_string()),
            );
        }
        if let Some(test) = request.metadata.get("test").and_then(|v| v.as_bool()) {
            metadata.insert("test".to_string(), serde_json::Value::Bool(test));
        }
        if let Some(customer_id) = request.customer_id {
            metadata.insert(
                "customer_id".to_string(),
                serde_json::Value::String(customer_id.to_string()),
            );
        }

        let payload = serde_json::json!({
            "request_id": uuid::Uuid::new_v4().to_string(),
            "amount": request.amount,
            "currency": request.currency,
            "descriptor": "R Commerce Payment", // Max 32 chars
            "merchant_order_id": merchant_order_id,
            "metadata": metadata,
        });

        let response = self
            .client
            .post(format!("{}/pa/payment_intents/create", self.base_url))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| Error::network(format!("Airwallex API error: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        if !status.is_success() {
            return Err(Error::payment_error(format!(
                "Airwallex error ({}): {}",
                status, body
            )));
        }

        let intent: AirwallexPaymentIntent = serde_json::from_str(&body).map_err(|e| {
            tracing::error!(
                "Failed to parse Airwallex intent response (HTTP {}): {} — body: {}",
                status,
                e,
                body
            );
            Error::Network(format!("Failed to parse Airwallex response: {}", e))
        })?;

        Ok(PaymentSession {
            id: intent.id,
            client_secret: intent.client_secret,
            status: match intent.status.as_str() {
                "REQUIRES_ACTION" => PaymentSessionStatus::Open,
                "SUCCEEDED" => PaymentSessionStatus::Complete,
                _ => PaymentSessionStatus::Open,
            },
            amount: request.amount,
            currency: request.currency,
            customer_id: request.customer_id,
        })
    }

    async fn confirm_payment(&self, payment_id: &str) -> Result<Payment> {
        let token = self.get_access_token().await?;

        // First, check the payment intent status
        let response = self
            .client
            .get(format!(
                "{}/pa/payment_intents/{}",
                self.base_url, payment_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Airwallex error: {}",
                error_text
            )));
        }

        let intent: AirwallexPaymentIntent = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Airwallex response: {}", e)))?;

        // If payment requires confirmation, confirm it
        if intent.status == "REQUIRES_ACTION" || intent.status == "PENDING" {
            let confirm_payload = serde_json::json!({
                "request_id": uuid::Uuid::new_v4().to_string(),
                "payment_method": {
                    "type": "card",
                },
            });

            let _confirm_response = self
                .client
                .post(format!(
                    "{}/pa/payment_intents/{}/confirm",
                    self.base_url, payment_id
                ))
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .json(&confirm_payload)
                .send()
                .await
                .map_err(|e| Error::network(format!("Airwallex confirm error: {}", e)))?;
        }

        // Get the latest payment from the intent
        let payment_response = self.get_payment(payment_id).await?;

        Ok(payment_response)
    }

    async fn capture_payment(&self, payment_id: &str, amount: Option<Decimal>) -> Result<Payment> {
        let token = self.get_access_token().await?;

        let mut payload = serde_json::json!({
            "request_id": uuid::Uuid::new_v4().to_string(),
        });

        if let Some(amt) = amount {
            payload["amount"] = serde_json::json!(amt);
        }

        let response = self
            .client
            .post(format!(
                "{}/pa/payment_intents/{}/capture",
                self.base_url, payment_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Airwallex error: {}",
                error_text
            )));
        }

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
        let token = self.get_access_token().await?;

        // Airwallex requires the refund currency to match the original payment
        let payment = self.get_payment(payment_id).await?;

        let mut payload = serde_json::json!({
            "request_id": uuid::Uuid::new_v4().to_string(),
            "payment_intent_id": payment_id,
            "reason": reason,
            "currency": payment.currency,
        });
        // Omitting `amount` refunds the payment in full
        if let Some(amount) = amount {
            payload["amount"] = serde_json::json!(amount);
        }

        let response = self
            .client
            .post(format!("{}/pa/refunds/create", self.base_url))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Airwallex error: {}",
                error_text
            )));
        }

        let refund: AirwallexRefund = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Airwallex refund: {}", e)))?;

        Ok(Refund {
            id: refund.id,
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(payment.amount),
            currency: refund.currency,
            status: Self::map_refund_status(&refund.status),
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn get_payment(&self, payment_id: &str) -> Result<Payment> {
        let token = self.get_access_token().await?;

        let response = self
            .client
            .get(format!(
                "{}/pa/payment_intents/{}",
                self.base_url, payment_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "Airwallex error: {}",
                error_text
            )));
        }

        let intent: AirwallexPaymentIntent = response
            .json()
            .await
            .map_err(|e| Error::Network(format!("Failed to parse Airwallex response: {}", e)))?;

        let amount = intent.amount;

        Ok(Payment {
            id: format!("pay_{}", uuid::Uuid::new_v4()),
            gateway: self.id().to_string(),
            amount,
            currency: intent.currency,
            status: Self::map_status(&intent.status),
            order_id: intent
                .metadata
                .get("order_id")
                .and_then(|v| v.as_str())
                .and_then(|id| uuid::Uuid::parse_str(id).ok())
                .unwrap_or_else(uuid::Uuid::nil),
            customer_id: intent
                .metadata
                .get("customer_id")
                .and_then(|v| v.as_str())
                .and_then(|id| uuid::Uuid::parse_str(id).ok()),
            payment_method: "card".to_string(),
            transaction_id: intent.id,
            client_secret: Some(intent.client_secret),
            card_brand: None,
            card_last_four: None,
            captured_at: if intent.status == "CAPTURED" || intent.status == "SUCCEEDED" {
                Some(chrono::Utc::now())
            } else {
                None
            },
            created_at: chrono::Utc::now(),
        })
    }

    async fn handle_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookEvent> {
        // Airwallex signs each webhook as HMAC-SHA256(secret, x-timestamp + raw_body)
        // and sends the hex digest in the `x-signature` header; the send time (Unix
        // ms) arrives in the `x-timestamp` header. This legacy trait has no header
        // map, so callers must pass the composite "<x-timestamp>.<x-signature>"
        // string as `signature`.
        let (timestamp, signature) = signature.split_once('.').ok_or_else(|| {
            Error::validation(
                "Invalid webhook signature format; expected \"<timestamp>.<signature>\"",
            )
        })?;

        // Verify webhook signature using HMAC-SHA256
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(self.webhook_secret.as_bytes())
            .map_err(|e| Error::validation(format!("Invalid webhook secret: {}", e)))?;
        mac.update(timestamp.as_bytes());
        mac.update(payload);
        let result = mac.finalize();
        let expected_signature = hex::encode(result.into_bytes());

        if signature != expected_signature {
            return Err(Error::validation("Invalid webhook signature"));
        }

        let event: AirwallexWebhookEvent = serde_json::from_slice(payload)
            .map_err(|e| Error::validation(format!("Invalid webhook payload: {}", e)))?;

        let payment_id = event.data.object.id.clone();
        let event_type = Self::map_event_type(&event.event_type)?;

        Ok(WebhookEvent {
            event_type,
            payment_id,
            data: serde_json::from_value(serde_json::json!(event.data.object))
                .map_err(|e| Error::Network(format!("Failed to parse webhook data: {}", e)))?,
        })
    }
}

// Airwallex API response types

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexAuthResponse {
    token: String,
    #[serde(rename = "expires_at")]
    expires_at: String, // ISO 8601 format: "2026-01-28T11:50:18+0000"
}

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexPaymentIntent {
    id: String,
    #[serde(rename = "client_secret")]
    client_secret: String,
    status: String,
    #[serde(with = "rust_decimal::serde::float")]
    amount: Decimal,
    currency: String,
    // Airwallex echoes metadata back verbatim — keep this tolerant of any
    // JSON value (e.g. nulls) since the echoed content is never read.
    #[serde(default)]
    metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexRefund {
    id: String,
    status: String,
    currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexWebhookEvent {
    /// Airwallex puts the event type in the `name` field; `type` is accepted
    /// as an alias for compatibility with older payloads.
    #[serde(rename = "name", alias = "type")]
    event_type: String,
    data: AirwallexWebhookData,
}

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexWebhookData {
    object: AirwallexWebhookObject,
}

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexWebhookObject {
    id: String,
    #[serde(rename = "payment_intent_id")]
    payment_intent_id: Option<String>,
    status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_airwallex_gateway_creation() {
        let gateway = AirwallexGateway::new(
            "test_client_id".to_string(),
            "test_api_key".to_string(),
            "test_webhook_secret".to_string(),
        );

        assert_eq!(gateway.id(), "airwallex");
        assert_eq!(gateway.name(), "Airwallex");
    }

    #[test]
    fn test_map_status() {
        use crate::payment::PaymentStatus;
        assert_eq!(
            AirwallexGateway::map_status("REQUIRES_ACTION"),
            PaymentStatus::Pending
        );
        assert_eq!(
            AirwallexGateway::map_status("PENDING"),
            PaymentStatus::Pending
        );
        assert_eq!(
            AirwallexGateway::map_status("PROCESSING"),
            PaymentStatus::Processing
        );
        assert_eq!(
            AirwallexGateway::map_status("SUCCEEDED"),
            PaymentStatus::Succeeded
        );
        assert_eq!(
            AirwallexGateway::map_status("CAPTURED"),
            PaymentStatus::Succeeded
        );
        assert_eq!(
            AirwallexGateway::map_status("FAILED"),
            PaymentStatus::Failed
        );
        assert_eq!(
            AirwallexGateway::map_status("CANCELLED"),
            PaymentStatus::Failed
        );
        assert_eq!(
            AirwallexGateway::map_status("REFUNDED"),
            PaymentStatus::Refunded
        );
    }

    /// Compute hex(HMAC-SHA256(secret, timestamp + raw_body)), i.e. the value
    /// Airwallex sends in the `x-signature` header.
    fn airwallex_signature(secret: &str, timestamp: &str, body: &[u8]) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(timestamp.as_bytes());
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    /// The legacy trait has no header map, so callers pass
    /// "<x-timestamp>.<x-signature>" as the signature argument.
    fn composite_signature(secret: &str, timestamp: &str, body: &[u8]) -> String {
        format!(
            "{}.{}",
            timestamp,
            airwallex_signature(secret, timestamp, body)
        )
    }

    fn test_gateway() -> AirwallexGateway {
        AirwallexGateway::new(
            "test_client_id".to_string(),
            "test_api_key".to_string(),
            "test_webhook_secret".to_string(),
        )
    }

    #[test]
    fn test_webhook_event_type_mapping() {
        use crate::payment::WebhookEventType;

        assert_eq!(
            AirwallexGateway::map_event_type("payment_intent.succeeded").unwrap(),
            WebhookEventType::PaymentSucceeded
        );
        assert_eq!(
            AirwallexGateway::map_event_type("payment_intent.failed").unwrap(),
            WebhookEventType::PaymentFailed
        );
        assert_eq!(
            AirwallexGateway::map_event_type("payment_intent.cancelled").unwrap(),
            WebhookEventType::PaymentCanceled
        );
        assert_eq!(
            AirwallexGateway::map_event_type("refund.succeeded").unwrap(),
            WebhookEventType::RefundSucceeded
        );
        assert!(AirwallexGateway::map_event_type("payment_consent.verified").is_err());
    }

    #[tokio::test]
    async fn test_webhook_verifies_timestamp_body_scheme() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "id": "evt_123",
            "name": "payment_intent.succeeded",
            "data": { "object": { "id": "int_xxx", "status": "SUCCEEDED" } }
        })
        .to_string();
        let signature = composite_signature("test_webhook_secret", timestamp, body.as_bytes());

        let event = gateway
            .handle_webhook(body.as_bytes(), &signature)
            .await
            .expect("webhook should verify");

        assert_eq!(
            event.event_type,
            crate::payment::WebhookEventType::PaymentSucceeded
        );
        assert_eq!(event.payment_id, "int_xxx");
    }

    #[tokio::test]
    async fn test_webhook_rejects_tampered_payload() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "name": "payment_intent.succeeded",
            "data": { "object": { "id": "int_xxx", "status": "SUCCEEDED" } }
        })
        .to_string();
        let signature = composite_signature("test_webhook_secret", timestamp, body.as_bytes());

        let tampered = body.replace("int_xxx", "int_evil");
        let result = gateway
            .handle_webhook(tampered.as_bytes(), &signature)
            .await;
        assert!(result.is_err(), "tampered body must fail verification");
    }

    #[tokio::test]
    async fn test_webhook_rejects_bare_signature() {
        // A bare HMAC hex without the timestamp prefix is not the Airwallex
        // scheme and must be rejected.
        let gateway = test_gateway();
        let body = serde_json::json!({
            "name": "payment_intent.succeeded",
            "data": { "object": { "id": "int_xxx", "status": "SUCCEEDED" } }
        })
        .to_string();
        let bare = airwallex_signature("test_webhook_secret", "1712345678000", body.as_bytes());

        let result = gateway.handle_webhook(body.as_bytes(), &bare).await;
        assert!(
            result.is_err(),
            "bare signature without timestamp must fail"
        );
    }

    #[tokio::test]
    async fn test_webhook_refund_succeeded() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "name": "refund.succeeded",
            "data": {
                "object": {
                    "id": "ref_zzz",
                    "payment_intent_id": "int_xxx",
                    "status": "SUCCEEDED"
                }
            }
        })
        .to_string();
        let signature = composite_signature("test_webhook_secret", timestamp, body.as_bytes());

        let event = gateway
            .handle_webhook(body.as_bytes(), &signature)
            .await
            .expect("refund webhook should verify");

        assert_eq!(
            event.event_type,
            crate::payment::WebhookEventType::RefundSucceeded
        );
        assert_eq!(event.payment_id, "ref_zzz");
    }
}
