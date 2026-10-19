//! Newsletter subscription routes
//!
//! - POST /api/v1/newsletter/subscribe (public)

use axum::{extract::State, routing::post, Json, Router};

use crate::state::AppState;
use rcommerce_core::models::SubscribeNewsletterRequest;
use rcommerce_core::repository::{Database, PostgresNewsletterRepository};
use rcommerce_core::services::NewsletterService;
use rcommerce_core::Error;

/// POST /api/v1/newsletter/subscribe - Subscribe an email to the newsletter
///
/// Idempotent: subscribing an existing active subscriber still returns 200.
pub async fn subscribe(
    State(state): State<AppState>,
    Json(mut request): Json<SubscribeNewsletterRequest>,
) -> Result<Json<serde_json::Value>, Error> {
    // Normalize before validation so padded input ("  a@b.com  ") is accepted
    request.email = request.email.trim().to_string();

    use validator::Validate;
    request
        .validate()
        .map_err(|e| Error::validation(format!("Invalid email: {}", e)))?;

    let service = NewsletterService::new(PostgresNewsletterRepository::new(Database::new(
        state.db.pool().clone(),
    )));

    service.subscribe(&request.email).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// Router for newsletter routes (public)
pub fn router() -> Router<AppState> {
    Router::new().route("/newsletter/subscribe", post(subscribe))
}
