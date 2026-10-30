use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::IpAddr;
use uuid::Uuid;

use crate::middleware::ClientIp;
use crate::state::AppState;
use rcommerce_core::{
    models::CreateCustomerRequest,
    notification::{Notification, NotificationChannel, NotificationPriority},
    Error,
};

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub remember_me: bool,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub customer: CustomerInfo,
}

/// Customer info in auth responses
#[derive(Debug, Serialize)]
pub struct CustomerInfo {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
}

/// Email verification required response
#[derive(Debug, Serialize)]
pub struct EmailVerificationRequiredResponse {
    pub message: String,
    pub requires_email_verification: bool,
}

impl IntoResponse for EmailVerificationRequiredResponse {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "code": "email_verification_required",
                "message": self.message,
                "requires_email_verification": self.requires_email_verification,
            }
        });
        (StatusCode::FORBIDDEN, Json(body)).into_response()
    }
}

/// Imported account login response
#[derive(Debug, Serialize)]
pub struct ImportedAccountResponse {
    pub message: String,
    pub requires_password_reset: bool,
}

impl IntoResponse for ImportedAccountResponse {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "code": "account_requires_password_reset",
                "message": self.message,
                "requires_password_reset": self.requires_password_reset,
            }
        });
        (StatusCode::FORBIDDEN, Json(body)).into_response()
    }
}

/// Register request
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<String>,
}

impl RegisterRequest {
    /// Checked before hashing: hashing cost grows with the password length,
    /// and the other fields must fit the `customers` columns (email 255,
    /// names 100, phone 50) or the insert fails with a database error.
    fn validate(&self) -> Result<(), Error> {
        if self.password.len() < 8 {
            return Err(Error::validation("Password must be at least 8 characters"));
        }
        if self.password.len() > 128 {
            return Err(Error::validation("Password must be at most 128 characters"));
        }
        if self.email.chars().count() > 255 {
            return Err(Error::validation("Email must be at most 255 characters"));
        }
        if self.first_name.chars().count() > 100 || self.last_name.chars().count() > 100 {
            return Err(Error::validation("Names must be at most 100 characters"));
        }
        if self
            .phone
            .as_deref()
            .is_some_and(|p| p.chars().count() > 50)
        {
            return Err(Error::validation("Phone must be at most 50 characters"));
        }
        Ok(())
    }
}

/// Register response
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub customer: CustomerInfo,
    pub message: String,
}

/// Refresh token request
#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// Refresh token response
#[derive(Debug, Serialize)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Password reset request
#[derive(Debug, Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

/// Password reset confirm request
#[derive(Debug, Deserialize)]
pub struct PasswordResetConfirmRequest {
    pub token: String,
    pub password: String,
}

/// Password reset response
#[derive(Debug, Serialize)]
pub struct PasswordResetResponse {
    pub message: String,
    pub token: Option<String>, // Only for demo/testing
}

/// Resend verification request
#[derive(Debug, Deserialize)]
pub struct ResendVerificationRequest {
    pub email: String,
}

/// Resend verification response
#[derive(Debug, Serialize)]
pub struct ResendVerificationResponse {
    pub message: String,
}

/// The only answer a failed login ever gives: unknown e-mail, wrong
/// password, imported account without a local password, and missing password
/// are indistinguishable (including in response time — see
/// [`verify_against_dummy`]).
fn unauthorized() -> Response {
    Error::unauthorized("Invalid email or password").into_response()
}

/// Generic throttling response; carries Retry-After but reveals
/// nothing about which budget tripped or whether the account exists.
fn login_throttled(retry_after: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({
            "error": {
                "code": "too_many_requests",
                "message": "Too many login attempts. Please try again later.",
            }
        })),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after.max(1).to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

/// Verify the offered password against the eagerly-created dummy hash so
/// unknown e-mails cost the same Argon2 work as real ones. The result is
/// discarded; this exists purely to even out timing.
fn verify_against_dummy(state: &AppState, password: &str) {
    if state.dummy_password_hash.is_empty() {
        return;
    }
    let _ = state
        .auth_service
        .verify_password(password, &state.dummy_password_hash);
}

/// Login input bounds. Argon2 hashing cost grows with the password length,
/// and e-mails must fit the `customers` column.
const MAX_LOGIN_EMAIL_CHARS: usize = 255;
const MAX_LOGIN_PASSWORD_BYTES: usize = 128;

fn credentials_within_bounds(email: &str, password: &str) -> bool {
    !email.is_empty()
        && email.chars().count() <= MAX_LOGIN_EMAIL_CHARS
        && !password.is_empty()
        && password.len() <= MAX_LOGIN_PASSWORD_BYTES
}

/// Login endpoint
pub async fn login(
    State(state): State<AppState>,
    client_ip: Option<axum::Extension<ClientIp>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, Response> {
    // The client IP was resolved (trusted-proxy aware) by client_ip
    // middleware. Hand-constructed routers without it share the unspecified
    // address; throttling then still applies (fail closed), it just cannot
    // distinguish clients.
    let ip = client_ip
        .map(|ext| ext.0.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));

    // Reserve both budgets before password verification so concurrent attempts
    // cannot race past either limit. Successful authentication clears the pair.
    if let Err(retry_after) = state.login_throttle.check_attempt(ip, &payload.email) {
        tracing::warn!("Login throttled for IP {}", ip);
        return Err(login_throttled(retry_after));
    }

    if !credentials_within_bounds(&payload.email, &payload.password) {
        verify_against_dummy(&state, &payload.password);
        return Err(unauthorized());
    }

    // Find customer by email
    let customer = state
        .customer_service
        .find_by_email(&payload.email)
        .await
        .map_err(|e| e.into_response())?;

    let Some(customer) = customer else {
        // Same cost, same answer as a wrong password: nothing here confirms
        // whether the e-mail is registered.
        verify_against_dummy(&state, &payload.password);
        return Err(unauthorized());
    };

    // Imported accounts without a local password get the same generic 401 as
    // unknown e-mails (again after dummy verification, so timing does not
    // single them out). The password-reset flow is how they set a password
    // and log in.
    let password_hash = match &customer.password_hash {
        Some(hash) => hash.clone(),
        None => {
            verify_against_dummy(&state, &payload.password);
            return Err(unauthorized());
        }
    };

    // Verify the password before revealing anything else about the account
    // (verification status, imported status, existence).
    let (valid, needs_rehash) = state
        .auth_service
        .verify_password(&payload.password, &password_hash)
        .map_err(|e| e.into_response())?;

    if !valid {
        return Err(unauthorized());
    }

    // Success: clear only this (IP, e-mail) pair's failures. The IP attempt
    // budget (volume, not correctness) stays, and other pairs are untouched.
    state.login_throttle.clear_pair(ip, &payload.email);

    // Email must be verified before logging in; this is only revealed to
    // someone who proved they hold the password.
    if !customer.email_verified {
        return Err(EmailVerificationRequiredResponse {
            message: "Please verify your email before logging in.".to_string(),
            requires_email_verification: true,
        }
        .into_response());
    }

    // Rehash password if it was using legacy format (PHPass or bcrypt)
    if needs_rehash {
        let new_hash = state
            .auth_service
            .hash_password(&payload.password)
            .map_err(|e| e.into_response())?;
        if let Err(e) = state
            .customer_service
            .update_password_hash(customer.id, &new_hash)
            .await
        {
            tracing::warn!(
                "Failed to rehash password for customer {}: {}",
                customer.id,
                e
            );
        } else {
            tracing::info!(
                "Password rehashed with Argon2id for customer {}",
                customer.id
            );
        }
    }

    // Generate tokens with role-based permissions
    let access_token = state
        .auth_service
        .generate_access_token(customer.id, &customer.email, &customer.role)
        .map_err(|e| e.into_response())?;
    let refresh_token = state
        .auth_service
        .generate_refresh_token(customer.id, payload.remember_me)
        .map_err(|e| e.into_response())?;

    Ok(Json(LoginResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 24 * 3600, // 24 hours
        customer: CustomerInfo {
            id: customer.id,
            email: customer.email,
            first_name: customer.first_name,
            last_name: customer.last_name,
        },
    }))
}

/// Register endpoint
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), Error> {
    payload.validate()?;

    let password_hash = state.auth_service.hash_password(&payload.password)?;

    let customer = state
        .customer_service
        .create_customer_with_password(
            CreateCustomerRequest {
                email: payload.email.clone(),
                first_name: payload.first_name.clone(),
                last_name: payload.last_name.clone(),
                phone: payload.phone,
                accepts_marketing: false,
                currency: rcommerce_core::models::Currency::USD,
            },
            password_hash,
        )
        .await?;

    // Send welcome email with verification link
    if let Err(e) = send_verification_email(&state, &customer).await {
        tracing::error!("Failed to send welcome email to {}: {}", customer.email, e);
    }

    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            customer: CustomerInfo {
                id: customer.id,
                email: customer.email,
                first_name: customer.first_name,
                last_name: customer.last_name,
            },
            message: "Registration successful. Please check your email to verify your account."
                .to_string(),
        }),
    ))
}

/// Send a verification email to a customer.
async fn send_verification_email(
    state: &AppState,
    customer: &rcommerce_core::models::Customer,
) -> Result<(), Error> {
    let Some(ref notification_service) = state.notification_service else {
        tracing::warn!("Notification service not configured; skipping verification email");
        return Ok(());
    };

    let verification_token = state
        .auth_service
        .generate_email_verification_token(customer.id, &customer.email)?;
    let store_url =
        std::env::var("STOREFRONT_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let verification_link = format!(
        "{}/account/verify-email?token={}",
        store_url, verification_token
    );

    let notification = rcommerce_core::notification::EmailNotificationFactory::verification(
        &customer.email,
        &format!("{} {}", customer.first_name, customer.last_name),
        &verification_link,
        "24 hours",
        &state.branding,
    )?;

    notification_service.send(&notification).await?;
    tracing::info!("Welcome/verification email sent to {}", customer.email);
    Ok(())
}

/// Refresh access token
pub async fn refresh_token(
    State(state): State<AppState>,
    Json(payload): Json<RefreshTokenRequest>,
) -> Result<Json<RefreshTokenResponse>, Error> {
    // Verify refresh token
    let claims = state.auth_service.verify_token(&payload.refresh_token)?;

    // Ensure it's a refresh token
    if claims.token_type != rcommerce_core::services::TokenType::Refresh {
        return Err(Error::unauthorized("Invalid token type"));
    }

    // Fetch customer to get their current role
    let customer = state
        .customer_service
        .find_by_id(claims.sub)
        .await?
        .ok_or_else(|| Error::unauthorized("Customer not found"))?;

    // Generate new access token with role-based permissions
    let access_token =
        state
            .auth_service
            .generate_access_token(claims.sub, &customer.email, &customer.role)?;

    Ok(Json(RefreshTokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: 24 * 3600, // 24 hours
    }))
}

/// Request password reset
/// Generates a reset token and sends it via email
///
/// Security note: the token is never returned in the response in production.
pub async fn request_password_reset(
    State(state): State<AppState>,
    Json(payload): Json<PasswordResetRequest>,
) -> Result<Json<PasswordResetResponse>, Error> {
    // Find customer by email
    let customer = state.customer_service.find_by_email(&payload.email).await?;

    // Always return success even if email not found (security)
    if customer.is_none() {
        tracing::info!(
            "Password reset requested for non-existent email: {}",
            payload.email
        );
        return Ok(Json(PasswordResetResponse {
            message: "If the email exists, a reset link has been sent".to_string(),
            token: None,
        }));
    }

    let customer = customer.unwrap();

    // Generate reset token (short-lived JWT)
    let reset_token = state
        .auth_service
        .generate_password_reset_token(customer.id, &customer.email)?;

    // Send reset email via SMTP if configured
    let store_url =
        std::env::var("STOREFRONT_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let reset_link = format!("{}/account/reset-password?token={}", store_url, reset_token);

    if let Some(ref notification_service) = state.notification_service {
        let notification = rcommerce_core::notification::EmailNotificationFactory::password_reset(
            &customer.email,
            &format!("{} {}", customer.first_name, customer.last_name),
            &reset_token,
            &reset_link,
            "1 hour",
            &state.branding,
        )?;

        if let Err(e) = notification_service.send(&notification).await {
            tracing::error!(
                "Failed to send password reset email to {}: {}",
                customer.email,
                e
            );
            // Still return success to avoid email enumeration, but log the failure
        } else {
            tracing::info!("Password reset email sent to {}", customer.email);
        }
    } else {
        tracing::warn!(
            "SMTP not configured; cannot send password reset email to {}",
            customer.email
        );
    }

    // Only return token in development mode for testing
    let token = if cfg!(debug_assertions) {
        tracing::debug!("Development mode: returning reset token in response");
        Some(reset_token)
    } else {
        None
    };

    Ok(Json(PasswordResetResponse {
        message: "If the email exists, a reset link has been sent".to_string(),
        token,
    }))
}

/// Confirm password reset
/// Validates token and updates password
pub async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(payload): Json<PasswordResetConfirmRequest>,
) -> Result<Json<PasswordResetResponse>, Error> {
    // Validate password strength
    if payload.password.len() < 8 {
        return Err(Error::validation("Password must be at least 8 characters"));
    }

    // Verify reset token
    let claims = state
        .auth_service
        .verify_password_reset_token(&payload.token)?;

    // Hash new password
    let password_hash = state.auth_service.hash_password(&payload.password)?;

    // Update customer password
    state
        .customer_service
        .update_password_hash(claims.sub, &password_hash)
        .await?;

    // Imported customers proving email access via password reset are marked verified.
    state
        .customer_service
        .update_email_verified(claims.sub, true)
        .await?;

    tracing::info!("Password reset successful for customer {}", claims.sub);

    Ok(Json(PasswordResetResponse {
        message: "Password reset successful. Please log in with your new password.".to_string(),
        token: None,
    }))
}

/// Verify email request
#[derive(Debug, Deserialize)]
pub struct VerifyEmailRequest {
    pub token: String,
}

/// Verify email endpoint
pub async fn verify_email(
    State(state): State<AppState>,
    Json(payload): Json<VerifyEmailRequest>,
) -> Result<Json<PasswordResetResponse>, Error> {
    let claims = state
        .auth_service
        .verify_email_verification_token(&payload.token)?;

    state
        .customer_service
        .update_email_verified(claims.sub, true)
        .await?;

    tracing::info!("Email verified for customer {}", claims.sub);

    Ok(Json(PasswordResetResponse {
        message: "Email verified successfully. You can now log in.".to_string(),
        token: None,
    }))
}

/// Resend verification email endpoint
pub async fn resend_verification_email(
    State(state): State<AppState>,
    Json(payload): Json<ResendVerificationRequest>,
) -> Result<Json<ResendVerificationResponse>, Error> {
    let customer = match state.customer_service.find_by_email(&payload.email).await? {
        Some(c) => c,
        None => {
            return Ok(Json(ResendVerificationResponse {
                message:
                    "If an account with that email exists, a verification email has been sent."
                        .to_string(),
            }));
        }
    };

    if customer.email_verified {
        return Ok(Json(ResendVerificationResponse {
            message: "Email is already verified.".to_string(),
        }));
    }

    send_verification_email(&state, &customer).await?;

    Ok(Json(ResendVerificationResponse {
        message: "If an account with that email exists, a verification email has been sent."
            .to_string(),
    }))
}

/// Public auth routes (no API key required)
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/register", post(register))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/password-reset", post(request_password_reset))
        .route("/auth/password-reset/confirm", post(confirm_password_reset))
        .route("/auth/verify-email", post(verify_email))
        .route("/auth/resend-verification", post(resend_verification_email))
}

/// Protected auth routes (API key required)
pub fn protected_router() -> Router<AppState> {
    Router::new()
}

#[cfg(test)]
mod tests {
    use super::RegisterRequest;
    use super::*;

    fn request() -> RegisterRequest {
        RegisterRequest {
            email: "jane@example.com".to_string(),
            password: "correct-horse-battery".to_string(),
            first_name: "Jane".to_string(),
            last_name: "Doe".to_string(),
            phone: Some("+32 2 000 00 00".to_string()),
        }
    }

    #[test]
    fn accepts_a_normal_registration() {
        assert!(request().validate().is_ok());
    }

    #[test]
    fn password_length_is_bounded() {
        let mut r = request();
        r.password = "short".to_string();
        assert!(r.validate().is_err());
        r.password = "x".repeat(129);
        assert!(r.validate().is_err());
        r.password = "x".repeat(128);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn fields_must_fit_their_columns() {
        let mut r = request();
        r.first_name = "A".repeat(101);
        assert!(r.validate().is_err());

        let mut r = request();
        r.last_name = "é".repeat(100); // limits count characters, not bytes
        assert!(r.validate().is_ok());

        let mut r = request();
        r.email = format!("{}@example.com", "a".repeat(250));
        assert!(r.validate().is_err());

        let mut r = request();
        r.phone = Some("1".repeat(51));
        assert!(r.validate().is_err());
    }

    #[test]
    fn login_credentials_are_bounded() {
        assert!(credentials_within_bounds("a@example.com", "secret123"));
        assert!(!credentials_within_bounds("", "secret123"));
        assert!(!credentials_within_bounds("a@example.com", ""));
        assert!(!credentials_within_bounds(
            &format!("{}@x.com", "a".repeat(250)),
            "secret123"
        ));
        assert!(!credentials_within_bounds(
            "a@example.com",
            &"x".repeat(129)
        ));
        assert!(credentials_within_bounds("a@example.com", &"x".repeat(128)));
    }

    #[test]
    fn throttled_response_carries_retry_after_only() {
        let response = login_throttled(42);
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "42");
        // No body fields distinguishing account state — generic shape only.
        let body = response.into_body();
        let bytes = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async { axum::body::to_bytes(body, usize::MAX).await.unwrap() });
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["error"]["code"].as_str().unwrap().contains("too_many"));
        assert!(json["error"].get("email").is_none());
    }
}
