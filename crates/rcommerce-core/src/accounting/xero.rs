//! Xero accounting integration
//!
//! Creates ACCREC invoices in Xero when orders are paid, then records a
//! payment against the invoice so it reconciles automatically.
//!
//! Uses a Xero "custom connection" (OAuth2 client credentials grant) so the
//! server can talk to a single Xero organisation without user interaction.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::XeroConfig;
use crate::{Error, Result};

const XERO_TOKEN_URL: &str = "https://identity.xero.com/connect/token";
const XERO_API_BASE: &str = "https://api.xero.com/api.xro/2.0";

/// Xero invoice + payment client with token caching
pub struct XeroClient {
    config: XeroConfig,
    client: reqwest::Client,
    token: Arc<RwLock<Option<CachedToken>>>,
}

struct CachedToken {
    access_token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// Line item for a Xero invoice
#[derive(Debug, Clone)]
pub struct XeroInvoiceLine {
    pub description: String,
    pub quantity: Decimal,
    pub unit_amount: Decimal,
    pub account_code: String,
}

/// Parameters for creating a Xero invoice from an order
pub struct XeroInvoiceParams {
    pub order_id: Uuid,
    pub order_number: String,
    pub customer_name: String,
    pub customer_email: String,
    pub currency: String,
    pub lines: Vec<XeroInvoiceLine>,
    pub total: Decimal,
    pub date: String,
    pub reference: String,
}

impl XeroClient {
    pub fn new(config: XeroConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            token: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if the client has full credentials configured
    pub fn is_configured(&self) -> bool {
        self.config.enabled
            && self.config.client_id.is_some()
            && self.config.client_secret.is_some()
            && self.config.tenant_id.is_some()
    }

    async fn get_access_token(&self) -> Result<String> {
        {
            let guard = self.token.read().await;
            if let Some(ref cached) = *guard {
                if cached.expires_at > chrono::Utc::now() + chrono::Duration::minutes(2) {
                    return Ok(cached.access_token.clone());
                }
            }
        }

        let client_id = self
            .config
            .client_id
            .as_ref()
            .ok_or_else(|| Error::config("Xero client_id not configured"))?;
        let client_secret = self
            .config
            .client_secret
            .as_ref()
            .ok_or_else(|| Error::config("Xero client_secret not configured"))?;

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: i64,
        }

        let response = self
            .client
            .post(XERO_TOKEN_URL)
            .basic_auth(client_id, Some(client_secret))
            .form(&[
                ("grant_type", "client_credentials"),
                ("scope", "accounting.transactions accounting.contacts"),
            ])
            .send()
            .await
            .map_err(|e| Error::network(format!("Xero token request failed: {}", e)))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::config(format!(
                "Xero token request rejected: {}",
                body
            )));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| Error::network(format!("Failed to parse Xero token: {}", e)))?;

        let expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(token_response.expires_in - 60);

        {
            let mut guard = self.token.write().await;
            *guard = Some(CachedToken {
                access_token: token_response.access_token.clone(),
                expires_at,
            });
        }

        Ok(token_response.access_token)
    }

    fn tenant_id(&self) -> Result<&str> {
        self.config
            .tenant_id
            .as_deref()
            .ok_or_else(|| Error::config("Xero tenant_id not configured"))
    }

    /// Create an ACCREC invoice in Xero for a paid order
    pub async fn create_invoice(&self, params: &XeroInvoiceParams) -> Result<String> {
        let token = self.get_access_token().await?;

        let line_items: Vec<serde_json::Value> = params
            .lines
            .iter()
            .map(|line| {
                serde_json::json!({
                    "Description": line.description,
                    "Quantity": line.quantity,
                    "UnitAmount": line.unit_amount,
                    "AccountCode": line.account_code,
                })
            })
            .collect();

        let payload = serde_json::json!({
            "Invoices": [{
                "Type": "ACCREC",
                "Contact": {
                    "Name": params.customer_name,
                    "EmailAddress": params.customer_email,
                },
                "Date": params.date,
                "DueDate": params.date,
                "InvoiceNumber": params.order_number,
                "Reference": params.reference,
                "CurrencyCode": params.currency,
                "Status": self.config.invoice_status,
                "LineAmountTypes": "Inclusive",
                "LineItems": line_items,
            }]
        });

        let response = self
            .client
            .post(format!("{}/Invoices", XERO_API_BASE))
            .header("Authorization", format!("Bearer {}", token))
            .header("Xero-Tenant-Id", self.tenant_id()?)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| Error::network(format!("Xero invoice request failed: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| Error::network(format!("Xero response read failed: {}", e)))?;

        if !status.is_success() {
            return Err(Error::payment_error(format!(
                "Xero invoice creation failed ({}): {}",
                status, body
            )));
        }

        #[derive(Deserialize)]
        struct InvoiceResponse {
            #[serde(rename = "Invoices")]
            invoices: Vec<InvoiceId>,
        }
        #[derive(Deserialize)]
        struct InvoiceId {
            #[serde(rename = "InvoiceID")]
            invoice_id: String,
        }

        let parsed: InvoiceResponse = serde_json::from_str(&body).map_err(|e| {
            Error::network(format!(
                "Failed to parse Xero invoice response: {} — {}",
                e, body
            ))
        })?;

        parsed
            .invoices
            .first()
            .map(|i| i.invoice_id.clone())
            .ok_or_else(|| Error::network("Xero response contained no invoice ID".to_string()))
    }

    /// Record a payment against a Xero invoice (reconciles it)
    pub async fn create_payment(
        &self,
        invoice_id: &str,
        amount: Decimal,
        currency: &str,
        date: &str,
        reference: &str,
    ) -> Result<()> {
        let token = self.get_access_token().await?;

        let bank_account_code = self
            .config
            .bank_account_code
            .as_ref()
            .ok_or_else(|| Error::config("Xero bank_account_code not configured"))?;

        let payload = serde_json::json!({
            "Payments": [{
                "Invoice": { "InvoiceID": invoice_id },
                "Account": { "Code": bank_account_code },
                "Date": date,
                "Amount": amount,
                "CurrencyRate": 1.0,
                "Reference": reference,
            }]
        });

        let _ = currency; // Xero infers currency from the invoice

        let response = self
            .client
            .post(format!("{}/Payments", XERO_API_BASE))
            .header("Authorization", format!("Bearer {}", token))
            .header("Xero-Tenant-Id", self.tenant_id()?)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| Error::network(format!("Xero payment request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::payment_error(format!(
                "Xero payment creation failed ({}): {}",
                status, body
            )));
        }

        Ok(())
    }

    /// Create an invoice for an order and immediately reconcile it with a payment
    pub async fn sync_paid_order(&self, params: &XeroInvoiceParams) -> Result<String> {
        let invoice_id = self.create_invoice(params).await?;

        tracing::info!(
            "Xero invoice {} created for order {}",
            invoice_id,
            params.order_number
        );

        if let Err(e) = self
            .create_payment(
                &invoice_id,
                params.total,
                &params.currency,
                &params.date,
                &format!("Payment for {}", params.order_number),
            )
            .await
        {
            // Log but don't fail — the invoice exists, payment can be reconciled manually
            tracing::error!(
                "Xero payment reconciliation failed for invoice {}: {}",
                invoice_id,
                e
            );
        } else {
            tracing::info!(
                "Xero payment reconciled for invoice {} (order {})",
                invoice_id,
                params.order_number
            );
        }

        Ok(invoice_id)
    }
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct XeroError {
    #[serde(rename = "ErrorNumber")]
    error_number: Option<i32>,
    #[serde(rename = "Message")]
    message: Option<String>,
}
