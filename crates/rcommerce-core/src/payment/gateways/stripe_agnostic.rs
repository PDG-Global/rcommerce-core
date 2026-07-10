//! Stripe Payment Gateway - Agnostic Implementation
//!
//! Server-to-server implementation that handles card data securely without exposing keys to frontend.

use async_trait::async_trait;
use reqwest;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use serde_json::json;

use crate::payment::agnostic::*;
use crate::Result;

pub struct StripeAgnosticGateway {
    api_key: String,
    #[allow(dead_code)]
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
        let amount_in_cents: i64 = (amount * dec!(100)).try_into().unwrap_or(0i64);

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
            PaymentMethodData::CardToken { token } => {
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

        let intent: StripePaymentIntent = response
            .json()
            .await
            .map_err(|e| crate::Error::payment_error(format!("Failed to parse response: {}", e)))?;

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
            let amount_in_cents: i64 = (amt * dec!(100)).try_into().unwrap_or(0i64);
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
            amount: Decimal::from(refund.amount) / dec!(100),
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
        // Verify webhook signature
        let _signature = headers
            .iter()
            .find(|(k, _)| k == "Stripe-Signature")
            .map(|(_, v)| v.as_str())
            .unwrap_or("");

        // TODO: Implement proper signature verification
        // For now, just parse the event

        let event: StripeWebhookEvent = serde_json::from_slice(payload)
            .map_err(|e| crate::Error::validation(format!("Invalid webhook payload: {}", e)))?;

        let (event_type, payment_id, transaction_id) = match event.type_.as_str() {
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
            ),
            "charge.refunded" => (
                WebhookEventType::PaymentRefunded,
                event
                    .data
                    .object
                    .get("payment_intent")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                event
                    .data
                    .object
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            ),
            _ => return Err(crate::Error::validation("Unsupported webhook event type")),
        };

        Ok(WebhookEvent {
            event_type,
            payment_id,
            transaction_id,
            data: event.data.object,
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
            .map(|pm| self.extract_card_info(pm))
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
    async fn create_payment_intent_with_token(
        &self,
        amount: Decimal,
        currency: &str,
        token: &str,
        customer_email: &str,
        description: &str,
        metadata: serde_json::Value,
    ) -> Result<StripePaymentIntent> {
        let amount_in_cents: i64 = (amount * dec!(100)).try_into().unwrap_or(0i64);

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

fn map_refund_reason(reason: &str) -> String {
    match reason {
        "duplicate" => "duplicate".to_string(),
        "fraudulent" => "fraudulent".to_string(),
        "requested_by_customer" => "requested_by_customer".to_string(),
        _ => "requested_by_customer".to_string(),
    }
}
