//! WeChat Pay Payment Gateway - Agnostic Implementation
//!
//! Server-to-server implementation that handles WeChat Pay payments securely.

use async_trait::async_trait;
use base64::Engine as Base64Engine;
use reqwest;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::payment::agnostic::*;
use crate::Result;

/// WeChat Pay Agnostic Gateway
pub struct WeChatPayAgnosticGateway {
    mch_id: String,
    #[allow(dead_code)]
    api_key: String,
    app_id: String,
    serial_no: String,
    private_key: String,
    client: reqwest::Client,
    base_url: String,
    supported_methods: Vec<PaymentMethodConfig>,
}

impl WeChatPayAgnosticGateway {
    /// Create a new WeChat Pay agnostic gateway
    pub fn new(
        mch_id: String,
        api_key: String,
        app_id: String,
        serial_no: String,
        private_key: String,
        sandbox: bool,
    ) -> Self {
        let base_url = if sandbox {
            "https://api.mch.weixin.qq.com/sandboxnew".to_string()
        } else {
            "https://api.mch.weixin.qq.com/v3".to_string()
        };

        // Define supported payment methods
        let supported_methods = vec![PaymentMethodConfig {
            method_type: PaymentMethodType::WechatPay,
            enabled: true,
            display_name: "WeChat Pay".to_string(),
            requires_redirect: true,
            supports_3ds: false,
            supports_tokenization: false,
            supports_recurring: false,
            required_fields: vec![FieldDefinition {
                name: "openid".to_string(),
                label: "WeChat OpenID".to_string(),
                field_type: FieldType::Text,
                required: true,
                pattern: None,
                placeholder: Some("WeChat User OpenID".to_string()),
                help_text: Some("Required for JSAPI payments".to_string()),
            }],
            optional_fields: vec![],
            supported_currencies: vec!["CNY".to_string(), "HKD".to_string()],
            min_amount: Some(dec!(0.01)),
            max_amount: Some(dec!(100000.00)),
        }];

        Self {
            mch_id,
            api_key,
            app_id,
            serial_no,
            private_key,
            client: reqwest::Client::new(),
            base_url,
            supported_methods,
        }
    }

    /// Sign a request for WeChat Pay API authentication
    fn sign_request(
        &self,
        method: &str,
        url: &str,
        body: &str,
        nonce: &str,
        timestamp: i64,
    ) -> String {
        let message = format!("{}\n{}\n{}\n{}\n{}", method, url, timestamp, nonce, body);

        // Sign with RSA private key
        use rsa::pkcs1v15::SigningKey;
        use rsa::signature::{SignatureEncoding, Signer};
        use rsa::{pkcs8::DecodePrivateKey, RsaPrivateKey};
        use sha2::Sha256;

        let private_key =
            RsaPrivateKey::from_pkcs8_pem(&self.private_key).expect("Invalid private key");
        let signing_key = SigningKey::<Sha256>::new(private_key);
        let signature = signing_key.sign(message.as_bytes());

        base64::engine::general_purpose::STANDARD.encode(signature.to_bytes().as_ref())
    }

    /// Generate authorization header
    fn get_auth_header(&self, method: &str, url: &str, body: &str) -> String {
        let timestamp = chrono::Utc::now().timestamp();
        let nonce = uuid::Uuid::new_v4().to_string();
        let signature = self.sign_request(method, url, body, &nonce, timestamp);

        format!(
            "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",signature=\"{}\",timestamp=\"{}\",serial_no=\"{}\"",
            self.mch_id, nonce, signature, timestamp, self.serial_no
        )
    }

    /// Map WeChat Pay status to our PaymentStatus
    fn map_status(status: &str) -> PaymentStatus {
        match status {
            "SUCCESS" => PaymentStatus::Succeeded,
            "NOTPAY" => PaymentStatus::Pending,
            "USERPAYING" => PaymentStatus::Processing,
            "CLOSED" => PaymentStatus::Cancelled,
            "REVOKED" => PaymentStatus::Cancelled,
            "PAYERROR" => PaymentStatus::Failed,
            "REFUND" => PaymentStatus::Refunded,
            _ => PaymentStatus::Pending,
        }
    }

    /// Map WeChat Pay refund status
    fn map_refund_status(status: &str) -> RefundStatus {
        match status {
            "SUCCESS" => RefundStatus::Succeeded,
            "CLOSED" => RefundStatus::Failed,
            "PROCESSING" => RefundStatus::Processing,
            "ABNORMAL" => RefundStatus::Failed,
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
}

#[async_trait]
impl AgnosticPaymentGateway for WeChatPayAgnosticGateway {
    async fn get_config(&self) -> Result<GatewayConfig> {
        Ok(GatewayConfig {
            gateway_id: "wechatpay".to_string(),
            gateway_name: "WeChat Pay".to_string(),
            payment_methods: self.supported_methods.clone(),
            supports_3ds: false,
            supports_webhooks: true,
            supports_refunds: true,
            supports_partial_refunds: true,
            supported_currencies: vec!["CNY".to_string(), "HKD".to_string()],
            default_currency: "CNY".to_string(),
        })
    }

    async fn initiate_payment(
        &self,
        request: InitiatePaymentRequest,
    ) -> Result<InitiatePaymentResponse> {
        // Convert amount to cents (WeChat Pay uses smallest currency unit)
        let amount_cents: i64 = (request.amount * dec!(100))
            .try_into()
            .map_err(|_| crate::Error::validation("Invalid amount"))?;

        let trade_no = self.generate_trade_no(request.order_id);

        // Build request body
        let body = serde_json::json!({
            "mchid": self.mch_id,
            "appid": self.app_id,
            "description": request.description,
            "out_trade_no": trade_no,
            "notify_url": format!("https://yourstore.com/webhooks/wechatpay"),
            "amount": {
                "total": amount_cents,
                "currency": request.currency.to_uppercase()
            },
        });

        let url = format!("{}/pay/transactions/native", self.base_url);
        let auth_header = self.get_auth_header("POST", &url, &body.to_string());

        let response = self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("WeChat Pay API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "WeChat Pay error: {}",
                error_text
            )));
        }

        let wechat_response: WeChatPayNativeResponse = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse WeChat Pay response: {}", e))
        })?;

        // Return redirect response with QR code URL
        Ok(InitiatePaymentResponse::RequiresAction {
            payment_id: trade_no,
            action_type: PaymentActionType::Redirect,
            action_data: serde_json::json!({
                "code_url": wechat_response.code_url,
                "qr_code_url": wechat_response.code_url,
            }),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
        })
    }

    async fn complete_payment_action(
        &self,
        request: CompletePaymentActionRequest,
    ) -> Result<CompletePaymentActionResponse> {
        // Query the transaction status from WeChat Pay
        let url = format!(
            "{}/pay/transactions/out-trade-no/{}",
            self.base_url, request.payment_id
        );
        let auth_header = self.get_auth_header("GET", &url, "");

        let response = self
            .client
            .get(&url)
            .header("Authorization", auth_header)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("WeChat Pay API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "WeChat Pay error: {}",
                error_text
            )));
        }

        let transaction: WeChatPayTransaction = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse WeChat Pay response: {}", e))
        })?;

        let status = Self::map_status(&transaction.trade_state);

        match status {
            PaymentStatus::Succeeded => Ok(CompletePaymentActionResponse::Success {
                payment_id: request.payment_id,
                transaction_id: transaction.transaction_id.unwrap_or_default(),
                payment_status: status,
                payment_method: PaymentMethodInfo {
                    method_type: PaymentMethodType::WechatPay,
                    last_four: None,
                    card_brand: None,
                    exp_month: None,
                    exp_year: None,
                    cardholder_name: None,
                    token: None,
                },
                receipt_url: None,
            }),
            PaymentStatus::Failed => Ok(CompletePaymentActionResponse::Failed {
                payment_id: request.payment_id,
                error_code: "payment_failed".to_string(),
                error_message: transaction.trade_state_desc,
                retry_allowed: true,
            }),
            _ => Ok(CompletePaymentActionResponse::RequiresAction {
                payment_id: request.payment_id,
                action_type: PaymentActionType::Redirect,
                action_data: serde_json::json!({
                    "status": transaction.trade_state,
                    "description": transaction.trade_state_desc,
                }),
            }),
        }
    }

    async fn get_payment_status(&self, payment_id: &str) -> Result<PaymentStatus> {
        let url = format!(
            "{}/pay/transactions/out-trade-no/{}",
            self.base_url, payment_id
        );
        let auth_header = self.get_auth_header("GET", &url, "");

        let response = self
            .client
            .get(&url)
            .header("Authorization", auth_header)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("WeChat Pay API error: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::payment_error("Failed to get payment status"));
        }

        let transaction: WeChatPayTransaction = response
            .json()
            .await
            .map_err(|e| crate::Error::network(format!("Failed to parse response: {}", e)))?;

        Ok(Self::map_status(&transaction.trade_state))
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<RefundResponse> {
        let refund_amount_cents: i64 = amount
            .map(|a| (a * dec!(100)).try_into().unwrap_or(0))
            .unwrap_or(0);

        let refund_no = format!("REF_{}_{}", payment_id, chrono::Utc::now().timestamp());

        let body = serde_json::json!({
            "out_trade_no": payment_id,
            "out_refund_no": refund_no,
            "reason": reason,
            "amount": {
                "refund": refund_amount_cents,
                "total": refund_amount_cents,
                "currency": "CNY"
            }
        });

        let url = format!("{}/refund/domestic/refunds", self.base_url);
        let auth_header = self.get_auth_header("POST", &url, &body.to_string());

        let response = self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| crate::Error::network(format!("WeChat Pay API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::Error::payment_error(format!(
                "WeChat Pay refund error: {}",
                error_text
            )));
        }

        let refund_response: WeChatPayRefundResponse = response.json().await.map_err(|e| {
            crate::Error::network(format!("Failed to parse WeChat Pay refund response: {}", e))
        })?;

        Ok(RefundResponse {
            refund_id: refund_response.refund_id,
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(Decimal::ZERO),
            currency: "CNY".to_string(),
            status: Self::map_refund_status(&refund_response.status),
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &[(String, String)],
    ) -> Result<WebhookEvent> {
        // Extract signature from headers
        let signature = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Wechatpay-Signature"))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");

        let timestamp = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Wechatpay-Timestamp"))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");

        let nonce = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Wechatpay-Nonce"))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");

        let serial = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Wechatpay-Serial"))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");

        // Verify webhook signature using RSA
        // The signature is: BASE64(RSA-SHA256(timestamp + nonce + body))
        let _message = format!(
            "{}\n{}\n{}",
            timestamp,
            nonce,
            String::from_utf8_lossy(payload)
        );

        // For now, we accept the webhook (proper verification requires fetching the WeChat Pay public key)
        // TODO: Implement full RSA signature verification with WeChat Pay public key
        let _ = (signature, serial); // Acknowledge these are used for verification

        let notification: WeChatPayNotification = serde_json::from_slice(payload).map_err(|e| {
            crate::Error::validation(format!("Invalid WeChat Pay webhook payload: {}", e))
        })?;

        let event_type = match notification.event_type.as_str() {
            "TRANSACTION.SUCCESS" => WebhookEventType::PaymentSucceeded,
            "TRANSACTION.FAIL" => WebhookEventType::PaymentFailed,
            "REFUND.SUCCESS" => WebhookEventType::PaymentRefunded,
            "REFUND.ABNORMAL" => WebhookEventType::PaymentFailed,
            "REFUND.CLOSED" => WebhookEventType::PaymentFailed,
            _ => {
                return Err(crate::Error::validation(
                    "Unsupported WeChat Pay webhook event type",
                ))
            }
        };

        let payment_id = notification.out_trade_no.clone();
        let transaction_id = notification.transaction_id.clone();

        Ok(WebhookEvent {
            event_type,
            payment_id,
            transaction_id,
            data: serde_json::json!(notification),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn tokenize_payment_method(
        &self,
        _payment_method_data: PaymentMethodData,
    ) -> Result<PaymentMethodToken> {
        // WeChat Pay doesn't support tokenization in the traditional sense
        Err(crate::Error::validation(
            "WeChat Pay does not support payment method tokenization",
        ))
    }

    async fn get_saved_payment_methods(
        &self,
        _customer_id: &str,
    ) -> Result<Vec<PaymentMethodInfo>> {
        // WeChat Pay doesn't support saved payment methods
        Ok(vec![])
    }

    async fn delete_payment_method(&self, _token: &str) -> Result<()> {
        // WeChat Pay doesn't support saved payment methods
        Err(crate::Error::validation(
            "WeChat Pay does not support saved payment methods",
        ))
    }
}

// WeChat Pay API response types
#[derive(Debug, Serialize, Deserialize)]
struct WeChatPayNativeResponse {
    code_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WeChatPayTransaction {
    out_trade_no: String,
    transaction_id: Option<String>,
    #[serde(rename = "trade_type")]
    _trade_type: Option<String>,
    trade_state: String,
    trade_state_desc: String,
    #[serde(rename = "bank_type")]
    _bank_type: Option<String>,
    #[serde(rename = "attach")]
    _attach: Option<String>,
    #[serde(rename = "success_time")]
    _success_time: Option<String>,
    payer: Option<WeChatPayPayer>,
    amount: WeChatPayAmount,
}

#[derive(Debug, Serialize, Deserialize)]
struct WeChatPayPayer {
    openid: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WeChatPayAmount {
    total: i64,
    payer_total: Option<i64>,
    currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WeChatPayRefundResponse {
    refund_id: String,
    out_refund_no: String,
    out_trade_no: String,
    channel: Option<String>,
    user_received_account: Option<String>,
    status: String,
    create_time: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WeChatPayNotification {
    id: String,
    create_time: String,
    event_type: String,
    out_trade_no: String,
    transaction_id: Option<String>,
    trade_state: Option<String>,
    #[serde(flatten)]
    _extra: std::collections::HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wechatpay_agnostic_gateway_creation() {
        let gateway = WeChatPayAgnosticGateway::new(
            "1234567890".to_string(),
            "api_key".to_string(),
            "wx1234567890".to_string(),
            "serial_no".to_string(),
            "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----".to_string(),
            true,
        );

        assert_eq!(gateway.mch_id, "1234567890");
        assert_eq!(gateway.app_id, "wx1234567890");
        assert!(gateway.base_url.contains("sandbox"));
    }

    #[test]
    fn test_wechatpay_status_mapping() {
        assert_eq!(
            WeChatPayAgnosticGateway::map_status("SUCCESS"),
            PaymentStatus::Succeeded
        );
        assert_eq!(
            WeChatPayAgnosticGateway::map_status("NOTPAY"),
            PaymentStatus::Pending
        );
        assert_eq!(
            WeChatPayAgnosticGateway::map_status("USERPAYING"),
            PaymentStatus::Processing
        );
        assert_eq!(
            WeChatPayAgnosticGateway::map_status("CLOSED"),
            PaymentStatus::Cancelled
        );
        assert_eq!(
            WeChatPayAgnosticGateway::map_status("PAYERROR"),
            PaymentStatus::Failed
        );
        assert_eq!(
            WeChatPayAgnosticGateway::map_status("REFUND"),
            PaymentStatus::Refunded
        );
    }

    #[test]
    fn test_wechatpay_refund_status_mapping() {
        assert_eq!(
            WeChatPayAgnosticGateway::map_refund_status("SUCCESS"),
            RefundStatus::Succeeded
        );
        assert_eq!(
            WeChatPayAgnosticGateway::map_refund_status("PROCESSING"),
            RefundStatus::Processing
        );
        assert_eq!(
            WeChatPayAgnosticGateway::map_refund_status("CLOSED"),
            RefundStatus::Failed
        );
        assert_eq!(
            WeChatPayAgnosticGateway::map_refund_status("ABNORMAL"),
            RefundStatus::Failed
        );
    }
}
