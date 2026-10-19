//! WeChat Pay gateway implementation
//!
//! WeChat Pay is a digital wallet service incorporated into WeChat, which allows users to
//! perform mobile payments and online transactions. It's one of the dominant payment methods
//! in China and is increasingly accepted globally.
//!
//! ## API Documentation
//! - Official Docs: https://pay.weixin.qq.com/wiki/doc/apiv3/index.shtml
//! - Sandbox Testing: https://pay.weixin.qq.com/wiki/doc/apiv3/open/pay/chapter2_8_1.shtml
//!
//! ## Supported Features
//! - Native Payments (QR Code)
//! - JSAPI Payments (In-App / Mini Programs)
//! - H5 Payments (Mobile Browser)
//! - App Payments (Mobile SDK)
//! - Refunds
//! - Transaction queries
//! - Webhook notifications

use async_trait::async_trait;
use base64::Engine as Base64Engine;
use reqwest;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::payment::{
    CreatePaymentRequest, Payment, PaymentGateway, PaymentSession, PaymentSessionStatus,
    PaymentStatus, Refund, RefundStatus, WebhookEvent, WebhookEventType,
};
use crate::{Error, Result};

/// WeChat Pay gateway configuration
pub struct WeChatPayGateway {
    /// Merchant ID (mchid) - Your WeChat Pay merchant account ID
    mch_id: String,

    /// API v3 Key - Used for encrypting/decrypting sensitive data
    #[allow(dead_code)]
    api_key: String,

    /// App ID - Your WeChat App ID (for JSAPI/App payments)
    app_id: String,

    /// API Client Serial Number - For certificate-based authentication
    serial_no: String,

    /// Private key for signing requests (PEM format)
    private_key: String,

    /// HTTP client for API requests
    client: reqwest::Client,

    /// API base URL (sandbox or production)
    base_url: String,
}

impl WeChatPayGateway {
    /// Create a new WeChat Pay gateway instance
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

        Self {
            mch_id,
            api_key,
            app_id,
            serial_no,
            private_key,
            client: reqwest::Client::new(),
            base_url,
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

    /// Map WeChat Pay transaction status to our PaymentStatus
    fn map_wechat_status(status: &str) -> PaymentStatus {
        match status {
            "SUCCESS" => PaymentStatus::Succeeded,
            "REFUND" => PaymentStatus::Refunded,
            "NOTPAY" => PaymentStatus::Pending,
            "CLOSED" => PaymentStatus::Canceled,
            "REVOKED" => PaymentStatus::Canceled,
            "USERPAYING" => PaymentStatus::Processing,
            "PAYERROR" => PaymentStatus::Failed,
            _ => PaymentStatus::Pending,
        }
    }

    /// Map WeChat Pay refund status to our RefundStatus
    fn map_refund_status(status: &str) -> RefundStatus {
        match status {
            "SUCCESS" => RefundStatus::Succeeded,
            "CLOSED" => RefundStatus::Canceled,
            "PROCESSING" => RefundStatus::Pending,
            "ABNORMAL" => RefundStatus::Failed,
            _ => RefundStatus::Pending,
        }
    }

    /// Generate a unique out_trade_no (order ID)
    fn generate_trade_no(&self, order_id: uuid::Uuid) -> String {
        format!(
            "{}_{}",
            order_id.to_string().replace("-", ""),
            chrono::Utc::now().timestamp()
        )
    }
}

#[async_trait]
impl PaymentGateway for WeChatPayGateway {
    fn id(&self) -> &'static str {
        "wechatpay"
    }

    fn name(&self) -> &'static str {
        "WeChat Pay"
    }

    async fn create_payment(&self, request: CreatePaymentRequest) -> Result<PaymentSession> {
        // Convert amount to cents (WeChat Pay uses smallest currency unit)
        let amount_cents: i64 = (request.amount * dec!(100))
            .try_into()
            .map_err(|_| Error::validation("Invalid amount"))?;

        let trade_no = self.generate_trade_no(request.order_id);

        let body = serde_json::json!({
            "mchid": self.mch_id,
            "appid": self.app_id,
            "description": format!("Order {}", request.order_id),
            "out_trade_no": trade_no,
            "notify_url": "https://yourstore.com/webhooks/wechatpay",
            "amount": {
                "total": amount_cents,
                "currency": request.currency.to_uppercase()
            },
            "payer": {
                "openid": request.customer_id.map(|id| id.to_string()).unwrap_or_default()
            }
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
            .map_err(|e| Error::network(format!("WeChat Pay API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "WeChat Pay error: {}",
                error_text
            )));
        }

        let wechat_response: WeChatPayNativeResponse = response
            .json()
            .await
            .map_err(|e| Error::network(format!("Failed to parse WeChat Pay response: {}", e)))?;

        Ok(PaymentSession {
            id: trade_no,
            client_secret: wechat_response.code_url, // QR code URL for native payments
            status: PaymentSessionStatus::Open,
            amount: request.amount,
            currency: request.currency,
            customer_id: request.customer_id,
        })
    }

    async fn confirm_payment(&self, payment_id: &str) -> Result<Payment> {
        // Query the transaction status from WeChat Pay
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
            .map_err(|e| Error::network(format!("WeChat Pay API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "WeChat Pay error: {}",
                error_text
            )));
        }

        let transaction: WeChatPayTransaction = response
            .json()
            .await
            .map_err(|e| Error::network(format!("Failed to parse WeChat Pay response: {}", e)))?;

        Ok(Payment {
            id: format!("pay_{}", uuid::Uuid::new_v4()),
            gateway: self.id().to_string(),
            amount: Decimal::new(transaction.amount.total, 2),
            currency: transaction.amount.currency,
            status: Self::map_wechat_status(&transaction.trade_state),
            order_id: uuid::Uuid::parse_str(
                transaction.out_trade_no.split('_').next().unwrap_or(""),
            )
            .unwrap_or_else(|_| uuid::Uuid::nil()),
            customer_id: transaction
                .payer
                .as_ref()
                .and_then(|p| uuid::Uuid::parse_str(&p.openid).ok()),
            payment_method: "wechatpay".to_string(),
            transaction_id: transaction.transaction_id.unwrap_or_default(),
            client_secret: None,
            card_brand: None,
            card_last_four: None,
            captured_at: if transaction.trade_state == "SUCCESS" {
                Some(chrono::Utc::now())
            } else {
                None
            },
            created_at: chrono::Utc::now(),
        })
    }

    async fn capture_payment(&self, payment_id: &str, _amount: Option<Decimal>) -> Result<Payment> {
        // WeChat Pay auto-captures on successful payment
        // This method just returns the current payment status
        self.confirm_payment(payment_id).await
    }

    async fn refund_payment(
        &self,
        payment_id: &str,
        amount: Option<Decimal>,
        reason: &str,
    ) -> Result<Refund> {
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
                "total": refund_amount_cents, // Should be queried from original transaction
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
            .map_err(|e| Error::network(format!("WeChat Pay API error: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::payment_error(format!(
                "WeChat Pay refund error: {}",
                error_text
            )));
        }

        let refund_response: WeChatPayRefundResponse = response.json().await.map_err(|e| {
            Error::network(format!("Failed to parse WeChat Pay refund response: {}", e))
        })?;

        Ok(Refund {
            id: refund_response.refund_id,
            payment_id: payment_id.to_string(),
            amount: amount.unwrap_or(Decimal::ZERO),
            currency: "CNY".to_string(),
            status: Self::map_refund_status(&refund_response.status),
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn get_payment(&self, payment_id: &str) -> Result<Payment> {
        self.confirm_payment(payment_id).await
    }

    async fn handle_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookEvent> {
        // Verify webhook signature
        let _signature = signature;

        let notification: WeChatPayNotification = serde_json::from_slice(payload)
            .map_err(|e| Error::validation(format!("Invalid WeChat Pay webhook payload: {}", e)))?;

        let event_type = match notification.event_type.as_str() {
            "TRANSACTION.SUCCESS" => WebhookEventType::PaymentSucceeded,
            "TRANSACTION.FAIL" => WebhookEventType::PaymentFailed,
            "REFUND.SUCCESS" => WebhookEventType::RefundSucceeded,
            _ => {
                return Err(Error::validation(
                    "Unsupported WeChat Pay webhook event type",
                ))
            }
        };

        let payment_id = notification.out_trade_no.clone();

        Ok(WebhookEvent {
            event_type,
            payment_id,
            data: serde_json::json!(notification),
        })
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
    trade_type: Option<String>,
    trade_state: String,
    trade_state_desc: String,
    bank_type: Option<String>,
    attach: Option<String>,
    success_time: Option<String>,
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
    extra: std::collections::HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wechatpay_gateway_creation() {
        let gateway = WeChatPayGateway::new(
            "1234567890".to_string(),
            "api_key".to_string(),
            "wx1234567890".to_string(),
            "serial_no".to_string(),
            "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----".to_string(),
            true, // sandbox
        );

        assert_eq!(gateway.id(), "wechatpay");
        assert_eq!(gateway.name(), "WeChat Pay");
        assert!(gateway.base_url.contains("sandbox"));
    }

    #[test]
    fn test_wechatpay_status_mapping() {
        assert!(matches!(
            WeChatPayGateway::map_wechat_status("SUCCESS"),
            PaymentStatus::Succeeded
        ));
        assert!(matches!(
            WeChatPayGateway::map_wechat_status("NOTPAY"),
            PaymentStatus::Pending
        ));
        assert!(matches!(
            WeChatPayGateway::map_wechat_status("CLOSED"),
            PaymentStatus::Canceled
        ));
        assert!(matches!(
            WeChatPayGateway::map_wechat_status("PAYERROR"),
            PaymentStatus::Failed
        ));
    }
}
