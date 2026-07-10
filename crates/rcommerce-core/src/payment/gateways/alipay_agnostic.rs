//! AliPay Payment Gateway - Agnostic Implementation
//!
//! Server-to-server implementation that handles AliPay payments securely.

use async_trait::async_trait;
use base64::Engine as Base64Engine;
use reqwest;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::payment::agnostic::*;
use crate::Result;

/// AliPay Agnostic Gateway
pub struct AliPayAgnosticGateway {
    app_id: String,
    private_key: String,
    alipay_public_key: String,
    client: reqwest::Client,
    gateway_url: String,
    sign_type: String,
    format: String,
    charset: String,
    version: String,
    supported_methods: Vec<PaymentMethodConfig>,
}

impl AliPayAgnosticGateway {
    /// Create a new AliPay agnostic gateway
    pub fn new(
        app_id: String,
        private_key: String,
        alipay_public_key: String,
        sandbox: bool,
    ) -> Self {
        let gateway_url = if sandbox {
            "https://openapi.alipaydev.com/gateway.do".to_string()
        } else {
            "https://openapi.alipay.com/gateway.do".to_string()
        };

        // Define supported payment methods
        let supported_methods = vec![PaymentMethodConfig {
            method_type: PaymentMethodType::Alipay,
            enabled: true,
            display_name: "AliPay".to_string(),
            requires_redirect: true,
            supports_3ds: false,
            supports_tokenization: false,
            supports_recurring: false,
            required_fields: vec![FieldDefinition {
                name: "return_url".to_string(),
                label: "Return URL".to_string(),
                field_type: FieldType::Text,
                required: true,
                pattern: None,
                placeholder: Some("https://yourstore.com/payment/return".to_string()),
                help_text: Some("URL to redirect after payment".to_string()),
            }],
            optional_fields: vec![],
            supported_currencies: vec!["CNY".to_string(), "USD".to_string()],
            min_amount: Some(dec!(0.01)),
            max_amount: Some(dec!(1000000.00)),
        }];

        Self {
            app_id,
            private_key,
            alipay_public_key,
            client: reqwest::Client::new(),
            gateway_url,
            sign_type: "RSA2".to_string(),
            format: "JSON".to_string(),
            charset: "utf-8".to_string(),
            version: "1.0".to_string(),
            supported_methods,
        }
    }

    /// Generate a signature for AliPay API requests (RSA2)
    fn generate_signature(&self, params: &HashMap<String, String>) -> String {
        // Sort parameters alphabetically
        let mut sorted_keys: Vec<&String> = params.keys().collect();
        sorted_keys.sort();

        // Build query string (key=value&key2=value2), excluding sign and sign_type
        let query_string = sorted_keys
            .iter()
            .filter(|&&k| k != "sign" && k != "sign_type")
            .map(|k| format!("{}={}", k, params.get(*k).unwrap_or(&String::new())))
            .collect::<Vec<_>>()
            .join("&");

        // Sign with RSA2 (SHA256)
        use rsa::pkcs1v15::SigningKey;
        use rsa::signature::{SignatureEncoding, Signer};
        use rsa::{pkcs8::DecodePrivateKey, RsaPrivateKey};
        use sha2::Sha256;

        let private_key =
            RsaPrivateKey::from_pkcs8_pem(&self.private_key).expect("Invalid private key");
        let signing_key = SigningKey::<Sha256>::new(private_key);
        let signature = signing_key.sign(query_string.as_bytes());

        base64::engine::general_purpose::STANDARD.encode(signature.to_bytes().as_ref())
    }

    /// Verify AliPay response signature (RSA2)
    fn verify_signature(&self, params: &HashMap<String, String>, signature: &str) -> bool {
        // Build the string to verify (all params except sign and sign_type, sorted alphabetically)
        let mut sorted_keys: Vec<&String> = params.keys().collect();
        sorted_keys.sort();

        let query_string = sorted_keys
            .iter()
            .filter(|&&k| k != "sign" && k != "sign_type")
            .map(|k| format!("{}={}", k, params.get(*k).unwrap_or(&String::new())))
            .collect::<Vec<_>>()
            .join("&");

        // Verify with RSA public key
        use rsa::pkcs1v15::VerifyingKey;
        use rsa::signature::Verifier;
        use rsa::{pkcs8::DecodePublicKey, RsaPublicKey};
        use sha2::Sha256;

        let public_key = match RsaPublicKey::from_public_key_pem(&self.alipay_public_key) {
            Ok(key) => key,
            Err(_) => return false,
        };

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);

        let signature_bytes = match base64::engine::general_purpose::STANDARD.decode(signature) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        match rsa::pkcs1v15::Signature::try_from(signature_bytes.as_slice()) {
            Ok(sig) => verifying_key.verify(query_string.as_bytes(), &sig).is_ok(),
            Err(_) => false,
        }
    }

    /// Build common parameters for all requests
    fn build_common_params(&self, method: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();
        params.insert("app_id".to_string(), self.app_id.clone());
        params.insert("method".to_string(), method.to_string());
        params.insert("format".to_string(), self.format.clone());
        params.insert("charset".to_string(), self.charset.clone());
        params.insert("sign_type".to_string(), self.sign_type.clone());
        params.insert(
            "timestamp".to_string(),
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        );
        params.insert("version".to_string(), self.version.clone());
        params
    }

    /// Map AliPay trade status to our PaymentStatus
    fn map_status(status: &str) -> PaymentStatus {
        match status {
            "TRADE_SUCCESS" => PaymentStatus::Succeeded,
            "TRADE_FINISHED" => PaymentStatus::Succeeded,
            "TRADE_CLOSED" => PaymentStatus::Cancelled,
            "WAIT_BUYER_PAY" => PaymentStatus::Pending,
            "TRADE_PENDING" => PaymentStatus::Processing,
            _ => PaymentStatus::Pending,
        }
    }

    /// Map AliPay refund status
    #[allow(dead_code)]
    fn map_refund_status(status: &str) -> RefundStatus {
        match status {
            "REFUND_SUCCESS" => RefundStatus::Succeeded,
            "REFUND_CLOSED" => RefundStatus::Failed,
            "REFUND_PROCESSING" => RefundStatus::Processing,
            _ => RefundStatus::Pending,
        }
    }

    /// Generate a unique out_trade_no
    fn generate_trade_no(&self, order_id: uuid::Uuid) -> String {
        format!(
            "{}_{}",
            order_id.to_string().replace("-", ""),
            chrono::Utc::now().timestamp()
        )
    }

    /// Parse API response
    fn parse_response<T: serde::de::DeserializeOwned>(
        &self,
        response_text: &str,
        method: &str,
    ) -> Result<T> {
        let response_key = format!("{}_response", method.replace('.', "_"));

        let value: serde_json::Value = serde_json::from_str(response_text).map_err(|e| {
            crate::Error::network(format!("Failed to parse AliPay response: {}", e))
        })?;

        let response_data = value
            .get(&response_key)
            .ok_or_else(|| crate::Error::network("Missing response data from AliPay"))?;

        serde_json::from_value(response_data.clone()).map_err(|e| {
            crate::Error::network(format!("Failed to parse AliPay response data: {}", e))
        })
    }
}

#[async_trait]
impl AgnosticPaymentGateway for AliPayAgnosticGateway {
    async fn get_config(&self) -> Result<GatewayConfig> {
        Ok(GatewayConfig {
            gateway_id: "alipay".to_string(),
            gateway_name: "AliPay".to_string(),
            payment_methods: self.supported_methods.clone(),
            supports_3ds: false,
            supports_webhooks: true,
            supports_refunds: true,
            supports_partial_refunds: true,
            supported_currencies: vec!["CNY".to_string(), "USD".to_string()],
            default_currency: "CNY".to_string(),
        })
    }

    async fn initiate_payment(
        &self,
        request: InitiatePaymentRequest,
    ) -> Result<InitiatePaymentResponse> {
        let trade_no = self.generate_trade_no(request.order_id);

        // Extract return URL from payment method data if available
        let return_url = match &request.payment_method_data {
            PaymentMethodData::Redirect { return_url, .. } => return_url.clone(),
            _ => "https://yourstore.com/checkout/success".to_string(),
        };

        // Build business parameters
        let biz_content = serde_json::json!({
            "out_trade_no": trade_no,
            "total_amount": request.amount.to_string(),
            "subject": request.description,
            "product_code": "FAST_INSTANT_TRADE_PAY",
            "body": format!("Payment for order {}", request.order_id),
        });

        // Build request parameters
        let mut params = self.build_common_params("alipay.trade.page.pay");
        params.insert(
            "notify_url".to_string(),
            "https://yourstore.com/webhooks/alipay".to_string(),
        );
        params.insert("return_url".to_string(), return_url);
        params.insert("biz_content".to_string(), biz_content.to_string());

        // Generate signature
        let sign = self.generate_signature(&params);
        params.insert("sign".to_string(), sign);

        // Build payment URL
        let query_string = params
            .iter()
            .map(|(k, v)| {
                let encoded_key =
                    url::form_urlencoded::byte_serialize(k.as_bytes()).collect::<String>();
                let encoded_value =
                    url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>();
                format!("{}={}", encoded_key, encoded_value)
            })
            .collect::<Vec<_>>()
            .join("&");

        let payment_url = format!("{}?{}", self.gateway_url, query_string);

        // Return redirect response
        Ok(InitiatePaymentResponse::RequiresAction {
            payment_id: trade_no,
            action_type: PaymentActionType::Redirect,
            action_data: serde_json::json!({
                "redirect_url": payment_url,
                "payment_url": payment_url,
            }),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
        })
    }

    async fn complete_payment_action(
        &self,
        request: CompletePaymentActionRequest,
    ) -> Result<CompletePaymentActionResponse> {
        // Query the trade status from AliPay
        let biz_content = serde_json::json!({
            "out_trade_no": request.payment_id,
        });

        let mut params = self.build_common_params("alipay.trade.query");
        params.insert("biz_content".to_string(), biz_content.to_string());

        let sign = self.generate_signature(&params);
        params.insert("sign".to_string(), sign);

        let response = self
            .client
            .post(&self.gateway_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("AliPay API error: {}", e)))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| crate::Error::network(format!("Failed to read AliPay response: {}", e)))?;

        let trade_response: AliPayTradeQueryResponse =
            self.parse_response(&response_text, "alipay.trade.query")?;

        if trade_response.code != "10000" {
            return Err(crate::Error::payment_error(format!(
                "AliPay query failed: {} - {}",
                trade_response.sub_code.unwrap_or_default(),
                trade_response.sub_msg.unwrap_or_default()
            )));
        }

        let status = trade_response
            .trade_status
            .as_ref()
            .map(|s| Self::map_status(s))
            .unwrap_or(PaymentStatus::Pending);

        match status {
            PaymentStatus::Succeeded => Ok(CompletePaymentActionResponse::Success {
                payment_id: request.payment_id,
                transaction_id: trade_response.trade_no.unwrap_or_default(),
                payment_status: status,
                payment_method: PaymentMethodInfo {
                    method_type: PaymentMethodType::Alipay,
                    last_four: None,
                    card_brand: None,
                    exp_month: None,
                    exp_year: None,
                    cardholder_name: trade_response.buyer_logon_id,
                    token: None,
                },
                receipt_url: None,
            }),
            PaymentStatus::Failed | PaymentStatus::Cancelled => {
                Ok(CompletePaymentActionResponse::Failed {
                    payment_id: request.payment_id,
                    error_code: trade_response
                        .sub_code
                        .unwrap_or_else(|| "payment_failed".to_string()),
                    error_message: trade_response
                        .sub_msg
                        .unwrap_or_else(|| "Payment failed".to_string()),
                    retry_allowed: true,
                })
            }
            _ => Ok(CompletePaymentActionResponse::RequiresAction {
                payment_id: request.payment_id,
                action_type: PaymentActionType::Redirect,
                action_data: serde_json::json!({
                    "status": trade_response.trade_status,
                }),
            }),
        }
    }

    async fn get_payment_status(&self, payment_id: &str) -> Result<PaymentStatus> {
        let biz_content = serde_json::json!({
            "out_trade_no": payment_id,
        });

        let mut params = self.build_common_params("alipay.trade.query");
        params.insert("biz_content".to_string(), biz_content.to_string());

        let sign = self.generate_signature(&params);
        params.insert("sign".to_string(), sign);

        let response = self
            .client
            .post(&self.gateway_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("AliPay API error: {}", e)))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| crate::Error::network(format!("Failed to read AliPay response: {}", e)))?;

        let trade_response: AliPayTradeQueryResponse =
            self.parse_response(&response_text, "alipay.trade.query")?;

        if trade_response.code != "10000" {
            return Err(crate::Error::payment_error("Failed to get payment status"));
        }

        Ok(trade_response
            .trade_status
            .map(|s| Self::map_status(&s))
            .unwrap_or(PaymentStatus::Pending))
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<RefundResponse> {
        let refund_amount = amount.map(|a| a.to_string()).unwrap_or_default();
        let refund_no = format!("REF_{}_{}", payment_id, chrono::Utc::now().timestamp());

        let biz_content = serde_json::json!({
            "out_trade_no": payment_id,
            "refund_amount": refund_amount,
            "refund_reason": reason,
            "out_request_no": refund_no,
        });

        let mut params = self.build_common_params("alipay.trade.refund");
        params.insert("biz_content".to_string(), biz_content.to_string());

        let sign = self.generate_signature(&params);
        params.insert("sign".to_string(), sign);

        let response = self
            .client
            .post(&self.gateway_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("AliPay API error: {}", e)))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| crate::Error::network(format!("Failed to read AliPay response: {}", e)))?;

        let refund_response: AliPayRefundResponse =
            self.parse_response(&response_text, "alipay.trade.refund")?;

        if refund_response.code != "10000" {
            return Err(crate::Error::payment_error(format!(
                "AliPay refund failed: {} - {}",
                refund_response.sub_code.unwrap_or_default(),
                refund_response.sub_msg.unwrap_or_default()
            )));
        }

        Ok(RefundResponse {
            refund_id: refund_no,
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(Decimal::ZERO),
            currency: "CNY".to_string(),
            status: RefundStatus::Succeeded,
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn handle_webhook(
        &self,
        payload: &[u8],
        _headers: &[(String, String)],
    ) -> Result<WebhookEvent> {
        // Parse the form-urlencoded notification
        let notification_str = String::from_utf8_lossy(payload);
        let params: HashMap<String, String> = notification_str
            .split('&')
            .filter_map(|p| {
                let (key, value) = p.split_once('=')?;
                let decoded_value = url::form_urlencoded::parse(value.as_bytes())
                    .map(|(k, _)| k.to_string())
                    .next()?;
                Some((key.to_string(), decoded_value))
            })
            .collect();

        // Verify signature
        let signature = params.get("sign").cloned().unwrap_or_default();
        if !self.verify_signature(&params, &signature) {
            return Err(crate::Error::validation("Invalid webhook signature"));
        }

        let trade_status = params.get("trade_status").map(|s| s.as_str()).unwrap_or("");
        let out_trade_no = params.get("out_trade_no").cloned().unwrap_or_default();
        let trade_no = params.get("trade_no").cloned();

        let event_type = match trade_status {
            "TRADE_SUCCESS" | "TRADE_FINISHED" => WebhookEventType::PaymentSucceeded,
            "TRADE_CLOSED" => WebhookEventType::PaymentFailed,
            "WAIT_BUYER_PAY" => WebhookEventType::PaymentPending,
            _ => WebhookEventType::PaymentFailed,
        };

        Ok(WebhookEvent {
            event_type,
            payment_id: out_trade_no,
            transaction_id: trade_no,
            data: serde_json::json!(params),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn tokenize_payment_method(
        &self,
        _payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken> {
        // AliPay doesn't support tokenization in the traditional sense
        Err(crate::Error::validation(
            "AliPay does not support payment method tokenization",
        ))
    }

    async fn get_saved_payment_methods(
        &self,
        _customer_id: &str,
    ) -> Result<Vec<PaymentMethodInfo>> {
        // AliPay doesn't support saved payment methods
        Ok(vec![])
    }

    async fn delete_payment_method(&self, _token: &str) -> Result<()> {
        // AliPay doesn't support saved payment methods
        Err(crate::Error::validation(
            "AliPay does not support saved payment methods",
        ))
    }
}

// AliPay API response types
#[derive(Debug, Serialize, Deserialize)]
struct AliPayTradeQueryResponse {
    code: String,
    msg: String,
    sub_code: Option<String>,
    sub_msg: Option<String>,
    trade_no: Option<String>,
    out_trade_no: Option<String>,
    buyer_logon_id: Option<String>,
    trade_status: Option<String>,
    total_amount: Option<String>,
    receipt_amount: Option<String>,
    buyer_pay_amount: Option<String>,
    point_amount: Option<String>,
    invoice_amount: Option<String>,
    send_pay_date: Option<String>,
    store_id: Option<String>,
    terminal_id: Option<String>,
    fund_bill_list: Option<String>,
    store_name: Option<String>,
    buyer_user_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AliPayRefundResponse {
    code: String,
    msg: String,
    sub_code: Option<String>,
    sub_msg: Option<String>,
    trade_no: Option<String>,
    out_trade_no: Option<String>,
    buyer_logon_id: Option<String>,
    fund_change: Option<String>,
    refund_fee: Option<String>,
    gmt_refund_pay: Option<String>,
    buyer_user_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alipay_agnostic_gateway_creation() {
        let gateway = AliPayAgnosticGateway::new(
            "2024XXXXXX".to_string(),
            "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----".to_string(),
            "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----".to_string(),
            true,
        );

        assert_eq!(gateway.app_id, "2024XXXXXX");
        assert_eq!(gateway.sign_type, "RSA2");
        assert!(gateway.gateway_url.contains("alipaydev"));
    }

    #[test]
    fn test_alipay_status_mapping() {
        assert_eq!(
            AliPayAgnosticGateway::map_status("TRADE_SUCCESS"),
            PaymentStatus::Succeeded
        );
        assert_eq!(
            AliPayAgnosticGateway::map_status("TRADE_FINISHED"),
            PaymentStatus::Succeeded
        );
        assert_eq!(
            AliPayAgnosticGateway::map_status("WAIT_BUYER_PAY"),
            PaymentStatus::Pending
        );
        assert_eq!(
            AliPayAgnosticGateway::map_status("TRADE_CLOSED"),
            PaymentStatus::Cancelled
        );
    }

    #[test]
    fn test_alipay_refund_status_mapping() {
        assert_eq!(
            AliPayAgnosticGateway::map_refund_status("REFUND_SUCCESS"),
            RefundStatus::Succeeded
        );
        assert_eq!(
            AliPayAgnosticGateway::map_refund_status("REFUND_PROCESSING"),
            RefundStatus::Processing
        );
        assert_eq!(
            AliPayAgnosticGateway::map_refund_status("REFUND_CLOSED"),
            RefundStatus::Failed
        );
    }
}
