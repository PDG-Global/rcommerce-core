use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

use rcommerce_core::{
    models::{CreateCustomerRequest, Customer},
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

/// Argon2 hash verified for unknown e-mails, so they cost as much as known ones
static DUMMY_PASSWORD_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Login endpoint
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, Response> {
    let (customer, needs_rehash) = check_credentials(&state, &payload).await.map_err(|e| *e)?;

    // Email must be verified before logging in (only told to the password holder).
    if !customer.email_verified {
        return Err(EmailVerificationRequiredResponse {
            message: "Please verify your email before logging in.".to_string(),
            requires_email_verification: true,
        }
        .into_response());
    }

    // Rehash password if it was using legacy format (PHPass or bcrypt)
    if needs_rehash {
        rehash_password(&state, customer.id, &payload.password)
            .await
            .map_err(|e| e.into_response())?;
    }

    login_response(&state, customer, payload.remember_me)
        .map(Json)
        .map_err(|e| e.into_response())
}

/// The account matching the login credentials, and whether its password hash
/// uses a legacy format. An unknown e-mail costs the same hashing work and gets
/// the same answer as a wrong password.
async fn check_credentials(
    state: &AppState,
    payload: &LoginRequest,
) -> Result<(Customer, bool), Box<Response>> {
    let invalid = || Box::new(Error::unauthorized("Invalid email or password").into_response());
    let customer = state
        .customer_service
        .find_by_email(&payload.email)
        .await
        .map_err(|e| Box::new(e.into_response()))?;
    let Some(customer) = customer else {
        verify_dummy_password(state, &payload.password).map_err(|e| Box::new(e.into_response()))?;
        return Err(invalid());
    };

    // Imported accounts without a local password must reset their password before logging in.
    if customer.is_imported && customer.password_hash.is_none() {
        return Err(Box::new(
            ImportedAccountResponse {
                message: "This account was imported. Please reset your password to continue."
                    .to_string(),
                requires_password_reset: true,
            }
            .into_response(),
        ));
    }

    // Verify the password before revealing anything else about the account
    let password_hash = customer.password_hash.as_ref().ok_or_else(invalid)?;
    let (valid, needs_rehash) = state
        .auth_service
        .verify_password(&payload.password, password_hash)
        .map_err(|e| Box::new(e.into_response()))?;
    if !valid {
        return Err(invalid());
    }
    Ok((customer, needs_rehash))
}

/// Verify a password against a dummy hash: the same work as for a real
/// account, so the response time does not reveal which e-mails are registered
fn verify_dummy_password(state: &AppState, password: &str) -> Result<(), Error> {
    let dummy = match DUMMY_PASSWORD_HASH.get() {
        Some(hash) => hash,
        None => {
            let hash = state
                .auth_service
                .hash_password("rcommerce-dummy-password")?;
            DUMMY_PASSWORD_HASH.get_or_init(|| hash)
        }
    };
    let _ = state.auth_service.verify_password(password, dummy);
    Ok(())
}

/// Store an Argon2id hash in place of a legacy one (PHPass or bcrypt). A
/// failure to store it is logged and the login goes on.
async fn rehash_password(state: &AppState, customer_id: Uuid, password: &str) -> Result<(), Error> {
    let new_hash = state.auth_service.hash_password(password)?;
    if let Err(e) = state
        .customer_service
        .update_password_hash(customer_id, &new_hash)
        .await
    {
        tracing::warn!(
            "Failed to rehash password for customer {}: {}",
            customer_id,
            e
        );
    } else {
        tracing::info!(
            "Password rehashed with Argon2id for customer {}",
            customer_id
        );
    }
    Ok(())
}

/// Access and refresh tokens, with role-based permissions, for a logged-in customer
fn login_response(
    state: &AppState,
    customer: Customer,
    remember_me: bool,
) -> Result<LoginResponse, Error> {
    let access_token =
        state
            .auth_service
            .generate_access_token(customer.id, &customer.email, &customer.role)?;
    let refresh_token = state
        .auth_service
        .generate_refresh_token(customer.id, remember_me)?;

    Ok(LoginResponse {
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
    })
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
}
