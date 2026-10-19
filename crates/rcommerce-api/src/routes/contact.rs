//! Contact form endpoint — sends customer inquiries to the store email.

use axum::{extract::State, http::StatusCode, Json};
use rcommerce_core::notification::{Notification, NotificationChannel, NotificationPriority};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ContactRequest {
    pub topic: String,
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub order: Option<String>,
    pub message: String,
}

const TOPICS: &[&str] = &[
    "shipping",
    "sales",
    "payments",
    "distribution",
    "new-products",
    "other",
];

pub async fn submit_contact(
    State(state): State<AppState>,
    Json(body): Json<ContactRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    // Validate
    let email = body.email.trim();
    if email.is_empty() || !email.contains('@') || email.len() > 254 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Valid email is required"})),
        ));
    }
    if body.name.trim().is_empty() || body.name.len() > 200 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Name is required"})),
        ));
    }
    if body.message.trim().is_empty() || body.message.len() > 5000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Message is required"})),
        ));
    }
    if !TOPICS.contains(&body.topic.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid topic"})),
        ));
    }

    let Some(ref notification_service) = state.notification_service else {
        tracing::error!("Contact form submitted but notification service not configured");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Contact form unavailable"})),
        ));
    };

    let order_line = body
        .order
        .as_deref()
        .filter(|o| !o.trim().is_empty())
        .map(|o| format!("Order: {}\n", o))
        .unwrap_or_default();

    let text_body = format!(
        "New contact form submission\n\nTopic: {}\nName: {}\nEmail: {}\n{}\nMessage:\n{}",
        body.topic, body.name, email, order_line, body.message
    );

    let html_body = format!(
        "<h2>New contact form submission</h2>\
        <p><strong>Topic:</strong> {}</p>\
        <p><strong>Name:</strong> {}</p>\
        <p><strong>Email:</strong> <a href=\"mailto:{}\">{}</a></p>\
        {}\
        <p><strong>Message:</strong></p>\
        <p>{}</p>",
        escape_html(&body.topic),
        escape_html(&body.name),
        escape_html(email),
        escape_html(email),
        body.order
            .as_deref()
            .filter(|o| !o.trim().is_empty())
            .map(|o| format!("<p><strong>Order:</strong> {}</p>", escape_html(o)))
            .unwrap_or_default(),
        escape_html(&body.message).replace('\n', "<br/>"),
    );

    let notification = Notification::new(
        NotificationChannel::Email,
        state.branding.support_email.clone(),
        format!("[Contact] {} — {}", body.topic, body.name),
        text_body,
    )
    .with_priority(NotificationPriority::High)
    .with_html_body(html_body);

    notification_service
        .send(&notification)
        .await
        .map_err(|e| {
            tracing::error!("Failed to send contact form email: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::json!({"error": "Failed to send message. Please try again later."}),
                ),
            )
        })?;

    tracing::info!(
        "Contact form submitted: topic={} from={} <{}>",
        body.topic,
        body.name,
        email
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({"message": "Message sent. We'll reply within 24-48 hours."})),
    ))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
