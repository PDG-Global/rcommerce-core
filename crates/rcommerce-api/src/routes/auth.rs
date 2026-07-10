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

/// Login endpoint
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, Response> {
    // Find customer by email
    let customer = state
        .customer_service
        .find_by_email(&payload.email)
        .await
        .map_err(|e| e.into_response())?
        .ok_or_else(|| Error::unauthorized("Invalid email or password").into_response())?;

    // Imported accounts without a local password must reset their password before logging in.
    if customer.is_imported && customer.password_hash.is_none() {
        return Err(ImportedAccountResponse {
            message: "This account was imported. Please reset your password to continue.".to_string(),
            requires_password_reset: true,
        }
        .into_response());
    }

    // Email must be verified before logging in.
    if !customer.email_verified {
        return Err(EmailVerificationRequiredResponse {
            message: "Please verify your email before logging in.".to_string(),
            requires_email_verification: true,
        }
        .into_response());
    }

    // Verify password
    let password_hash = customer
        .password_hash
        .as_ref()
        .ok_or_else(|| Error::unauthorized("Invalid email or password").into_response())?;

    let (valid, needs_rehash) = state
        .auth_service
        .verify_password(&payload.password, password_hash)
        .map_err(|e| e.into_response())?;

    if !valid {
        return Err(Error::unauthorized("Invalid email or password").into_response());
    }

    // Rehash password if it was using legacy format (PHPass or bcrypt)
    if needs_rehash {
        let new_hash = state.auth_service.hash_password(&payload.password).map_err(|e| e.into_response())?;
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
    let access_token =
        state
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
    if payload.password.len() < 8 {
        return Err(Error::validation("Password must be at least 8 characters"));
    }

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
            message: "Registration successful. Please check your email to verify your account.".to_string(),
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
    let store_url = std::env::var("STOREFRONT_URL")
        .unwrap_or_else(|_| "https://demo.rcommerce.app".to_string());
    let verification_link = format!(
        "{}/account/verify-email?token={}",
        store_url, verification_token
    );

    let notification = Notification::new(
        NotificationChannel::Email,
        customer.email.clone(),
        "Welcome to rCommerce — please verify your email".to_string(),
        format!(
            "Hi {},\n\nWelcome to rCommerce! Please verify your email address by clicking the link below:\n\n{}\n\nThis link expires in 24 hours.\n\nIf you didn't create an account, you can safely ignore this email.",
            customer.first_name, verification_link
        ),
    )
    .with_priority(NotificationPriority::High)
    .with_html_body(format!(
        "<h1>Welcome to rCommerce!</h1><p>Hi {},</p><p>Please verify your email address by clicking the link below:</p><p><a href=\"{}\">Verify Email</a></p><p>This link expires in 24 hours.</p><p>If you didn't create an account, you can safely ignore this email.</p>",
        customer.first_name, verification_link
    ));

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
    let store_url = std::env::var("STOREFRONT_URL").unwrap_or_else(|_| "https://demo.rcommerce.app".to_string());
    let reset_link = format!("{}/account/reset-password?token={}", store_url, reset_token);

    if let Some(ref notification_service) = state.notification_service {
        let notification = Notification::new(
            NotificationChannel::Email,
            customer.email.clone(),
            "Reset your rCommerce password".to_string(),
            format!(
                "Hi {},\n\nWe received a request to reset your password. Click the link below to set a new password:\n\n{}\n\nThis link expires in 1 hour. If you didn't request this, you can safely ignore this email.",
                customer.first_name,
                reset_link
            ),
        )
        .with_priority(NotificationPriority::High)
        .with_html_body(format!(
            "<p>Hi {},</p><p>We received a request to reset your password. Click the link below to set a new password:</p><p><a href=\"{}\">Reset Password</a></p><p>This link expires in 1 hour. If you didn't request this, you can safely ignore this email.</p>",
            customer.first_name,
            reset_link
        ));

        if let Err(e) = notification_service.send(&notification).await {
            tracing::error!("Failed to send password reset email to {}: {}", customer.email, e);
            // Still return success to avoid email enumeration, but log the failure
        } else {
            tracing::info!("Password reset email sent to {}", customer.email);
        }
    } else {
        tracing::warn!("SMTP not configured; cannot send password reset email to {}", customer.email);
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
                message: "If an account with that email exists, a verification email has been sent.".to_string(),
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
        message: "If an account with that email exists, a verification email has been sent.".to_string(),
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
