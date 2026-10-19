//! AliPay gateway implementation
//!
//! AliPay (Alibaba's payment platform) is one of the world's largest mobile and online payment
//! platforms. It supports various payment scenarios including QR code payments, web payments,
//! mobile app payments, and face-to-face payments.
//!
//! ## API Documentation
//! - Official Docs: https://opendocs.alipay.com/
//! - Global Site: https://global.alipay.com/
//! - Sandbox Testing: https://opendocs.alipay.com/open/200/105311
//!
//! ## Supported Features
//! - PC Web Payments (网页支付)
//! - Mobile Web Payments (手机网站支付)
//! - App Payments (APP支付)
//! - QR Code Payments (扫码支付)
//! - Face-to-Face Payments (当面付)
//! - Refunds
//! - Transaction queries
//! - Webhook notifications (async notifications)
//! - Bill downloads

use async_trait::async_trait;
use base64::Engine as Base64Engine;
use reqwest;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::payment::{
    CreatePaymentRequest, Payment, PaymentGateway, PaymentSession, PaymentSessionStatus,
    PaymentStatus, Refund, RefundStatus, WebhookEvent, WebhookEventType,
};
use crate::{Error, Result};

/// AliPay gateway configuration
pub struct AliPayGateway {
    /// App ID - Your AliPay application ID
    app_id: String,

    /// Merchant Private Key (RSA2) - For signing requests
    private_key: String,

    /// AliPay Public Key - For verifying responses
    #[allow(dead_code)]
    alipay_public_key: String,

    /// HTTP client for API requests
    client: reqwest::Client,

    /// API gateway URL (sandbox or production)
    gateway_url: String,

    /// Sign type (RSA2 recommended)
    sign_type: String,

    /// Format (JSON)
    format: String,

    /// Charset (UTF-8)
    charset: String,

    /// API version
    version: String,
}

impl AliPayGateway {
    /// Create a new AliPay gateway instance
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
        }
    }

    /// Generate a signature for AliPay API requests
    fn generate_signature(&self, params: &HashMap<String, String>) -> String {
        // Sort parameters alphabetically
        let mut sorted_keys: Vec<&String> = params.keys().collect();
        sorted_keys.sort();

        // Build query string (key=value&key2=value2)
        let query_string = sorted_keys
            .iter()
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
    pub fn map_alipay_status(status: &str) -> PaymentStatus {
        match status {
            "TRADE_SUCCESS" => PaymentStatus::Succeeded,
            "TRADE_FINISHED" => PaymentStatus::Succeeded,
            "TRADE_CLOSED" => PaymentStatus::Canceled,
            "WAIT_BUYER_PAY" => PaymentStatus::Pending,
            "TRADE_PENDING" => PaymentStatus::Processing,
            _ => PaymentStatus::Pending,
        }
    }

    /// Map AliPay refund status to our RefundStatus
    #[allow(dead_code)]
    fn map_refund_status(status: &str) -> RefundStatus {
        match status {
            "REFUND_SUCCESS" => RefundStatus::Succeeded,
            "REFUND_CLOSED" => RefundStatus::Canceled,
            "REFUND_PROCESSING" => RefundStatus::Pending,
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
        // AliPay returns responses like: {"alipay_trade_create_response": {...}, "sign": "..."}
        let response_key = format!("{}_response", method.replace('.', "_"));

        let value: serde_json::Value = serde_json::from_str(response_text)
            .map_err(|e| Error::network(format!("Failed to parse AliPay response: {}", e)))?;

        let response_data = value
            .get(&response_key)
            .ok_or_else(|| Error::network("Missing response data from AliPay"))?;

        serde_json::from_value(response_data.clone())
            .map_err(|e| Error::network(format!("Failed to parse AliPay response data: {}", e)))
    }
}

#[async_trait]
impl PaymentGateway for AliPayGateway {
    fn id(&self) -> &'static str {
        "alipay"
    }

    fn name(&self) -> &'static str {
        "AliPay"
    }

    async fn create_payment(&self, request: CreatePaymentRequest) -> Result<PaymentSession> {
        let trade_no = self.generate_trade_no(request.order_id);

        // Build business parameters
        let biz_content = serde_json::json!({
            "out_trade_no": trade_no,
            "total_amount": request.amount.to_string(),
            "subject": format!("Order {}", request.order_id),
            "product_code": "FAST_INSTANT_TRADE_PAY", // PC web payment
            "body": format!("Payment for order {}", request.order_id),
        });

        // Build request parameters
        let mut params = self.build_common_params("alipay.trade.page.pay");
        params.insert(
            "notify_url".to_string(),
            "https://yourstore.com/webhooks/alipay".to_string(),
        );
        params.insert(
            "return_url".to_string(),
            "https://yourstore.com/checkout/success".to_string(),
        );
        params.insert("biz_content".to_string(), biz_content.to_string());

        // Generate signature
        let sign = self.generate_signature(&params);
        params.insert("sign".to_string(), sign);

        // For page payments, we return a URL that the client should redirect to
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

        Ok(PaymentSession {
            id: trade_no,
            client_secret: payment_url, // The payment page URL
            status: PaymentSessionStatus::Open,
            amount: request.amount,
            currency: request.currency,
            customer_id: request.customer_id,
        })
    }

    async fn confirm_payment(&self, payment_id: &str) -> Result<Payment> {
        // Query the trade status from AliPay
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
            .map_err(|e| Error::network(format!("AliPay API error: {}", e)))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| Error::network(format!("Failed to read AliPay response: {}", e)))?;

        let trade_response: AliPayTradeQueryResponse =
            self.parse_response(&response_text, "alipay.trade.query")?;

        if trade_response.code != "10000" {
            return Err(Error::payment_error(format!(
                "AliPay query failed: {} - {}",
                trade_response.sub_code.unwrap_or_default(),
                trade_response.sub_msg.unwrap_or_default()
            )));
        }

        Ok(Payment {
            id: format!("pay_{}", uuid::Uuid::new_v4()),
            gateway: self.id().to_string(),
            amount: Decimal::from_str_exact(&trade_response.total_amount.unwrap_or_default())
                .unwrap_or(Decimal::ZERO),
            currency: "CNY".to_string(), // AliPay primarily uses CNY
            status: trade_response
                .trade_status
                .map(|s| Self::map_alipay_status(&s))
                .unwrap_or(PaymentStatus::Pending),
            order_id: uuid::Uuid::parse_str(payment_id.split('_').next().unwrap_or(""))
                .unwrap_or_else(|_| uuid::Uuid::nil()),
            customer_id: trade_response
                .buyer_user_id
                .and_then(|id| uuid::Uuid::parse_str(&id).ok()),
            payment_method: "alipay".to_string(),
            transaction_id: trade_response.trade_no.unwrap_or_default(),
            client_secret: None,
            card_brand: None,
            card_last_four: None,
            captured_at: trade_response.send_pay_date.and_then(|d| {
                chrono::NaiveDateTime::parse_from_str(&d, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|ndt| ndt.and_utc())
            }),
            created_at: chrono::Utc::now(),
        })
    }

    async fn capture_payment(&self, payment_id: &str, _amount: Option<Decimal>) -> Result<Payment> {
        // AliPay auto-captures on successful payment
        self.confirm_payment(payment_id).await
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<Refund> {
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
            .map_err(|e| Error::network(format!("AliPay API error: {}", e)))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| Error::network(format!("Failed to read AliPay response: {}", e)))?;

        let refund_response: AliPayRefundResponse =
            self.parse_response(&response_text, "alipay.trade.refund")?;

        if refund_response.code != "10000" {
            return Err(Error::payment_error(format!(
                "AliPay refund failed: {} - {}",
                refund_response.sub_code.unwrap_or_default(),
                refund_response.sub_msg.unwrap_or_default()
            )));
        }

        Ok(Refund {
            id: refund_no,
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(Decimal::ZERO),
            currency: "CNY".to_string(),
            status: if refund_response.code == "10000" {
                RefundStatus::Succeeded
            } else {
                RefundStatus::Failed
            },
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn get_payment(&self, payment_id: &str) -> Result<Payment> {
        self.confirm_payment(payment_id).await
    }

    async fn handle_webhook(&self, payload: &[u8], _signature: &str) -> Result<WebhookEvent> {
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

        let trade_status = params.get("trade_status").map(|s| s.as_str()).unwrap_or("");
        let out_trade_no = params.get("out_trade_no").cloned().unwrap_or_default();

        let event_type = match trade_status {
            "TRADE_SUCCESS" | "TRADE_FINISHED" => WebhookEventType::PaymentSucceeded,
            "TRADE_CLOSED" => WebhookEventType::PaymentFailed,
            "REFUND_SUCCESS" => WebhookEventType::RefundSucceeded,
            _ => WebhookEventType::PaymentFailed,
        };

        Ok(WebhookEvent {
            event_type,
            payment_id: out_trade_no,
            data: serde_json::json!(params),
        })
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
    fn test_alipay_gateway_creation() {
        let gateway = AliPayGateway::new(
            "2024XXXXXX".to_string(),
            "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----".to_string(),
            "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----".to_string(),
            true, // sandbox
        );

        assert_eq!(gateway.id(), "alipay");
        assert_eq!(gateway.name(), "AliPay");
        assert!(gateway.gateway_url.contains("alipaydev"));
    }

    #[test]
    fn test_alipay_status_mapping() {
        assert!(matches!(
            AliPayGateway::map_alipay_status("TRADE_SUCCESS"),
            PaymentStatus::Succeeded
        ));
        assert!(matches!(
            AliPayGateway::map_alipay_status("TRADE_FINISHED"),
            PaymentStatus::Succeeded
        ));
        assert!(matches!(
            AliPayGateway::map_alipay_status("WAIT_BUYER_PAY"),
            PaymentStatus::Pending
        ));
        assert!(matches!(
            AliPayGateway::map_alipay_status("TRADE_CLOSED"),
            PaymentStatus::Canceled
        ));
    }
}
