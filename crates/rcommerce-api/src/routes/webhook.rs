//! Webhook Management API Routes
//!
//! Provides endpoints for managing webhooks including:
//! - Creating, updating, and deleting webhooks
//! - Testing webhook deliveries
//! - Viewing delivery history

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

/// Webhook response
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub id: String,
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub is_active: bool,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Create webhook request
#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
}

/// Update webhook request
#[derive(Debug, Deserialize)]
pub struct UpdateWebhookRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    pub is_active: Option<bool>,
}

/// Test webhook request
#[derive(Debug, Deserialize)]
pub struct TestWebhookRequest {
    pub event_type: String,
    pub payload: Option<serde_json::Value>,
}

/// Webhook delivery response
#[derive(Debug, Serialize)]
pub struct WebhookDeliveryResponse {
    pub id: String,
    pub event_type: String,
    pub status: i32,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// List webhooks query params
#[derive(Debug, Deserialize)]
pub struct ListWebhooksQuery {
    pub is_active: Option<bool>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// List all webhooks
pub async fn list_webhooks(
    State(state): State<AppState>,
    Query(query): Query<ListWebhooksQuery>,
) -> Result<Json<Vec<WebhookResponse>>, StatusCode> {
    let pool = state.db.pool();

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let rows = sqlx::query(
        "SELECT id, name, url, events, is_active, last_triggered_at, created_at FROM webhooks ORDER BY created_at DESC LIMIT $1 OFFSET $2"
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let webhooks: Vec<WebhookResponse> = rows
        .iter()
        .map(|r| WebhookResponse {
            id: r
                .try_get::<Uuid, _>("id")
                .map(|id| id.to_string())
                .unwrap_or_default(),
            name: r.try_get("name").unwrap_or_default(),
            url: r.try_get("url").unwrap_or_default(),
            events: r.try_get::<Vec<String>, _>("events").unwrap_or_default(),
            is_active: r.try_get("is_active").unwrap_or(true),
            last_triggered_at: r.try_get("last_triggered_at").ok(),
            created_at: r.try_get("created_at").unwrap_or(Utc::now()),
        })
        .collect();

    Ok(Json(webhooks))
}

/// Get a single webhook
pub async fn get_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WebhookResponse>, StatusCode> {
    let pool = state.db.pool();

    let row = sqlx::query(
        "SELECT id, name, url, events, is_active, last_triggered_at, created_at FROM webhooks WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let webhook = WebhookResponse {
        id: row
            .try_get::<Uuid, _>("id")
            .map(|id| id.to_string())
            .unwrap_or_default(),
        name: row.try_get("name").unwrap_or_default(),
        url: row.try_get("url").unwrap_or_default(),
        events: row.try_get::<Vec<String>, _>("events").unwrap_or_default(),
        is_active: row.try_get("is_active").unwrap_or(true),
        last_triggered_at: row.try_get("last_triggered_at").ok(),
        created_at: row.try_get("created_at").unwrap_or(Utc::now()),
    };

    Ok(Json(webhook))
}

/// Create a new webhook
pub async fn create_webhook(
    State(state): State<AppState>,
    Json(request): Json<CreateWebhookRequest>,
) -> Result<Json<WebhookResponse>, StatusCode> {
    let pool = state.db.pool();

    // Validate URL
    if !request.url.starts_with("http://") && !request.url.starts_with("https://") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate events
    if request.events.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Generate secret if not provided
    let secret = request.secret.unwrap_or_else(|| {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
        hex::encode(bytes)
    });

    let id = Uuid::new_v4();

    let row = sqlx::query(
        r#"
        INSERT INTO webhooks (id, name, url, secret, events, is_active, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, true, NOW(), NOW())
        RETURNING id, name, url, events, is_active, last_triggered_at, created_at
        "#,
    )
    .bind(id)
    .bind(&request.name)
    .bind(&request.url)
    .bind(&secret)
    .bind(&request.events)
    .fetch_one(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let webhook = WebhookResponse {
        id: row
            .try_get::<Uuid, _>("id")
            .map(|id| id.to_string())
            .unwrap_or_default(),
        name: row.try_get("name").unwrap_or_default(),
        url: row.try_get("url").unwrap_or_default(),
        events: row.try_get::<Vec<String>, _>("events").unwrap_or_default(),
        is_active: row.try_get("is_active").unwrap_or(true),
        last_triggered_at: row.try_get("last_triggered_at").ok(),
        created_at: row.try_get("created_at").unwrap_or(Utc::now()),
    };

    Ok(Json(webhook))
}

/// Update a webhook
pub async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateWebhookRequest>,
) -> Result<Json<WebhookResponse>, StatusCode> {
    let pool = state.db.pool();

    // Build dynamic update - simplified version
    let mut updates = vec![];

    if let Some(name) = request.name {
        let _ = sqlx::query("UPDATE webhooks SET name = $1, updated_at = NOW() WHERE id = $2")
            .bind(&name)
            .bind(id)
            .execute(pool)
            .await;
        updates.push("name");
    }

    if let Some(url) = request.url {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(StatusCode::BAD_REQUEST);
        }
        let _ = sqlx::query("UPDATE webhooks SET url = $1, updated_at = NOW() WHERE id = $2")
            .bind(&url)
            .bind(id)
            .execute(pool)
            .await;
        updates.push("url");
    }

    if let Some(events) = request.events {
        if events.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let _ = sqlx::query("UPDATE webhooks SET events = $1, updated_at = NOW() WHERE id = $2")
            .bind(&events)
            .bind(id)
            .execute(pool)
            .await;
        updates.push("events");
    }

    if let Some(is_active) = request.is_active {
        let _ = sqlx::query("UPDATE webhooks SET is_active = $1, updated_at = NOW() WHERE id = $2")
            .bind(is_active)
            .bind(id)
            .execute(pool)
            .await;
        updates.push("is_active");
    }

    if updates.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Fetch updated webhook
    let row = sqlx::query(
        "SELECT id, name, url, events, is_active, last_triggered_at, created_at FROM webhooks WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let webhook = WebhookResponse {
        id: row
            .try_get::<Uuid, _>("id")
            .map(|id| id.to_string())
            .unwrap_or_default(),
        name: row.try_get("name").unwrap_or_default(),
        url: row.try_get("url").unwrap_or_default(),
        events: row.try_get::<Vec<String>, _>("events").unwrap_or_default(),
        is_active: row.try_get("is_active").unwrap_or(true),
        last_triggered_at: row.try_get("last_triggered_at").ok(),
        created_at: row.try_get("created_at").unwrap_or(Utc::now()),
    };

    Ok(Json(webhook))
}

/// Delete a webhook
pub async fn delete_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let pool = state.db.pool();

    let result = sqlx::query("DELETE FROM webhooks WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Webhook deleted successfully"
    })))
}

/// Test a webhook
pub async fn test_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<TestWebhookRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let pool = state.db.pool();

    // Get webhook details
    let row =
        sqlx::query("SELECT url, secret, events FROM webhooks WHERE id = $1 AND is_active = true")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let url: String = row
        .try_get("url")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let secret: String = row
        .try_get("secret")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let events: Vec<String> = row
        .try_get("events")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Verify event type is in webhook's events
    if !events.contains(&request.event_type) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Build test payload
    let payload = request.payload.unwrap_or_else(|| {
        serde_json::json!({
            "event": request.event_type,
            "test": true,
            "timestamp": Utc::now().to_rfc3339(),
            "data": {
                "message": "This is a test webhook delivery"
            }
        })
    });

    // Sign payload
    let signature = sign_payload(&payload, &secret);

    // Send test webhook
    let client = reqwest::Client::new();
    let start = std::time::Instant::now();

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("X-Webhook-Signature", signature)
        .header("X-Webhook-Test", "true")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await;

    let duration = start.elapsed().as_millis() as i64;

    match response {
        Ok(resp) => {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();

            // Record delivery
            let _ = sqlx::query(
                r#"
                INSERT INTO webhook_deliveries (id, webhook_id, event_type, payload, response_status, response_body, delivered_at, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
                "#
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(&request.event_type)
            .bind(&payload)
            .bind(status)
            .bind(&body)
            .execute(pool)
            .await;

            // Update last_triggered_at
            let _ = sqlx::query("UPDATE webhooks SET last_triggered_at = NOW() WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await;

            Ok(Json(serde_json::json!({
                "success": (200..300).contains(&status),
                "status_code": status,
                "response_body": body,
                "duration_ms": duration,
                "message": if (200..300).contains(&status) {
                    "Webhook test successful"
                } else {
                    "Webhook returned non-success status"
                }
            })))
        }
        Err(e) => {
            // Record failed delivery
            let _ = sqlx::query(
                r#"
                INSERT INTO webhook_deliveries (id, webhook_id, event_type, payload, error_message, created_at)
                VALUES ($1, $2, $3, $4, $5, NOW())
                "#
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(&request.event_type)
            .bind(&payload)
            .bind(e.to_string())
            .execute(pool)
            .await;

            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get webhook delivery history
pub async fn get_webhook_deliveries(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ListWebhooksQuery>,
) -> Result<Json<Vec<WebhookDeliveryResponse>>, StatusCode> {
    let pool = state.db.pool();

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let rows = sqlx::query(
        r#"
        SELECT id, event_type, response_status, delivered_at, created_at
        FROM webhook_deliveries
        WHERE webhook_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let deliveries: Vec<WebhookDeliveryResponse> = rows
        .iter()
        .map(|r| WebhookDeliveryResponse {
            id: r
                .try_get::<Uuid, _>("id")
                .map(|id| id.to_string())
                .unwrap_or_default(),
            event_type: r.try_get("event_type").unwrap_or_default(),
            status: r.try_get::<i32, _>("response_status").unwrap_or(0),
            delivered_at: r.try_get("delivered_at").ok(),
            created_at: r.try_get("created_at").unwrap_or(Utc::now()),
        })
        .collect();

    Ok(Json(deliveries))
}

/// Sign webhook payload with HMAC-SHA256
fn sign_payload(payload: &serde_json::Value, secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let payload_str = payload.to_string();
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload_str.as_bytes());
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

/// Create webhook router
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/webhooks", get(list_webhooks).post(create_webhook))
        .route(
            "/webhooks/:id",
            get(get_webhook).put(update_webhook).delete(delete_webhook),
        )
        .route("/webhooks/:id/test", post(test_webhook))
        .route("/webhooks/:id/deliveries", get(get_webhook_deliveries))
}
