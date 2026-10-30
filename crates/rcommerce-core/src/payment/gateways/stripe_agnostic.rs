//! Stripe Payment Gateway - Agnostic Implementation
//!
//! Server-to-server implementation that handles card data securely without exposing keys to frontend.

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;

use crate::payment::agnostic::*;
use crate::Result;

const STRIPE_WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS: i64 = 300;

pub struct StripeAgnosticGateway {
    api_key: String,
    webhook_secret: String,
    client: reqwest::Client,
    supported_methods: Vec<PaymentMethodConfig>,
}

impl StripeAgnosticGateway {
    pub fn new(api_key: String, webhook_secret: String) -> Self {
        // Define supported payment methods for Stripe
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
                supported_currencies: vec![], // All currencies supported
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
            PaymentMethodConfig {
                method_type: PaymentMethodType::BuyNowPayLater,
                enabled: true,
                display_name: "Buy Now, Pay Later".to_string(),
                requires_redirect: true,
                supports_3ds: false,
                supports_tokenization: false,
                supports_recurring: false,
                required_fields: vec![],
                optional_fields: vec![],
                supported_currencies: vec!["USD".to_string(), "EUR".to_string(), "GBP".to_string()],
                min_amount: Some(dec!(1.00)),
                max_amount: Some(dec!(10000.00)),
            },
        ];

        Self {
            api_key,
            webhook_secret,
            client: reqwest::Client::new(),
            supported_methods,
        }
    }

    fn verify_webhook_signature(
        &self,
        payload: &[u8],
        signature_header: &str,
        now: i64,
    ) -> Result<()> {
        if self.webhook_secret.trim().is_empty() {
            return Err(crate::Error::config(
                "Stripe webhook secret is not configured",
            ));
        }

        let mut timestamp = None;
        let mut signatures = Vec::new();
        for part in signature_header.split(',') {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| crate::Error::validation("Malformed Stripe-Signature header"))?;
            let value = value.trim();
            match key.trim() {
                "t" => {
                    if timestamp.is_some() {
                        return Err(crate::Error::validation(
                            "Malformed Stripe-Signature header: duplicate timestamp",
                        ));
                    }
                    let parsed = value.parse::<i64>().map_err(|_| {
                        crate::Error::validation(
                            "Malformed Stripe-Signature header: invalid timestamp",
                        )
                    })?;
                    timestamp = Some((value, parsed));
                }
                "v1" => {
                    let signature = hex::decode(value).map_err(|_| {
                        crate::Error::validation(
                            "Malformed Stripe-Signature header: invalid v1 signature",
                        )
                    })?;
                    if signature.len() != 32 {
                        return Err(crate::Error::validation(
                            "Malformed Stripe-Signature header: invalid v1 signature length",
                        ));
                    }
                    signatures.push(signature);
                }
                _ => {}
            }
        }

        let (timestamp_value, timestamp) = timestamp.ok_or_else(|| {
            crate::Error::validation("Malformed Stripe-Signature header: missing timestamp")
        })?;
        if signatures.is_empty() {
            return Err(crate::Error::validation(
                "Malformed Stripe-Signature header: missing v1 signature",
            ));
        }
        if now.saturating_sub(timestamp) > STRIPE_WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS
            || timestamp.saturating_sub(now) > STRIPE_WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS
        {
            return Err(crate::Error::validation(
                "Stripe webhook timestamp is outside the allowed tolerance",
            ));
        }

        for signature in signatures {
            let mut mac = Hmac::<Sha256>::new_from_slice(self.webhook_secret.as_bytes())
                .map_err(|e| crate::Error::validation(format!("Invalid webhook secret: {}", e)))?;
            mac.update(timestamp_value.as_bytes());
            mac.update(b".");
            mac.update(payload);
            if mac.verify_slice(&signature).is_ok() {
                return Ok(());
            }
        }

        Err(crate::Error::validation("Invalid Stripe webhook signature"))
    }

    /// Create a Stripe payment method from card data
    async fn create_payment_method(&self, card_data: &CardData) -> Result<String> {
        let params = [
            ("type", "card"),
            ("card[number]", &card_data.number),
            ("card[exp_month]", &card_data.exp_month),
            ("card[exp_year]", &card_data.exp_year),
            ("card[cvc]", &card_data.cvc),
            ("billing_details[name]", &card_data.name),
        ];

        let response = self
            .client
            .post("https://api.stripe.com/v1/payment_methods")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(parse_stripe_error(response).await);
        }

        let payment_method: StripePaymentMethod = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

        Ok(payment_method.id)
    }

    /// Create a payment intent
    async fn create_payment_intent(
        &self,
        amount: Decimal,
        currency: &str,
        payment_method_id: &str,
        customer_email: &str,
        description: &str,
        metadata: serde_json::Value,
    ) -> Result<StripePaymentIntent> {
        let amount_in_cents = stripe_amount_to_minor_units(amount, currency)?;

        let mut params: Vec<(&str, String)> = vec![
            ("amount", amount_in_cents.to_string()),
            ("currency", currency.to_lowercase()),
            ("payment_method", payment_method_id.to_string()),
            ("confirmation_method", "manual".to_string()),
            ("capture_method", "automatic".to_string()),
            ("receipt_email", customer_email.to_string()),
            ("description", description.to_string()),
            ("confirm", "true".to_string()),
        ];

        // Add metadata
        if let Some(order_id) = metadata.get("order_id").and_then(|v| v.as_str()) {
            params.push(("metadata[order_id]", order_id.to_string()));
        }

        let response = self
            .client
            .post("https://api.stripe.com/v1/payment_intents")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(parse_stripe_error(response).await);
        }

        let intent: StripePaymentIntent = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

        Ok(intent)
    }

    /// Confirm a payment intent that requires action
    async fn confirm_payment_intent(&self, intent_id: &str) -> Result<StripePaymentIntent> {
        let response = self
            .client
            .post(format!(
                "https://api.stripe.com/v1/payment_intents/{}/confirm",
                intent_id
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(parse_stripe_error(response).await);
        }

        let intent: StripePaymentIntent = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

        Ok(intent)
    }

    /// Map Stripe status to our PaymentStatus
    fn map_status(status: &str) -> PaymentStatus {
        match status {
            "requires_payment_method" => PaymentStatus::Pending,
            "requires_confirmation" => PaymentStatus::Pending,
            "requires_action" => PaymentStatus::RequiresAction,
            "processing" => PaymentStatus::Processing,
            "succeeded" => PaymentStatus::Succeeded,
            "canceled" => PaymentStatus::Cancelled,
            _ => PaymentStatus::Failed,
        }
    }

    /// Extract card info from payment method
    fn extract_card_info(&self, payment_method: &StripePaymentMethod) -> PaymentMethodInfo {
        PaymentMethodInfo {
            method_type: PaymentMethodType::Card,
            last_four: payment_method.card.as_ref().map(|c| c.last4.clone()),
            card_brand: payment_method.card.as_ref().map(|c| c.brand.clone()),
            exp_month: payment_method
                .card
                .as_ref()
                .map(|c| c.exp_month.to_string()),
            exp_year: payment_method.card.as_ref().map(|c| c.exp_year.to_string()),
            cardholder_name: payment_method.billing_details.name.clone(),
            token: None,
        }
    }
}

#[async_trait]
impl AgnosticPaymentGateway for StripeAgnosticGateway {
    async fn get_config(&self) -> Result<GatewayConfig> {
        Ok(GatewayConfig {
            gateway_id: "stripe".to_string(),
            gateway_name: "Stripe".to_string(),
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
                "JPY".to_string(),
                "CNY".to_string(),
                "HKD".to_string(),
                "SGD".to_string(),
            ],
            default_currency: "USD".to_string(),
        })
    }

    async fn initiate_payment(
        &self,
        request: InitiatePaymentRequest,
    ) -> Result<InitiatePaymentResponse> {
        // Extract card data from payment method data
        let card_data = match &request.payment_method_data {
            PaymentMethodData::Card {
                number,
                exp_month,
                exp_year,
                cvc,
                name,
            } => CardData {
                number: number.clone(),
                exp_month: exp_month.clone(),
                exp_year: exp_year.clone(),
                cvc: cvc.clone(),
                name: name.clone(),
            },
            PaymentMethodData::CardToken { token, .. } => {
                // Use existing token
                let intent = self
                    .create_payment_intent_with_token(
                        request.amount,
                        &request.currency,
                        token,
                        &request.customer_email,
                        &request.description,
                        request.metadata,
                    )
                    .await?;

                return self.handle_intent_response(intent);
            }
            _ => {
                return Err(crate::Error::validation(
                    "Unsupported payment method for Stripe",
                ))
            }
        };

        // Step 1: Create a payment method with Stripe
        let payment_method_id = self.create_payment_method(&card_data).await?;

        // Step 2: Create and confirm the payment intent
        let intent = self
            .create_payment_intent(
                request.amount,
                &request.currency,
                &payment_method_id,
                &request.customer_email,
                &request.description,
                request.metadata,
            )
            .await?;

        // Step 3: Handle the response based on status
        self.handle_intent_response(intent)
    }

    async fn complete_payment_action(
        &self,
        request: CompletePaymentActionRequest,
    ) -> Result<CompletePaymentActionResponse> {
        // Confirm the payment intent
        let intent = self.confirm_payment_intent(&request.payment_id).await?;

        match intent.status.as_str() {
            "succeeded" => {
                // Get payment method info if available
                let payment_method = if let Some(ref pm) = intent.payment_method {
                    match self.get_payment_method(&pm.id).await {
                        Ok(pm) => self.extract_card_info(&pm),
                        Err(_) => PaymentMethodInfo {
                            method_type: PaymentMethodType::Card,
                            last_four: None,
                            card_brand: None,
                            exp_month: None,
                            exp_year: None,
                            cardholder_name: None,
                            token: None,
                        },
                    }
                } else {
                    PaymentMethodInfo {
                        method_type: PaymentMethodType::Card,
                        last_four: None,
                        card_brand: None,
                        exp_month: None,
                        exp_year: None,
                        cardholder_name: None,
                        token: None,
                    }
                };

                let payment_id = intent.id.clone();
                let transaction_id = intent
                    .charges
                    .data
                    .first()
                    .map(|c| c.id.clone())
                    .unwrap_or_else(|| intent.id.clone());
                let receipt_url = intent
                    .charges
                    .data
                    .first()
                    .and_then(|c| c.receipt_url.clone());

                Ok(CompletePaymentActionResponse::Success {
                    payment_id,
                    transaction_id,
                    payment_status: PaymentStatus::Succeeded,
                    payment_method,
                    receipt_url,
                })
            }
            "requires_action" | "requires_source_action" => {
                // Get the next action (3DS)
                if let Some(action) = intent.next_action {
                    Ok(CompletePaymentActionResponse::RequiresAction {
                        payment_id: intent.id.clone(),
                        action_type: PaymentActionType::ThreeDSecure,
                        action_data: json!({
                            "type": action.type_,
                            "redirect_url": action.redirect_to_url,
                        }),
                    })
                } else {
                    Err(crate::Error::payment_error(
                        "Payment requires action but no action data",
                    ))
                }
            }
            "canceled" => Ok(CompletePaymentActionResponse::Failed {
                payment_id: intent.id.clone(),
                error_code: "payment_cancelled".to_string(),
                error_message: "Payment was cancelled".to_string(),
                retry_allowed: true,
            }),
            _ => Ok(CompletePaymentActionResponse::Failed {
                payment_id: intent.id.clone(),
                error_code: "payment_failed".to_string(),
                error_message: format!("Payment failed with status: {}", intent.status),
                retry_allowed: true,
            }),
        }
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
        let mut params: Vec<(&str, String)> = vec![
            ("payment_intent", payment_id.to_string()),
            ("reason", map_refund_reason(reason)),
        ];

        if let Some(amt) = amount {
            let intent = self.get_payment_intent(payment_id).await?;
            let amount_in_cents = stripe_amount_to_minor_units(amt, &intent.currency)?;
            params.push(("amount", amount_in_cents.to_string()));
        }

        let response = self
            .client
            .post("https://api.stripe.com/v1/refunds")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let raw_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown".to_string());

            // Try to parse as Stripe error, if fails return raw text
            if let Ok(error) = serde_json::from_str::<StripeError>(&raw_text) {
                return Err(crate::Error::payment_error(format!(
                    "Stripe error: {} - {}",
                    error.error.code, error.error.message
                )));
            }

            // Return raw response if parsing failed
            return Err(crate::Error::payment_error(format!(
                "Stripe error (status {}): {}",
                status, raw_text
            )));
        }

        let refund: StripeRefund = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

        Ok(RefundResponse {
            refund_id: refund.id,
            payment_id: payment_id.to_string(),
            amount: stripe_amount_from_minor_units(refund.amount, &refund.currency),
            currency: refund.currency.to_uppercase(),
            status: match refund.status.as_str() {
                "pending" => RefundStatus::Pending,
                "succeeded" => RefundStatus::Succeeded,
                "failed" => RefundStatus::Failed,
                _ => RefundStatus::Pending,
            },
            reason: reason.to_string(),
            created_at: chrono::DateTime::from_timestamp(refund.created, 0)
                .unwrap_or_else(chrono::Utc::now),
        })
    }

    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &[(String, String)],
    ) -> Result<WebhookEvent> {
        let signature = headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("Stripe-Signature"))
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| crate::Error::validation("Missing Stripe-Signature header"))?;
        self.verify_webhook_signature(payload, signature, chrono::Utc::now().timestamp())?;

        let event: StripeWebhookEvent = serde_json::from_slice(payload)
            .map_err(|e| crate::Error::validation(format!("Invalid webhook payload: {}", e)))?;

        let (event_type, payment_id, transaction_id, refund_data) = match event.type_.as_str() {
            "payment_intent.succeeded" => (
                WebhookEventType::PaymentSucceeded,
                event
                    .data
                    .object
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                event
                    .data
                    .object
                    .get("charges")
                    .and_then(|c| c.get("data"))
                    .and_then(|d| d.as_array())
                    .and_then(|a| a.first())
                    .and_then(|c| c.get("id"))
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string()),
                None,
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
                None,
                None,
            ),
            "payment_intent.canceled" => (
                WebhookEventType::PaymentCancelled,
                event
                    .data
                    .object
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                None,
                None,
            ),
            "charge.refunded" => {
                let (payment_id, refund_id, data) = stripe_refund_from_charge(&event.data.object)?;
                (
                    WebhookEventType::PaymentRefunded,
                    payment_id,
                    Some(refund_id),
                    Some(data),
                )
            }
            _ => return Err(crate::Error::validation("Unsupported webhook event type")),
        };

        Ok(WebhookEvent {
            event_type,
            payment_id,
            transaction_id,
            data: refund_data.unwrap_or(event.data.object),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn tokenize_payment_method(
        &self,
        payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken> {
        let card_data = match payment_method_data {
            PaymentMethodData::Card {
                number,
                exp_month,
                exp_year,
                cvc,
                name,
            } => CardData {
                number,
                exp_month,
                exp_year,
                cvc,
                name,
            },
            _ => {
                return Err(crate::Error::validation(
                    "Only card tokenization is supported",
                ))
            }
        };

        let payment_method_id = self.create_payment_method(&card_data).await?;

        // Get the payment method details
        let pm = self.get_payment_method(&payment_method_id).await?;

        Ok(PaymentMethodToken {
            token: payment_method_id,
            payment_method: self.extract_card_info(&pm),
            expires_at: None, // Stripe tokens don't expire
        })
    }

    async fn create_customer(&self, email: &str, name: &str) -> Result<String> {
        let mut params = vec![("email", email)];
        if !name.trim().is_empty() {
            params.push(("name", name));
        }
        let response = self
            .client
            .post("https://api.stripe.com/v1/customers")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;
        if !response.status().is_success() {
            return Err(parse_stripe_error(response).await);
        }
        let customer: serde_json::Value = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse customer: {}", e)))?;
        customer
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| crate::Error::payment_error("Stripe customer response missing id"))
    }

    async fn tokenize_payment_method_for_customer(
        &self,
        customer_id: &str,
        payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken> {
        let token = self.tokenize_payment_method(payment_method_data).await?;
        let response = self
            .client
            .post(format!(
                "https://api.stripe.com/v1/payment_methods/{}/attach",
                token.token
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&[("customer", customer_id)])
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;
        if !response.status().is_success() {
            return Err(parse_stripe_error(response).await);
        }
        Ok(token)
    }

    async fn get_saved_payment_methods(&self, customer_id: &str) -> Result<Vec<PaymentMethodInfo>> {
        // List payment methods for customer
        let response = self
            .client
            .get("https://api.stripe.com/v1/payment_methods")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .query(&[("customer", customer_id), ("type", "card")])
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::payment_error(
                "Failed to get saved payment methods",
            ));
        }

        let list: StripeList<StripePaymentMethod> = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

        Ok(list
            .data
            .iter()
            .map(|pm| {
                let mut info = self.extract_card_info(pm);
                info.token = Some(pm.id.clone());
                info
            })
            .collect())
    }

    async fn delete_payment_method(&self, token: &str) -> Result<()> {
        let response = self
            .client
            .post(format!(
                "https://api.stripe.com/v1/payment_methods/{}/detach",
                token
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::payment_error(
                "Failed to delete payment method",
            ));
        }

        Ok(())
    }
}

// Helper methods
impl StripeAgnosticGateway {
    async fn get_payment_intent(&self, payment_id: &str) -> Result<StripePaymentIntent> {
        let response = self
            .client
            .get(format!(
                "https://api.stripe.com/v1/payment_intents/{}",
                payment_id
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::payment_error("Failed to get payment status"));
        }

        response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))
    }

    async fn create_payment_intent_with_token(
        &self,
        amount: Decimal,
        currency: &str,
        token: &str,
        customer_email: &str,
        description: &str,
        metadata: serde_json::Value,
    ) -> Result<StripePaymentIntent> {
        let amount_in_cents = stripe_amount_to_minor_units(amount, currency)?;

        let mut params: Vec<(&str, String)> = vec![
            ("amount", amount_in_cents.to_string()),
            ("currency", currency.to_lowercase()),
            ("payment_method", token.to_string()),
            ("confirmation_method", "manual".to_string()),
            ("capture_method", "automatic".to_string()),
            ("receipt_email", customer_email.to_string()),
            ("description", description.to_string()),
            ("confirm", "true".to_string()),
        ];

        if let Some(order_id) = metadata.get("order_id").and_then(|v| v.as_str()) {
            params.push(("metadata[order_id]", order_id.to_string()));
        }

        let response = self
            .client
            .post("https://api.stripe.com/v1/payment_intents")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(parse_stripe_error(response).await);
        }

        let intent: StripePaymentIntent = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

        Ok(intent)
    }

    async fn get_payment_method(&self, payment_method_id: &str) -> Result<StripePaymentMethod> {
        let response = self
            .client
            .get(format!(
                "https://api.stripe.com/v1/payment_methods/{}",
                payment_method_id
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("Stripe API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::payment_error("Failed to get payment method"));
        }

        let pm: StripePaymentMethod = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

        Ok(pm)
    }

    fn handle_intent_response(
        &self,
        intent: StripePaymentIntent,
    ) -> Result<InitiatePaymentResponse> {
        match intent.status.as_str() {
            "succeeded" => {
                let payment_method = intent
                    .payment_method
                    .as_ref()
                    .map(|pm| PaymentMethodInfo {
                        method_type: PaymentMethodType::Card,
                        last_four: pm.card.as_ref().map(|c| c.last4.clone()),
                        card_brand: pm.card.as_ref().map(|c| c.brand.clone()),
                        exp_month: pm.card.as_ref().map(|c| c.exp_month.to_string()),
                        exp_year: pm.card.as_ref().map(|c| c.exp_year.to_string()),
                        cardholder_name: pm.billing_details.name.clone(),
                        token: Some(pm.id.clone()),
                    })
                    .unwrap_or_else(|| PaymentMethodInfo {
                        method_type: PaymentMethodType::Card,
                        last_four: None,
                        card_brand: None,
                        exp_month: None,
                        exp_year: None,
                        cardholder_name: None,
                        token: None,
                    });

                let payment_id = intent.id.clone();
                let transaction_id = intent
                    .charges
                    .data
                    .first()
                    .map(|c| c.id.clone())
                    .unwrap_or_else(|| intent.id.clone());
                let receipt_url = intent
                    .charges
                    .data
                    .first()
                    .and_then(|c| c.receipt_url.clone());

                Ok(InitiatePaymentResponse::Success {
                    payment_id,
                    transaction_id,
                    payment_status: PaymentStatus::Succeeded,
                    payment_method,
                    receipt_url,
                })
            }
            "requires_action" | "requires_source_action" => {
                // 3D Secure required
                if let Some(action) = intent.next_action {
                    Ok(InitiatePaymentResponse::RequiresAction {
                        payment_id: intent.id.clone(),
                        action_type: PaymentActionType::ThreeDSecure,
                        action_data: json!({
                            "type": action.type_,
                            "redirect_url": action.redirect_to_url,
                            "use_stripe_sdk": action.use_stripe_sdk,
                        }),
                        expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
                    })
                } else {
                    Err(crate::Error::payment_error(
                        "Payment requires action but no action data",
                    ))
                }
            }
            "requires_payment_method" => {
                // Payment method failed
                Ok(InitiatePaymentResponse::Failed {
                    payment_id: intent.id.clone(),
                    error_code: "payment_method_failed".to_string(),
                    error_message: "The payment method was declined".to_string(),
                    retry_allowed: true,
                })
            }
            _ => Ok(InitiatePaymentResponse::Failed {
                payment_id: intent.id.clone(),
                error_code: "payment_failed".to_string(),
                error_message: format!("Payment failed with status: {}", intent.status),
                retry_allowed: true,
            }),
        }
    }
}

// Stripe API types
#[derive(Debug, Deserialize)]
struct StripeError {
    error: StripeErrorDetails,
}

#[derive(Debug, Deserialize)]
struct StripeErrorDetails {
    code: String,
    message: String,
}

/// Helper to parse Stripe error responses with better error messages
async fn parse_stripe_error(response: reqwest::Response) -> crate::Error {
    let status = response.status();
    let raw_text = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown".to_string());

    if let Ok(error) = serde_json::from_str::<StripeError>(&raw_text) {
        return crate::Error::payment_error(format!(
            "Stripe error: {} - {}",
            error.error.code, error.error.message
        ));
    }

    crate::Error::payment_error(format!("Stripe error (status {}): {}", status, raw_text))
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct StripePaymentMethod {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    type_: String,
    card: Option<StripeCard>,
    billing_details: StripeBillingDetails,
}

#[derive(Debug, Deserialize)]
struct StripeCard {
    brand: String,
    last4: String,
    exp_month: u32,
    exp_year: u32,
}

#[derive(Debug, Deserialize)]
struct StripeBillingDetails {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct StripePaymentIntent {
    id: String,
    status: String,
    #[allow(dead_code)]
    amount: i64,
    #[allow(dead_code)]
    currency: String,
    charges: StripeList<StripeCharge>,
    next_action: Option<StripeNextAction>,
    payment_method: Option<StripePaymentMethod>,
}

#[derive(Debug, Deserialize)]
struct StripeCharge {
    id: String,
    receipt_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StripeNextAction {
    #[serde(rename = "type")]
    type_: String,
    redirect_to_url: Option<serde_json::Value>,
    use_stripe_sdk: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct StripeRefund {
    id: String,
    amount: i64,
    currency: String,
    status: String,
    created: i64,
}

#[derive(Debug, Deserialize)]
struct StripeList<T> {
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct StripeWebhookEvent {
    #[allow(dead_code)]
    id: String,
    #[serde(rename = "type")]
    type_: String,
    data: StripeWebhookData,
}

#[derive(Debug, Deserialize)]
struct StripeWebhookData {
    object: serde_json::Value,
}

// Helper types
struct CardData {
    number: String,
    exp_month: String,
    exp_year: String,
    cvc: String,
    name: String,
}

fn stripe_refund_from_charge(
    charge: &serde_json::Value,
) -> Result<(String, String, serde_json::Value)> {
    let payment_id = charge
        .get("payment_intent")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| crate::Error::validation("Stripe refund webhook is missing payment_intent"))?
        .to_string();
    let refund = charge
        .get("refunds")
        .and_then(|refunds| refunds.get("data"))
        .and_then(serde_json::Value::as_array)
        .and_then(|refunds| refunds.first())
        .ok_or_else(|| {
            crate::Error::validation("Stripe charge.refunded webhook has no refund object")
        })?;
    let refund_id = refund
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.starts_with("re_"))
        .ok_or_else(|| crate::Error::validation("Stripe refund webhook has an invalid refund ID"))?
        .to_string();
    let amount = refund
        .get("amount")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| crate::Error::validation("Stripe refund webhook is missing amount"))?;
    let currency = refund
        .get("currency")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| crate::Error::validation("Stripe refund webhook is missing currency"))?;
    refund
        .get("status")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| crate::Error::validation("Stripe refund webhook is missing status"))?;

    let mut data = refund.clone();
    data["amount"] = json!(stripe_amount_from_minor_units(amount, currency).to_string());
    Ok((payment_id, refund_id, data))
}

pub(crate) fn stripe_currency_multiplier(currency: &str) -> Decimal {
    if matches!(
        currency.to_ascii_uppercase().as_str(),
        "BIF"
            | "CLP"
            | "DJF"
            | "GNF"
            | "JPY"
            | "KMF"
            | "KRW"
            | "MGA"
            | "PYG"
            | "RWF"
            | "UGX"
            | "VND"
            | "VUV"
            | "XAF"
            | "XOF"
            | "XPF"
    ) {
        Decimal::ONE
    } else {
        dec!(100)
    }
}

pub(crate) fn stripe_amount_to_minor_units(amount: Decimal, currency: &str) -> Result<i64> {
    amount
        .checked_mul(stripe_currency_multiplier(currency))
        .ok_or_else(|| {
            crate::Error::validation(format!(
                "Amount is outside Stripe's supported range for {}",
                currency.to_ascii_uppercase()
            ))
        })?
        .try_into()
        .map_err(|_| {
            crate::Error::validation(format!(
                "Amount is outside Stripe's supported range for {}",
                currency.to_ascii_uppercase()
            ))
        })
}

pub(crate) fn stripe_amount_from_minor_units(amount: i64, currency: &str) -> Decimal {
    Decimal::from(amount) / stripe_currency_multiplier(currency)
}

pub(crate) fn map_refund_reason(reason: &str) -> String {
    match reason {
        "duplicate" => "duplicate".to_string(),
        "fraudulent" => "fraudulent".to_string(),
        "requested_by_customer" => "requested_by_customer".to_string(),
        _ => "requested_by_customer".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEBHOOK_SECRET: &str = "whsec_test_secret";
    const WEBHOOK_PAYLOAD: &[u8] = br#"{"id":"evt_test","type":"payment_intent.succeeded","data":{"object":{"id":"pi_test","charges":{"data":[{"id":"ch_test"}]}}}}"#;
    const CHARGE_REFUNDED_PARTIAL_PAYLOAD: &[u8] = br#"{"id":"evt_charge_refunded","type":"charge.refunded","data":{"object":{"id":"ch_full_amount","payment_intent":"pi_direct_partial","amount":10000,"amount_refunded":3500,"currency":"usd","refunds":{"object":"list","data":[{"id":"re_direct_partial","object":"refund","amount":2500,"currency":"usd","status":"succeeded","payment_intent":"pi_direct_partial","created":1700000100},{"id":"re_previous_partial","object":"refund","amount":1000,"currency":"usd","status":"succeeded","payment_intent":"pi_direct_partial","created":1700000000}]}}}}"#;

    fn gateway() -> StripeAgnosticGateway {
        StripeAgnosticGateway::new("sk_test_123".to_string(), WEBHOOK_SECRET.to_string())
    }

    fn signature_header(timestamp: i64, payload: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(WEBHOOK_SECRET.as_bytes()).unwrap();
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        format!(
            "t={},v1={}",
            timestamp,
            hex::encode(mac.finalize().into_bytes())
        )
    }

    fn webhook_headers(signature: String) -> Vec<(String, String)> {
        vec![("Stripe-Signature".to_string(), signature)]
    }

    #[tokio::test]
    async fn webhook_accepts_valid_raw_body_signature() {
        let timestamp = chrono::Utc::now().timestamp();
        let event = gateway()
            .handle_webhook(
                WEBHOOK_PAYLOAD,
                &webhook_headers(signature_header(timestamp, WEBHOOK_PAYLOAD)),
            )
            .await
            .unwrap();

        assert_eq!(event.event_type, WebhookEventType::PaymentSucceeded);
        assert_eq!(event.payment_id, "pi_test");
        assert_eq!(event.transaction_id.as_deref(), Some("ch_test"));
    }

    #[tokio::test]
    async fn webhook_rejects_missing_signature() {
        let error = gateway()
            .handle_webhook(WEBHOOK_PAYLOAD, &[])
            .await
            .unwrap_err();
        assert!(matches!(error, crate::Error::Validation(_)));
    }

    #[tokio::test]
    async fn webhook_rejects_forged_signature() {
        let timestamp = chrono::Utc::now().timestamp();
        let forged = format!("t={},v1={}", timestamp, "00".repeat(32));
        let error = gateway()
            .handle_webhook(WEBHOOK_PAYLOAD, &webhook_headers(forged))
            .await
            .unwrap_err();
        assert!(matches!(error, crate::Error::Validation(_)));
    }

    #[tokio::test]
    async fn webhook_rejects_tampered_raw_body() {
        let timestamp = chrono::Utc::now().timestamp();
        let signature = signature_header(timestamp, WEBHOOK_PAYLOAD);
        let tampered = String::from_utf8(WEBHOOK_PAYLOAD.to_vec())
            .unwrap()
            .replace("pi_test", "pi_other");
        let error = gateway()
            .handle_webhook(tampered.as_bytes(), &webhook_headers(signature))
            .await
            .unwrap_err();
        assert!(matches!(error, crate::Error::Validation(_)));
    }

    #[tokio::test]
    async fn webhook_rejects_stale_and_malformed_signatures() {
        let now = chrono::Utc::now().timestamp();
        let stale = signature_header(now - 3600, WEBHOOK_PAYLOAD);
        let error = gateway()
            .handle_webhook(WEBHOOK_PAYLOAD, &webhook_headers(stale))
            .await
            .unwrap_err();
        assert!(matches!(error, crate::Error::Validation(_)));

        let malformed = vec![
            "not-a-signature".to_string(),
            format!("v1={}", "00".repeat(32)),
            format!("t=not-a-time,v1={}", "00".repeat(32)),
            format!("t={},v1=not-hex", now),
            format!("t={},v1=00", now),
            format!("t={}", now),
        ];
        for signature in malformed {
            let error = gateway()
                .handle_webhook(WEBHOOK_PAYLOAD, &webhook_headers(signature))
                .await
                .unwrap_err();
            assert!(matches!(error, crate::Error::Validation(_)));
        }
    }

    #[tokio::test]
    async fn direct_partial_charge_refunded_uses_actual_refund_identity_and_amount() {
        let timestamp = chrono::Utc::now().timestamp();
        let event = gateway()
            .handle_webhook(
                CHARGE_REFUNDED_PARTIAL_PAYLOAD,
                &webhook_headers(signature_header(timestamp, CHARGE_REFUNDED_PARTIAL_PAYLOAD)),
            )
            .await
            .unwrap();

        assert_eq!(event.event_type, WebhookEventType::PaymentRefunded);
        assert_eq!(event.payment_id, "pi_direct_partial");
        assert_eq!(event.transaction_id.as_deref(), Some("re_direct_partial"));
        assert_ne!(event.transaction_id.as_deref(), Some("pi_direct_partial"));
        assert_ne!(event.transaction_id.as_deref(), Some("ch_full_amount"));
        assert_eq!(event.data["id"], "re_direct_partial");
        assert_eq!(event.data["currency"], "usd");
        assert_eq!(event.data["status"], "succeeded");
        let amount = event.data["amount"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap();
        assert_eq!(amount, dec!(25.00));
        assert_ne!(amount, dec!(100.00));
        assert_ne!(amount, dec!(10000));
        assert!(event.data.get("refunds").is_none());
    }

    #[tokio::test]
    async fn charge_refunded_without_refund_object_is_rejected() {
        let payload = br#"{"id":"evt_bad_refund","type":"charge.refunded","data":{"object":{"id":"ch_only","payment_intent":"pi_only","amount":10000,"currency":"usd","refunds":{"data":[]}}}}"#;
        let timestamp = chrono::Utc::now().timestamp();
        let error = gateway()
            .handle_webhook(
                payload,
                &webhook_headers(signature_header(timestamp, payload)),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, crate::Error::Validation(_)));
    }

    #[test]
    fn stripe_minor_units_respect_zero_decimal_and_decimal_currencies() {
        assert_eq!(
            stripe_amount_to_minor_units(dec!(1234), "JPY").unwrap(),
            1234
        );
        assert_eq!(
            stripe_amount_to_minor_units(dec!(1234), "jpy").unwrap(),
            1234
        );
        assert_eq!(
            stripe_amount_to_minor_units(dec!(12.34), "USD").unwrap(),
            1234
        );
        assert_eq!(
            stripe_amount_to_minor_units(dec!(12.34), "EUR").unwrap(),
            1234
        );
    }

    #[test]
    fn stripe_responses_use_the_same_currency_minor_unit_rules() {
        assert_eq!(stripe_amount_from_minor_units(1234, "JPY"), dec!(1234));
        assert_eq!(stripe_amount_from_minor_units(1234, "usd"), dec!(12.34));
    }

    #[test]
    fn stripe_minor_unit_overflow_is_a_validation_error() {
        for currency in ["USD", "JPY"] {
            let error = stripe_amount_to_minor_units(Decimal::MAX, currency).unwrap_err();
            assert!(matches!(error, crate::Error::Validation(_)));
        }
    }

    #[tokio::test]
    async fn initiate_rejects_amount_overflow_before_calling_stripe() {
        let error = gateway()
            .create_payment_intent(
                Decimal::MAX,
                "USD",
                "pm_test",
                "customer@example.com",
                "test",
                json!({}),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, crate::Error::Validation(_)));

        let error = gateway()
            .create_payment_intent_with_token(
                Decimal::MAX,
                "JPY",
                "pm_test",
                "customer@example.com",
                "test",
                json!({}),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, crate::Error::Validation(_)));
    }
}
