//! Airwallex settlement reconciliation.
//!
//! Pulls settlement records from the Airwallex `pa/settlement_records` API
//! and backfills fees, settlement batch IDs, and settlement dates onto
//! payments and refunds. The accounting export joins charges to batched bank
//! deposits via `payments.settlement_id` (see
//! docs/api/05-accounting-export-api.md).

use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::AirwallexConfig;
use crate::{Error, Result};

const AUTH_URL: &str = "https://api.airwallex.com/api/v1/authentication/login";

#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    /// Payment intents queried
    pub queried: usize,
    /// Payments updated with fee/settlement data
    pub payments_updated: usize,
    /// Refunds updated with settlement dates
    pub refunds_updated: usize,
    /// Intent ids that returned no settlement record (not settled yet)
    pub not_settled: usize,
    /// Intent ids with no matching local payment row
    pub unmatched: Vec<String>,
}

/// Client for the Airwallex settlement_records API
pub struct SettlementSync {
    client_id: String,
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    token: Arc<RwLock<Option<CachedToken>>>,
}

struct CachedToken {
    access_token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl SettlementSync {
    /// Build a sync client from the Airwallex gateway config.
    /// Returns None when Airwallex is not configured.
    pub fn new(config: &AirwallexConfig) -> Option<Self> {
        let client_id = config.client_id.clone()?;
        let api_key = config.api_key.clone()?;
        Some(Self {
            client_id,
            api_key,
            base_url: if config.demo {
                "https://api-demo.airwallex.com".to_string()
            } else {
                "https://api.airwallex.com".to_string()
            },
            client: reqwest::Client::new(),
            token: Arc::new(RwLock::new(None)),
        })
    }

    async fn access_token(&self) -> Result<String> {
        {
            let guard = self.token.read().await;
            if let Some(ref cached) = *guard {
                if cached.expires_at > chrono::Utc::now() + chrono::Duration::minutes(2) {
                    return Ok(cached.access_token.clone());
                }
            }
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            token: String,
        }

        let response = self
            .client
            .post(AUTH_URL)
            .header("x-client-id", &self.client_id)
            .header("x-api-key", &self.api_key)
            .send()
            .await
            .map_err(|e| Error::network(format!("Airwallex auth request failed: {}", e)))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::payment_error(format!(
                "Airwallex auth rejected: {}",
                body
            )));
        }

        let parsed: TokenResponse = response
            .json()
            .await
            .map_err(|e| Error::network(format!("Failed to parse Airwallex token: {}", e)))?;

        {
            let mut guard = self.token.write().await;
            *guard = Some(CachedToken {
                access_token: parsed.token.clone(),
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(25),
            });
        }

        Ok(parsed.token)
    }

    /// Fetch settlement records for one payment intent.
    async fn fetch_records(&self, intent_id: &str) -> Result<Vec<SettlementRecord>> {
        let token = self.access_token().await?;

        let response = self
            .client
            .get(format!("{}/api/v1/pa/settlement_records", self.base_url))
            .query(&[("payment_intent_id", intent_id)])
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| Error::network(format!("Settlement records request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::payment_error(format!(
                "Settlement records query failed ({}): {}",
                status, body
            )));
        }

        // Read the body as text first so deserialization failures can
        // surface the offending payload instead of a bare decode error.
        let body = response
            .text()
            .await
            .map_err(|e| Error::network(format!("Settlement records read failed: {}", e)))?;

        #[derive(Deserialize)]
        struct ListResponse {
            items: Vec<SettlementRecord>,
        }

        let parsed: ListResponse = serde_json::from_str(&body).map_err(|e| {
            Error::network(format!(
                "Failed to parse settlement records: {} — body: {}",
                e,
                &body.chars().take(2000).collect::<String>()
            ))
        })?;

        Ok(parsed.items)
    }

    /// Sync one payment intent id → payments/refunds rows. Returns
    /// Ok(true) when a payment row was updated.
    pub async fn sync_intent(&self, pool: &PgPool, intent_id: &str) -> Result<bool> {
        let records = self.fetch_records(intent_id).await?;
        let mut updated = false;

        for record in records {
            match record.transaction_type.as_str() {
                "Payment" => {
                    let fee_components = serde_json::json!(record
                        .fee_details_list
                        .iter()
                        .flatten()
                        .filter_map(|f| match (&f.fee_type, f.settlement_total_amount) {
                            (Some(kind), Some(amount)) => Some(serde_json::json!({
                                "type": kind,
                                "amount": amount,
                            })),
                            _ => None,
                        })
                        .collect::<Vec<_>>());
                    let res = sqlx::query(
                        r#"
                        UPDATE payments SET
                            transaction_id = $2,
                            fee_amount = $3,
                            fee_currency = $4,
                            fee_components = $5,
                            settlement_id = $6,
                            settled_at = $7,
                            settlement_currency = $8,
                            fx_rate = $9,
                            updated_at = NOW()
                        WHERE gateway = 'airwallex'
                          AND gateway_payment_id = $1
                          AND settlement_id IS NULL
                        "#,
                    )
                    .bind(intent_id)
                    .bind(&record.transaction_id)
                    .bind(record.fees.map(|f| f.abs()))
                    .bind(&record.settlement_currency)
                    .bind(fee_components)
                    .bind(&record.settlement_batch_id)
                    .bind(record.settled_at.as_deref().and_then(parse_awx_time))
                    .bind(&record.settlement_currency)
                    .bind(record.exchange_rate)
                    .execute(pool)
                    .await?;
                    if res.rows_affected() > 0 {
                        updated = true;
                    }
                }
                "Refund" => {
                    // Refund settlement records carry the refund transaction
                    // id; match it to refunds.gateway_refund_id.
                    sqlx::query(
                        r#"
                        UPDATE refunds SET
                            settled_at = $2,
                            updated_at = NOW()
                        WHERE gateway_refund_id = $1
                          AND settled_at IS NULL
                        "#,
                    )
                    .bind(&record.transaction_id)
                    .bind(record.settled_at.as_deref().and_then(parse_awx_time))
                    .execute(pool)
                    .await?;
                }
                _ => {}
            }
        }

        Ok(updated)
    }

    /// Sync all local Airwallex payments that have not been matched to a
    /// settlement batch yet. `limit` bounds the number of intent queries
    /// per call (0 = no bound); queries are spaced to respect rate limits.
    pub async fn sync_pending(&self, pool: &PgPool, limit: usize) -> Result<SyncReport> {
        let mut query = String::from(
            "SELECT gateway_payment_id FROM payments \
             WHERE gateway = 'airwallex' AND gateway_payment_id IS NOT NULL \
               AND settlement_id IS NULL AND status IN ('paid', 'refunded') \
             ORDER BY created_at",
        );
        if limit > 0 {
            query.push_str(&format!(" LIMIT {}", limit));
        }
        let intent_ids: Vec<String> = sqlx::query_scalar(&query).fetch_all(pool).await?;

        let mut report = SyncReport {
            queried: intent_ids.len(),
            ..Default::default()
        };

        for intent_id in intent_ids {
            match self.sync_intent(pool, &intent_id).await {
                Ok(true) => report.payments_updated += 1,
                Ok(false) => {
                    // Distinguish "settled but no local row" from "not settled"
                    match self.fetch_records(&intent_id).await {
                        Ok(records) if records.is_empty() => report.not_settled += 1,
                        Ok(_) => report.unmatched.push(intent_id),
                        Err(_) => report.not_settled += 1,
                    }
                }
                Err(e) => {
                    tracing::warn!("Settlement sync failed for {}: {}", intent_id, e);
                    report.unmatched.push(intent_id);
                }
            }
            // ~8 queries/second, well within Airwallex rate limits
            tokio::time::sleep(std::time::Duration::from_millis(125)).await;
        }

        Ok(report)
    }
}

/// Decimal fields come back from Airwallex as JSON integers (`1`),
/// floats (`-5.2`), or occasionally strings — this workspace's
/// rust_decimal serde config only accepts strings, so convert via f64.
mod dec_opt {
    use rust_decimal::Decimal;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(d: D) -> Result<Option<Decimal>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Option::<serde_json::Value>::deserialize(d)?;
        Ok(v.and_then(|v| match v {
            serde_json::Value::Null => None,
            other => other
                .as_f64()
                .and_then(|f| Decimal::try_from(f).ok())
                .or_else(|| other.as_str().and_then(|s| s.parse().ok())),
        }))
    }
}

#[derive(Debug, Deserialize)]
struct SettlementRecord {
    #[serde(rename = "settlement_batch_id")]
    settlement_batch_id: Option<String>,
    #[serde(rename = "transaction_id")]
    transaction_id: Option<String>,
    #[serde(rename = "transaction_type")]
    transaction_type: String,
    #[serde(rename = "settled_at")]
    settled_at: Option<String>,
    #[serde(rename = "settlement_currency")]
    settlement_currency: Option<String>,
    #[serde(
        default,
        deserialize_with = "dec_opt::deserialize",
        rename = "exchange_rate"
    )]
    exchange_rate: Option<Decimal>,
    /// Total fees in settlement currency (negative); absent when no fee
    #[serde(default, deserialize_with = "dec_opt::deserialize", rename = "fees")]
    fees: Option<Decimal>,
    #[serde(rename = "fee_details_list")]
    fee_details_list: Option<Vec<FeeDetail>>,
}

#[derive(Debug, Deserialize)]
struct FeeDetail {
    #[serde(rename = "fee_type")]
    fee_type: Option<String>,
    #[serde(
        default,
        deserialize_with = "dec_opt::deserialize",
        rename = "settlement_total_amount"
    )]
    settlement_total_amount: Option<Decimal>,
}

/// Parse Airwallex timestamps like `2026-10-12T23:03:57+0800`
fn parse_awx_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z")
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(s))
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_awx_time() {
        let dt = parse_awx_time("2026-10-12T23:03:57+0800").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-10-12T15:03:57+00:00");
        let dt = parse_awx_time("2026-10-12T23:03:57+08:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-10-12T15:03:57+00:00");
        assert!(parse_awx_time("garbage").is_none());
    }
}
