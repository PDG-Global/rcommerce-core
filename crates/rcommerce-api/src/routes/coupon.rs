//! Coupon API Routes
//!
//! Provides endpoints for coupon management:
//! - Create, update, delete coupons (admin)
//! - List coupons
//! - Validate coupons

use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use rcommerce_core::models::{CreateCouponInput, DiscountType, UpdateCouponInput};

/// Request body for creating a coupon
#[derive(Debug, Deserialize)]
pub struct CreateCouponRequest {
    pub code: String,
    pub description: Option<String>,
    pub discount_type: String,
    pub discount_value: String,
    pub minimum_purchase: Option<String>,
    pub maximum_discount: Option<String>,
    pub starts_at: Option<String>,
    pub expires_at: Option<String>,
    pub usage_limit: Option<i32>,
    pub usage_limit_per_customer: Option<i32>,
}

fn parse_discount_type(value: &str) -> Result<DiscountType, rcommerce_core::Error> {
    match value.to_lowercase().as_str() {
        "percentage" => Ok(DiscountType::Percentage),
        "fixed_amount" => Ok(DiscountType::FixedAmount),
        "free_shipping" => Ok(DiscountType::FreeShipping),
        "buy_x_get_y" => Ok(DiscountType::BuyXGetY),
        _ => Err(rcommerce_core::Error::validation(format!(
            "Invalid discount type: {}",
            value
        ))),
    }
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal, rcommerce_core::Error> {
    value
        .parse()
        .map_err(|_| rcommerce_core::Error::validation(format!("Invalid {}: {}", field, value)))
}

fn parse_datetime(value: &str, field: &str) -> Result<DateTime<Utc>, rcommerce_core::Error> {
    value
        .parse::<DateTime<Utc>>()
        .map_err(|_| rcommerce_core::Error::validation(format!("Invalid {}: {}", field, value)))
}

impl TryFrom<CreateCouponRequest> for CreateCouponInput {
    type Error = rcommerce_core::Error;

    fn try_from(req: CreateCouponRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            code: req.code,
            description: req.description,
            discount_type: parse_discount_type(&req.discount_type)?,
            discount_value: parse_decimal(&req.discount_value, "discount_value")?,
            minimum_purchase: req
                .minimum_purchase
                .as_deref()
                .map(|v| parse_decimal(v, "minimum_purchase"))
                .transpose()?,
            maximum_discount: req
                .maximum_discount
                .as_deref()
                .map(|v| parse_decimal(v, "maximum_discount"))
                .transpose()?,
            starts_at: req
                .starts_at
                .as_deref()
                .map(|v| parse_datetime(v, "starts_at"))
                .transpose()?,
            expires_at: req
                .expires_at
                .as_deref()
                .map(|v| parse_datetime(v, "expires_at"))
                .transpose()?,
            usage_limit: req.usage_limit,
            usage_limit_per_customer: req.usage_limit_per_customer,
            applies_to_specific_products: false,
            applies_to_specific_collections: false,
            can_combine: false,
        })
    }
}

/// Create a new coupon (admin only)
pub async fn create_coupon(
    State(state): State<AppState>,
    Json(request): Json<CreateCouponRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), rcommerce_core::Error> {
    let input: CreateCouponInput = request.try_into()?;
    let coupon = state.coupon_service.create_coupon(input, None).await?;

    Ok((StatusCode::CREATED, Json(serde_json::to_value(coupon)?)))
}

/// List all coupons (admin only)
pub async fn list_coupons(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let coupons = state.coupon_service.list_coupons(false).await?;

    Ok(Json(serde_json::json!({
        "coupons": coupons,
        "pagination": {
            "page": 1,
            "per_page": coupons.len() as i64,
            "total": coupons.len() as i64,
            "total_pages": 1,
        }
    })))
}

/// Get coupon by ID (admin only)
pub async fn get_coupon(
    State(state): State<AppState>,
    Path(coupon_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let coupon = state.coupon_service.get_coupon(coupon_id).await?;

    Ok(Json(serde_json::to_value(coupon)?))
}

/// Request body for updating a coupon
#[derive(Debug, Deserialize)]
pub struct UpdateCouponRequest {
    pub description: Option<String>,
    pub is_active: Option<bool>,
    pub discount_value: Option<String>,
    pub minimum_purchase: Option<String>,
    pub maximum_discount: Option<String>,
    pub starts_at: Option<String>,
    pub expires_at: Option<String>,
    pub usage_limit: Option<i32>,
}

impl TryFrom<UpdateCouponRequest> for UpdateCouponInput {
    type Error = rcommerce_core::Error;

    fn try_from(req: UpdateCouponRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            description: req.description,
            is_active: req.is_active,
            discount_value: req
                .discount_value
                .as_deref()
                .map(|v| parse_decimal(v, "discount_value"))
                .transpose()?,
            minimum_purchase: req
                .minimum_purchase
                .as_deref()
                .map(|v| parse_decimal(v, "minimum_purchase"))
                .transpose()?,
            maximum_discount: req
                .maximum_discount
                .as_deref()
                .map(|v| parse_decimal(v, "maximum_discount"))
                .transpose()?,
            starts_at: req
                .starts_at
                .as_deref()
                .map(|v| parse_datetime(v, "starts_at"))
                .transpose()?,
            expires_at: req
                .expires_at
                .as_deref()
                .map(|v| parse_datetime(v, "expires_at"))
                .transpose()?,
            usage_limit: req.usage_limit,
        })
    }
}

/// Update a coupon (admin only)
pub async fn update_coupon(
    State(state): State<AppState>,
    Path(coupon_id): Path<Uuid>,
    Json(request): Json<UpdateCouponRequest>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let input: UpdateCouponInput = request.try_into()?;
    let coupon = state.coupon_service.update_coupon(coupon_id, input).await?;

    Ok(Json(serde_json::to_value(coupon)?))
}

/// Delete a coupon (admin only)
pub async fn delete_coupon(
    State(state): State<AppState>,
    Path(coupon_id): Path<Uuid>,
) -> Result<StatusCode, rcommerce_core::Error> {
    state.coupon_service.delete_coupon(coupon_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Request body for validating a coupon
#[derive(Debug, Deserialize)]
pub struct ValidateCouponRequest {
    pub code: String,
    pub cart_id: Uuid,
}

/// Validate a coupon without applying it
pub async fn validate_coupon(
    State(state): State<AppState>,
    Json(request): Json<ValidateCouponRequest>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let result = state
        .coupon_service
        .validate_coupon(&request.code, request.cart_id)
        .await?;

    if result.is_valid() {
        let calculation = state
            .coupon_service
            .get_discount_calculation(&request.code, Decimal::ZERO)
            .await?;

        Ok(Json(serde_json::json!({
            "valid": true,
            "coupon": {
                "code": request.code,
            },
            "discount_calculation": calculation,
        })))
    } else {
        Ok(Json(serde_json::json!({
            "valid": false,
            "error_code": format!("{:?}", result),
            "error_message": result.error_message(),
        })))
    }
}

/// Get coupon statistics (admin only)
pub async fn get_coupon_stats(
    State(state): State<AppState>,
    Path(coupon_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, rcommerce_core::Error> {
    let stats = state.coupon_service.get_coupon_stats(coupon_id).await?;

    Ok(Json(serde_json::json!({
        "coupon_id": stats.coupon_id,
        "code": stats.code,
        "total_usage": stats.total_usage,
        "total_discount_amount": stats.total_discount_amount,
        "remaining_uses": stats.remaining_uses,
    })))
}

/// Router for coupon routes
pub fn router() -> Router<AppState> {
    Router::new()
        // Coupon CRUD
        .route("/coupons", get(list_coupons).post(create_coupon))
        .route(
            "/coupons/:coupon_id",
            get(get_coupon).put(update_coupon).delete(delete_coupon),
        )
        // Coupon validation
        .route("/coupons/validate", post(validate_coupon))
        // Coupon stats
        .route("/coupons/:coupon_id/stats", get(get_coupon_stats))
}
