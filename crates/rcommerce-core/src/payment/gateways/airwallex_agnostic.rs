//! Airwallex Payment Gateway - Agnostic Implementation
//!
//! Server-to-server implementation that handles Airwallex payments securely.

use async_trait::async_trait;
use reqwest;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::payment::agnostic::*;
use crate::Result;

const AIRWALLEX_API_BASE_PROD: &str = "https://api.airwallex.com/api/v1";
const AIRWALLEX_API_BASE_DEMO: &str = "https://api.sandbox.airwallex.com/api/v1";

/// Airwallex Agnostic Gateway
pub struct AirwallexAgnosticGateway {
    client_id: String,
    api_key: String,
    webhook_secret: String,
    client: reqwest::Client,
    access_token: std::sync::Mutex<Option<AirwallexAccessToken>>,
    base_url: String,
    supported_methods: Vec<PaymentMethodConfig>,
}

/// Airwallex access token for authentication
#[derive(Debug, Clone)]
struct AirwallexAccessToken {
    token: String,
    expires_at: u64,
}

impl AirwallexAgnosticGateway {
    /// Create a new Airwallex agnostic gateway
    pub fn new(client_id: String, api_key: String, webhook_secret: String, demo: bool) -> Self {
        let base_url = if demo {
            AIRWALLEX_API_BASE_DEMO.to_string()
        } else {
            AIRWALLEX_API_BASE_PROD.to_string()
        };

        // Define supported payment methods
        let supported_methods = vec![
            PaymentMethodConfig {
                method_type: PaymentMethodType::Card,
                enabled: true,
                display_name: "Credit/Debit Card".to_string(),
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
                supported_currencies: vec![],
                min_amount: Some(dec!(0.50)),
                max_amount: None,
            },
            PaymentMethodConfig {
                method_type: PaymentMethodType::GooglePay,
                enabled: true,
                display_name: "Google Pay".to_string(),
                requires_redirect: false,
                supports_3ds: true,
                supports_tokenization: true,
                supports_recurring: true,
                required_fields: vec![],
                optional_fields: vec![],
                supported_currencies: vec![],
                min_amount: None,
                max_amount: None,
            },
            PaymentMethodConfig {
                method_type: PaymentMethodType::ApplePay,
                enabled: true,
                display_name: "Apple Pay".to_string(),
                requires_redirect: false,
                supports_3ds: true,
                supports_tokenization: true,
                supports_recurring: true,
                required_fields: vec![],
                optional_fields: vec![],
                supported_currencies: vec![],
                min_amount: None,
                max_amount: None,
            },
        ];

        Self {
            client_id,
            api_key,
            webhook_secret,
            client: reqwest::Client::new(),
            access_token: std::sync::Mutex::new(None),
            base_url,
            supported_methods,
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
                    return Ok(token.token.clone());
                }
            }
        }

        // Request new token
        let response = self
            .client
            .post(format!("{}/authentication/login", self.base_url))
            .header("Content-Type", "application/json")
            .header("x-client-id", &self.client_id)
            .header("x-api-key", &self.api_key)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Airwallex auth error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "Airwallex auth failed: {}",
                error_text
            )));
        }

        let auth_response: AirwallexAuthResponse = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse Airwallex auth response: {}", e))
        })?;

        // Parse ISO 8601 expires_at to Unix timestamp
        let expires_at =
            chrono::DateTime::parse_from_str(&auth_response.expires_at, "%Y-%m-%dT%H:%M:%S%z")
                .map_err(|e| crate::Error::validation(format!("Invalid expires_at format: {}", e)))?
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

    /// Create a payment intent
    async fn create_payment_intent(
        &self,
        amount: Decimal,
        currency: &str,
        order_id: uuid::Uuid,
        customer_id: Option<uuid::Uuid>,
    ) -> Result<AirwallexPaymentIntent> {
        let token = self.get_access_token().await?;

        let amount_in_cents = (amount * dec!(100))
            .to_i64()
            .ok_or_else(|| crate::Error::validation("Invalid amount"))?;

        let merchant_order_id = format!(
            "ORD-{}",
            order_id.to_string().split('-').next().unwrap_or("")
        );

        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "order_id".to_string(),
            serde_json::Value::String(order_id.to_string()),
        );
        if let Some(customer_id) = customer_id {
            metadata.insert(
                "customer_id".to_string(),
                serde_json::Value::String(customer_id.to_string()),
            );
        }

        let payload = serde_json::json!({
            "request_id": uuid::Uuid::new_v4().to_string(),
            "amount": amount_in_cents,
            "currency": currency,
            "descriptor": "R Commerce Payment",
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
            .map_err(|e| crate::Error::network(format!("Airwallex API error: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        if !status.is_success() {
            return Err(crate::Error::payment_error(format!(
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
            crate::Error::network(format!("Failed to parse Airwallex response: {}", e))
        })?;

        Ok(intent)
    }

    /// Confirm a payment intent with card details
    async fn confirm_payment_intent_with_card(
        &self,
        intent_id: &str,
        card_data: &CardData,
    ) -> Result<AirwallexPaymentIntent> {
        let token = self.get_access_token().await?;

        let confirm_payload = serde_json::json!({
            "request_id": uuid::Uuid::new_v4().to_string(),
            "payment_method": {
                "type": "card",
                "card": {
                    "number": card_data.number,
                    "expiry_month": card_data.exp_month,
                    "expiry_year": card_data.exp_year,
                    "cvc": card_data.cvc,
                    "name": card_data.name,
                },
            },
        });

        let response = self
            .client
            .post(format!(
                "{}/pa/payment_intents/{}/confirm",
                self.base_url, intent_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&confirm_payload)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Airwallex confirm error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "Airwallex confirm error: {}",
                error_text
            )));
        }

        let intent: AirwallexPaymentIntent = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse Airwallex response: {}", e))
        })?;

        Ok(intent)
    }

    /// Get payment intent details
    async fn get_payment_intent(&self, intent_id: &str) -> Result<AirwallexPaymentIntent> {
        let token = self.get_access_token().await?;

        let response = self
            .client
            .get(format!(
                "{}/pa/payment_intents/{}",
                self.base_url, intent_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "Airwallex error: {}",
                error_text
            )));
        }

        let intent: AirwallexPaymentIntent = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse Airwallex response: {}", e))
        })?;

        Ok(intent)
    }
}

#[async_trait]
impl AgnosticPaymentGateway for AirwallexAgnosticGateway {
    async fn get_config(&self) -> Result<GatewayConfig> {
        Ok(GatewayConfig {
            gateway_id: "airwallex".to_string(),
            gateway_name: "Airwallex".to_string(),
            payment_methods: self.supported_methods.clone(),
            supports_3ds: true,
            supports_webhooks: true,
            supports_refunds: true,
            supports_partial_refunds: true,
            supported_currencies: vec![
                "USD".to_string(),
                "EUR".to_string(),
                "GBP".to_string(),
                "AUD".to_string(),
                "CAD".to_string(),
                "CNY".to_string(),
                "HKD".to_string(),
                "SGD".to_string(),
                "JPY".to_string(),
            ],
            default_currency: "USD".to_string(),
        })
    }

    async fn initiate_payment(
        &self,
        request: InitiatePaymentRequest,
    ) -> Result<InitiatePaymentResponse> {
        // Create payment intent
        let intent = self
            .create_payment_intent(
                request.amount,
                &request.currency,
                request.order_id,
                request.customer_id,
            )
            .await?;

        // Extract card data if provided
        let card_data = match &request.payment_method_data {
            PaymentMethodData::Card {
                number,
                exp_month,
                exp_year,
                cvc,
                name,
            } => Some(CardData {
                number: number.clone(),
                exp_month: exp_month.clone(),
                exp_year: exp_year.clone(),
                cvc: cvc.clone(),
                name: name.clone(),
            }),
            PaymentMethodData::CardToken { token } => {
                // Use saved card token
                let token = token.clone();
                let confirmed_intent = self
                    .confirm_payment_intent_with_token(&intent.id, &token)
                    .await?;
                return self.handle_intent_response(confirmed_intent, &request.currency);
            }
            _ => None,
        };

        // If card data provided, confirm immediately
        if let Some(card) = card_data {
            let confirmed_intent = self
                .confirm_payment_intent_with_card(&intent.id, &card)
                .await?;
            return self.handle_intent_response(confirmed_intent, &request.currency);
        }

        // Otherwise, return requires action for client-side confirmation
        Ok(InitiatePaymentResponse::RequiresAction {
            payment_id: intent.id,
            action_type: PaymentActionType::Verification,
            action_data: serde_json::json!({
                "client_secret": intent.client_secret,
            }),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
        })
    }

    async fn complete_payment_action(
        &self,
        request: CompletePaymentActionRequest,
    ) -> Result<CompletePaymentActionResponse> {
        // Get the current intent status
        let intent = self.get_payment_intent(&request.payment_id).await?;

        self.handle_complete_action_response(intent, &request)
    }

    async fn get_payment_status(&self, payment_id: &str) -> Result<PaymentStatus> {
        let intent = self.get_payment_intent(payment_id).await?;
        Ok(Self::map_status(&intent.status))
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<RefundResponse> {
        let token = self.get_access_token().await?;

        let amount_in_cents = match amount {
            Some(a) => Some(
                (a * dec!(100))
                    .to_i64()
                    .ok_or_else(|| crate::Error::validation("Invalid amount"))?,
            ),
            None => None,
        };

        let payload = serde_json::json!({
            "request_id": uuid::Uuid::new_v4().to_string(),
            "payment_intent_id": payment_id,
            "reason": reason,
            "amount": amount_in_cents,
        });

        let response = self
            .client
            .post(format!("{}/pa/refunds", self.base_url))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "Airwallex error: {}",
                error_text
            )));
        }

        let refund: AirwallexRefund = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse Airwallex refund: {}", e))
        })?;

        Ok(RefundResponse {
            refund_id: refund.id,
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(Decimal::ZERO),
            currency: refund.currency,
            status: Self::map_refund_status(&refund.status),
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &[(String, String)],
    ) -> Result<WebhookEvent> {
        // Extract signature from headers.
        //
        // Airwallex signs each webhook as HMAC-SHA256(secret, x-timestamp + raw_body)
        // and sends the hex digest in the `x-signature` header; the send time (Unix
        // ms) arrives in `x-timestamp`. See
        // https://www.airwallex.com/docs/developer-tools/webhooks
        let signature = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-signature"))
            .map(|(_, v)| v.as_str());

        let timestamp = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-timestamp"))
            .map(|(_, v)| v.as_str());

        let (Some(signature), Some(timestamp)) = (signature, timestamp) else {
            return Err(crate::Error::validation(
                "Missing x-signature or x-timestamp webhook header",
            ));
        };

        // Verify webhook signature using HMAC-SHA256
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        type HmacSha256 = Hmac<Sha256>;

        // Signed message is the timestamp concatenated with the raw body
        // (timestamp first, no separator).
        let mut mac = HmacSha256::new_from_slice(self.webhook_secret.as_bytes())
            .map_err(|e| crate::Error::validation(format!("Invalid webhook secret: {}", e)))?;
        mac.update(timestamp.as_bytes());
        mac.update(payload);
        let result = mac.finalize();
        let expected_signature = hex::encode(result.into_bytes());

        if signature != expected_signature {
            return Err(crate::Error::validation("Invalid webhook signature"));
        }

        let event: AirwallexWebhookEvent = serde_json::from_slice(payload)
            .map_err(|e| crate::Error::validation(format!("Invalid webhook payload: {}", e)))?;

        let payment_id = event.data.object.id.clone();
        let event_type = match event.event_type.as_str() {
            "payment_intent.succeeded" => WebhookEventType::PaymentSucceeded,
            "payment_intent.failed" => WebhookEventType::PaymentFailed,
            "payment_intent.cancelled" => WebhookEventType::PaymentCancelled,
            "payment_intent.payment_attempt_failed" => WebhookEventType::PaymentFailed,
            "refund.succeeded" => WebhookEventType::PaymentRefunded,
            "refund.failed" => WebhookEventType::PaymentFailed,
            // Stored-card consent verified. Log-only on our side: consent
            // status is verified synchronously at checkout.
            "payment_consent.verified" => WebhookEventType::PaymentConsentVerified,
            _ => return Err(crate::Error::validation("Unsupported webhook event type")),
        };

        Ok(WebhookEvent {
            event_type,
            payment_id,
            transaction_id: event.data.object.payment_intent_id.clone(),
            data: serde_json::json!(event.data.object),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn tokenize_payment_method(
        &self,
        payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken> {
        let token = self.get_access_token().await?;

        let card_data = match payment_method_data {
            PaymentMethodData::Card {
                number,
                exp_month,
                exp_year,
                cvc,
                name,
            } => {
                serde_json::json!({
                    "type": "card",
                    "card": {
                        "number": number,
                        "expiry_month": exp_month,
                        "expiry_year": exp_year,
                        "cvc": cvc,
                        "name": name,
                    },
                })
            }
            _ => {
                return Err(crate::Error::validation(
                    "Only card tokenization is supported",
                ))
            }
        };

        let payload = serde_json::json!({
            "request_id": uuid::Uuid::new_v4().to_string(),
            "payment_method": card_data,
        });

        let response = self
            .client
            .post(format!("{}/pa/payment_methods", self.base_url))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "Airwallex error: {}",
                error_text
            )));
        }

        let pm: AirwallexPaymentMethod = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse Airwallex response: {}", e))
        })?;

        Ok(PaymentMethodToken {
            token: pm.id,
            payment_method: PaymentMethodInfo {
                method_type: PaymentMethodType::Card,
                last_four: pm.card.as_ref().map(|c| c.number_last_four.clone()),
                card_brand: pm.card.as_ref().map(|c| c.brand.clone()),
                exp_month: pm.card.as_ref().map(|c| c.expiry_month.clone()),
                exp_year: pm.card.as_ref().map(|c| c.expiry_year.clone()),
                cardholder_name: pm.card.as_ref().and_then(|c| c.name.clone()),
                token: None,
            },
            expires_at: None,
        })
    }

    async fn get_saved_payment_methods(&self, customer_id: &str) -> Result<Vec<PaymentMethodInfo>> {
        let token = self.get_access_token().await?;

        let response = self
            .client
            .get(format!("{}/pa/payment_methods", self.base_url))
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("customer_id", customer_id)])
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::payment_error(
                "Failed to get saved payment methods",
            ));
        }

        let list: AirwallexPaymentMethodList = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

        Ok(list
            .items
            .into_iter()
            .map(|pm| PaymentMethodInfo {
                method_type: PaymentMethodType::Card,
                last_four: pm.card.as_ref().map(|c| c.number_last_four.clone()),
                card_brand: pm.card.as_ref().map(|c| c.brand.clone()),
                exp_month: pm.card.as_ref().map(|c| c.expiry_month.clone()),
                exp_year: pm.card.as_ref().map(|c| c.expiry_year.clone()),
                cardholder_name: pm.card.as_ref().and_then(|c| c.name.clone()),
                token: Some(pm.id),
            })
            .collect())
    }

    async fn delete_payment_method(&self, token_str: &str) -> Result<()> {
        let token = self.get_access_token().await?;

        let response = self
            .client
            .delete(format!(
                "{}/pa/payment_methods/{}",
                self.base_url, token_str
            ))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Airwallex API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::payment_error(
                "Failed to delete payment method",
            ));
        }

        Ok(())
    }
}

// Helper methods
impl AirwallexAgnosticGateway {
    async fn confirm_payment_intent_with_token(
        &self,
        intent_id: &str,
        token: &str,
    ) -> Result<AirwallexPaymentIntent> {
        let access_token = self.get_access_token().await?;

        let confirm_payload = serde_json::json!({
            "request_id": uuid::Uuid::new_v4().to_string(),
            "payment_method": token,
            "payment_method_options": {
                "card": {
                    "three_ds_action": "FORCE_3DS"
                }
            }
        });

        let response = self
            .client
            .post(format!(
                "{}/pa/payment_intents/{}/confirm",
                self.base_url, intent_id
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .json(&confirm_payload)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Airwallex confirm error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "Airwallex confirm error: {}",
                error_text
            )));
        }

        let intent: AirwallexPaymentIntent = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse Airwallex response: {}", e))
        })?;

        Ok(intent)
    }

    fn handle_intent_response(
        &self,
        intent: AirwallexPaymentIntent,
        _currency: &str,
    ) -> Result<InitiatePaymentResponse> {
        match intent.status.as_str() {
            "SUCCEEDED" | "CAPTURED" => {
                let _amount = Decimal::from_f64_retain(intent.amount);
                let id = intent.id.clone();
                Ok(InitiatePaymentResponse::Success {
                    payment_id: id.clone(),
                    transaction_id: id,
                    payment_status: PaymentStatus::Succeeded,
                    payment_method: PaymentMethodInfo {
                        method_type: PaymentMethodType::Card,
                        last_four: None,
                        card_brand: None,
                        exp_month: None,
                        exp_year: None,
                        cardholder_name: None,
                        token: None,
                    },
                    receipt_url: None,
                })
            }
            "REQUIRES_ACTION" => Ok(InitiatePaymentResponse::RequiresAction {
                payment_id: intent.id,
                action_type: PaymentActionType::ThreeDSecure,
                action_data: serde_json::json!({
                    "client_secret": intent.client_secret,
                    "next_action": intent.next_action,
                }),
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
            }),
            "FAILED" => Ok(InitiatePaymentResponse::Failed {
                payment_id: intent.id,
                error_code: "payment_failed".to_string(),
                error_message: "Payment failed".to_string(),
                retry_allowed: true,
            }),
            _ => Ok(InitiatePaymentResponse::RequiresAction {
                payment_id: intent.id,
                action_type: PaymentActionType::Verification,
                action_data: serde_json::json!({
                    "status": intent.status,
                    "client_secret": intent.client_secret,
                }),
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
            }),
        }
    }

    fn handle_complete_action_response(
        &self,
        intent: AirwallexPaymentIntent,
        _request: &CompletePaymentActionRequest,
    ) -> Result<CompletePaymentActionResponse> {
        match intent.status.as_str() {
            "SUCCEEDED" | "CAPTURED" => {
                let id = intent.id.clone();
                Ok(CompletePaymentActionResponse::Success {
                    payment_id: id.clone(),
                    transaction_id: id,
                    payment_status: PaymentStatus::Succeeded,
                    payment_method: PaymentMethodInfo {
                        method_type: PaymentMethodType::Card,
                        last_four: None,
                        card_brand: None,
                        exp_month: None,
                        exp_year: None,
                        cardholder_name: None,
                        token: None,
                    },
                    receipt_url: None,
                })
            }
            "REQUIRES_ACTION" => Ok(CompletePaymentActionResponse::RequiresAction {
                payment_id: intent.id,
                action_type: PaymentActionType::ThreeDSecure,
                action_data: serde_json::json!({
                    "client_secret": intent.client_secret,
                    "next_action": intent.next_action,
                }),
            }),
            "FAILED" | "CANCELLED" => Ok(CompletePaymentActionResponse::Failed {
                payment_id: intent.id,
                error_code: "payment_failed".to_string(),
                error_message: "Payment failed".to_string(),
                retry_allowed: true,
            }),
            _ => Ok(CompletePaymentActionResponse::RequiresAction {
                payment_id: intent.id,
                action_type: PaymentActionType::Verification,
                action_data: serde_json::json!({
                    "status": intent.status,
                }),
            }),
        }
    }
}

// Helper types
struct CardData {
    number: String,
    exp_month: String,
    exp_year: String,
    cvc: String,
    name: String,
}

// Airwallex API response types
#[derive(Debug, Serialize, Deserialize)]
struct AirwallexAuthResponse {
    token: String,
    #[serde(rename = "expires_at")]
    expires_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexPaymentIntent {
    id: String,
    #[serde(rename = "client_secret")]
    client_secret: String,
    status: String,
    // Airwallex echoes amounts as JSON floats in major currency units
    // regardless of the unit sent in the request.
    amount: f64,
    currency: String,
    #[serde(default)]
    // Airwallex echoes metadata back verbatim — keep this tolerant of any
    // JSON value (e.g. nulls) since the echoed content is never read.
    metadata: serde_json::Value,
    #[serde(default)]
    next_action: Option<serde_json::Value>,
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

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexPaymentMethod {
    id: String,
    #[serde(rename = "type")]
    _type: String,
    card: Option<AirwallexCard>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexCard {
    #[serde(rename = "number_last_four")]
    number_last_four: String,
    brand: String,
    expiry_month: String,
    expiry_year: String,
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AirwallexPaymentMethodList {
    items: Vec<AirwallexPaymentMethod>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_airwallex_agnostic_gateway_creation() {
        let gateway = AirwallexAgnosticGateway::new(
            "test_client_id".to_string(),
            "test_api_key".to_string(),
            "test_webhook_secret".to_string(),
            true, // demo mode
        );

        assert_eq!(gateway.client_id, "test_client_id");
        // Demo mode targets the Airwallex sandbox API (the demo elements
        // stack authenticates against sandbox, not the legacy api-demo host)
        assert!(gateway.base_url.contains("sandbox"));
    }

    #[test]
    fn test_map_status() {
        assert_eq!(
            AirwallexAgnosticGateway::map_status("REQUIRES_ACTION"),
            PaymentStatus::Pending
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_status("PENDING"),
            PaymentStatus::Pending
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_status("PROCESSING"),
            PaymentStatus::Processing
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_status("SUCCEEDED"),
            PaymentStatus::Succeeded
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_status("CAPTURED"),
            PaymentStatus::Succeeded
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_status("FAILED"),
            PaymentStatus::Failed
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_status("CANCELLED"),
            PaymentStatus::Failed
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_status("REFUNDED"),
            PaymentStatus::Refunded
        );
    }

    #[test]
    fn test_map_refund_status() {
        assert_eq!(
            AirwallexAgnosticGateway::map_refund_status("PENDING"),
            RefundStatus::Pending
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_refund_status("SUCCEEDED"),
            RefundStatus::Succeeded
        );
        assert_eq!(
            AirwallexAgnosticGateway::map_refund_status("FAILED"),
            RefundStatus::Failed
        );
    }

    /// Compute the value Airwallex would send in the `x-signature` header:
    /// hex(HMAC-SHA256(secret, timestamp + raw_body)).
    fn airwallex_signature(secret: &str, timestamp: &str, body: &[u8]) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(timestamp.as_bytes());
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    fn test_gateway() -> AirwallexAgnosticGateway {
        AirwallexAgnosticGateway::new(
            "test_client_id".to_string(),
            "test_api_key".to_string(),
            "test_webhook_secret".to_string(),
            true,
        )
    }

    #[tokio::test]
    async fn test_webhook_verifies_real_airwallex_scheme() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "id": "evt_123",
            "name": "payment_intent.succeeded",
            "data": { "object": { "id": "int_xxx", "status": "SUCCEEDED" } }
        })
        .to_string();
        let signature = airwallex_signature("test_webhook_secret", timestamp, body.as_bytes());

        let headers = vec![
            ("x-timestamp".to_string(), timestamp.to_string()),
            ("x-signature".to_string(), signature),
        ];

        let event = gateway
            .handle_webhook(body.as_bytes(), &headers)
            .await
            .expect("webhook should verify");

        assert_eq!(event.event_type, WebhookEventType::PaymentSucceeded);
        assert_eq!(event.payment_id, "int_xxx");
    }

    #[tokio::test]
    async fn test_webhook_accepts_legacy_type_field() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "id": "evt_124",
            "type": "payment_intent.failed",
            "data": { "object": { "id": "int_yyy", "status": "FAILED" } }
        })
        .to_string();
        let signature = airwallex_signature("test_webhook_secret", timestamp, body.as_bytes());

        let headers = vec![
            ("X-Timestamp".to_string(), timestamp.to_string()),
            ("X-Signature".to_string(), signature),
        ];

        let event = gateway
            .handle_webhook(body.as_bytes(), &headers)
            .await
            .expect("webhook should verify");

        assert_eq!(event.event_type, WebhookEventType::PaymentFailed);
        assert_eq!(event.payment_id, "int_yyy");
    }

    #[tokio::test]
    async fn test_webhook_rejects_tampered_body() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "name": "payment_intent.succeeded",
            "data": { "object": { "id": "int_xxx", "status": "SUCCEEDED" } }
        })
        .to_string();
        let signature = airwallex_signature("test_webhook_secret", timestamp, body.as_bytes());

        let tampered = body.replace("int_xxx", "int_evil");
        let headers = vec![
            ("x-timestamp".to_string(), timestamp.to_string()),
            ("x-signature".to_string(), signature),
        ];

        let result = gateway.handle_webhook(tampered.as_bytes(), &headers).await;
        assert!(result.is_err(), "tampered body must fail verification");
    }

    #[tokio::test]
    async fn test_webhook_rejects_wrong_secret() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "name": "payment_intent.succeeded",
            "data": { "object": { "id": "int_xxx", "status": "SUCCEEDED" } }
        })
        .to_string();
        let signature = airwallex_signature("wrong_secret", timestamp, body.as_bytes());

        let headers = vec![
            ("x-timestamp".to_string(), timestamp.to_string()),
            ("x-signature".to_string(), signature),
        ];

        let result = gateway.handle_webhook(body.as_bytes(), &headers).await;
        assert!(result.is_err(), "signature from wrong secret must fail");
    }

    #[tokio::test]
    async fn test_webhook_rejects_missing_headers() {
        let gateway = test_gateway();
        let body = serde_json::json!({
            "name": "payment_intent.succeeded",
            "data": { "object": { "id": "int_xxx", "status": "SUCCEEDED" } }
        })
        .to_string();

        let result = gateway.handle_webhook(body.as_bytes(), &[]).await;
        assert!(result.is_err(), "missing signature headers must fail");
    }

    #[tokio::test]
    async fn test_webhook_refund_succeeded_maps_transaction_id() {
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
        let signature = airwallex_signature("test_webhook_secret", timestamp, body.as_bytes());

        let headers = vec![
            ("x-timestamp".to_string(), timestamp.to_string()),
            ("x-signature".to_string(), signature),
        ];

        let event = gateway
            .handle_webhook(body.as_bytes(), &headers)
            .await
            .expect("refund webhook should verify");

        assert_eq!(event.event_type, WebhookEventType::PaymentRefunded);
        assert_eq!(event.payment_id, "ref_zzz");
        assert_eq!(event.transaction_id.as_deref(), Some("int_xxx"));
    }

    #[tokio::test]
    async fn test_webhook_rejects_unsupported_event() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "name": "payment_intent.unknown_event",
            "data": { "object": { "id": "int_xxx" } }
        })
        .to_string();
        let signature = airwallex_signature("test_webhook_secret", timestamp, body.as_bytes());

        let headers = vec![
            ("x-timestamp".to_string(), timestamp.to_string()),
            ("x-signature".to_string(), signature),
        ];

        let result = gateway.handle_webhook(body.as_bytes(), &headers).await;
        assert!(
            result.is_err(),
            "unsupported event types should be rejected"
        );
    }

    #[tokio::test]
    async fn test_webhook_payment_consent_verified() {
        let gateway = test_gateway();
        let timestamp = "1712345678000";
        let body = serde_json::json!({
            "name": "payment_consent.verified",
            "data": { "object": { "id": "pc_123", "status": "VERIFIED" } }
        })
        .to_string();
        let signature = airwallex_signature("test_webhook_secret", timestamp, body.as_bytes());

        let headers = vec![
            ("x-timestamp".to_string(), timestamp.to_string()),
            ("x-signature".to_string(), signature),
        ];

        let event = gateway
            .handle_webhook(body.as_bytes(), &headers)
            .await
            .expect("payment_consent.verified webhook should verify");

        assert_eq!(event.event_type, WebhookEventType::PaymentConsentVerified);
        assert_eq!(event.payment_id, "pc_123");
    }
}
