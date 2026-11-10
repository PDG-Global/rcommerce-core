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
    axum::Extension(client_ip): axum::Extension<crate::middleware::ClientIp>,
    Json(body): Json<ContactRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    if !state
        .contact_rate_limiter
        .check_and_increment(client_ip.ip())
        .await
    {
        tracing::warn!("Contact rate limit exceeded for IP: {}", client_ip.ip());
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Too many submissions. Please try again later."})),
        ));
    }

    let email = body.email.trim();
    if !valid_email(email) {
        tracing::warn!(
            "Rejected contact submission with invalid email from IP: {}",
            client_ip.ip()
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Valid email is required"})),
        ));
    }
    if body.name.trim().is_empty() || body.name.len() > 200 || has_header_control_chars(&body.name)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Valid name is required"})),
        ));
    }
    if body.message.trim().is_empty()
        || body.message.len() > 5000
        || has_unsafe_control_chars(&body.message)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Valid message is required"})),
        ));
    }
    if body
        .order
        .as_deref()
        .is_some_and(|order| order.len() > 100 || has_header_control_chars(order))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid order number"})),
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
        "New contact form submission\n\nTopic: {}\nName: {}\nEmail: {}\nIP: {}\n{}\nMessage:\n{}",
        body.topic,
        body.name,
        email,
        client_ip.ip(),
        order_line,
        body.message
    );

    let html_body = format!(
        "<h2>New contact form submission</h2>\
        <p><strong>Topic:</strong> {}</p>\
        <p><strong>Name:</strong> {}</p>\
        <p><strong>Email:</strong> <a href=\"mailto:{}\">{}</a></p>\
        <p><strong>IP:</strong> {}</p>\
        {}\
        <p><strong>Message:</strong></p>\
        <p>{}</p>",
        escape_html(&body.topic),
        escape_html(&body.name),
        escape_html(email),
        escape_html(email),
        client_ip.ip(),
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
        "Contact form submitted: ip={} topic={} from={} <{}>",
        client_ip.ip(),
        body.topic,
        body.name,
        email
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({"message": "Message sent. We'll reply within 24-48 hours."})),
    ))
}

fn valid_email(email: &str) -> bool {
    email.len() <= 254 && email.parse::<lettre::Address>().is_ok()
}

fn has_header_control_chars(text: &str) -> bool {
    text.chars().any(|c| c.is_control())
}

fn has_unsafe_control_chars(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_email_validation_rejects_malformed_addresses() {
        for email in [
            "",
            "not-an-email",
            "../../../../tmp/pwned@x",
            "qa@example.com\nBcc: attacker@example.com",
        ] {
            assert!(!valid_email(email), "accepted invalid address: {email:?}");
        }
    }

    #[test]
    fn contact_email_validation_accepts_normal_addresses() {
        for email in ["qa@example.com", "customer+shop@example.co.uk"] {
            assert!(valid_email(email), "rejected valid address: {email:?}");
        }
    }
}
