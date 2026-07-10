//! Statistics API Routes
//!
//! Admin-only endpoints for retrieving analytics and statistics data.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use rcommerce_core::{
    repository::statistics_repository::{
        CustomerStatistics, DashboardMetrics, OrderStatistics, Period, PeriodComparison,
        PgStatisticsRepository, ProductPerformance, RevenueDataPoint,
    },
    services::statistics_service::{SalesReport, StatisticsService},
};

/// Query parameters for date range filtering
#[derive(Debug, Deserialize)]
pub struct DateRangeQuery {
    /// Start date (ISO 8601 format)
    pub from: Option<DateTime<Utc>>,
    /// End date (ISO 8601 format)
    pub to: Option<DateTime<Utc>>,
}

/// Query parameters for sales statistics
#[derive(Debug, Deserialize)]
pub struct SalesQuery {
    /// Start date (ISO 8601 format)
    pub from: Option<DateTime<Utc>>,
    /// End date (ISO 8601 format)
    pub to: Option<DateTime<Utc>>,
    /// Grouping period (day, week, month, year)
    #[serde(default = "default_period")]
    pub period: String,
}

fn default_period() -> String {
    "day".to_string()
}

/// Query parameters for product performance
#[derive(Debug, Deserialize)]
pub struct ProductPerformanceQuery {
    /// Number of products to return
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Start date filter
    pub from: Option<DateTime<Utc>>,
    /// End date filter
    pub to: Option<DateTime<Utc>>,
}

fn default_limit() -> i64 {
    10
}

/// API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: Utc::now(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
            timestamp: Utc::now(),
        }
    }
}

/// Dashboard metrics response
#[derive(Debug, Serialize)]
pub struct DashboardResponse {
    pub metrics: DashboardMetrics,
    pub trends: TrendSummary,
}

#[derive(Debug, Serialize)]
pub struct TrendSummary {
    pub revenue_change_percent: f64,
    pub orders_change_percent: f64,
    pub customers_change_percent: f64,
    pub aov_change_percent: f64,
}

/// Get dashboard overview
///
/// GET /api/v1/admin/statistics/dashboard
pub async fn get_dashboard(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<DashboardResponse>>, StatusCode> {
    let service = create_statistics_service(&state);

    match service.get_full_dashboard().await {
        Ok(data) => {
            let trends = TrendSummary {
                revenue_change_percent: data.comparison.revenue.change_percentage,
                orders_change_percent: data.comparison.orders.change_percentage,
                customers_change_percent: data.comparison.customers.change_percentage,
                aov_change_percent: data.comparison.average_order_value.change_percentage,
            };

            let response = DashboardResponse {
                metrics: data.metrics,
                trends,
            };

            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            tracing::error!("Failed to get dashboard metrics: {}", e);
            Ok(Json(ApiResponse::error(
                "Failed to retrieve dashboard metrics",
            )))
        }
    }
}

/// Get sales summary statistics
///
/// GET /api/v1/admin/statistics/sales?from=&to=&period=
pub async fn get_sales(
    State(state): State<AppState>,
    Query(query): Query<SalesQuery>,
) -> Result<Json<ApiResponse<SalesReport>>, StatusCode> {
    let service = create_statistics_service(&state);

    // Parse period
    let period = match query.period.parse::<Period>() {
        Ok(p) => p,
        Err(_) => {
            return Ok(Json(ApiResponse::error(
                "Invalid period. Use: day, week, month, year",
            )));
        }
    };

    // Default date range: last 30 days
    let date_to = query.to.unwrap_or_else(Utc::now);
    let date_from = query.from.unwrap_or_else(|| date_to - Duration::days(30));

    // Validate date range
    if date_from > date_to {
        return Ok(Json(ApiResponse::error(
            "Invalid date range: from must be before to",
        )));
    }

    match service.get_sales_report(date_from, date_to, period).await {
        Ok(report) => Ok(Json(ApiResponse::success(report))),
        Err(e) => {
            tracing::error!("Failed to get sales statistics: {}", e);
            Ok(Json(ApiResponse::error(
                "Failed to retrieve sales statistics",
            )))
        }
    }
}

/// Get order statistics
///
/// GET /api/v1/admin/statistics/orders?from=&to=
pub async fn get_orders(
    State(state): State<AppState>,
    Query(query): Query<DateRangeQuery>,
) -> Result<Json<ApiResponse<OrderStatistics>>, StatusCode> {
    let service = create_statistics_service(&state);

    // Default date range: last 30 days
    let date_to = query.to.unwrap_or_else(Utc::now);
    let date_from = query.from.unwrap_or_else(|| date_to - Duration::days(30));

    match service.get_order_statistics(date_from, date_to).await {
        Ok(stats) => Ok(Json(ApiResponse::success(stats))),
        Err(e) => {
            tracing::error!("Failed to get order statistics: {}", e);
            Ok(Json(ApiResponse::error(
                "Failed to retrieve order statistics",
            )))
        }
    }
}

/// Get product performance statistics
///
/// GET /api/v1/admin/statistics/products?limit=&from=&to=
pub async fn get_products(
    State(state): State<AppState>,
    Query(query): Query<ProductPerformanceQuery>,
) -> Result<Json<ApiResponse<Vec<ProductPerformance>>>, StatusCode> {
    let service = create_statistics_service(&state);

    match service
        .get_product_performance(query.limit, query.from, query.to)
        .await
    {
        Ok(products) => Ok(Json(ApiResponse::success(products))),
        Err(e) => {
            tracing::error!("Failed to get product performance: {}", e);
            Ok(Json(ApiResponse::error(
                "Failed to retrieve product performance",
            )))
        }
    }
}

/// Get customer statistics
///
/// GET /api/v1/admin/statistics/customers
pub async fn get_customers(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CustomerStatistics>>, StatusCode> {
    let service = create_statistics_service(&state);

    match service.get_customer_statistics().await {
        Ok(stats) => Ok(Json(ApiResponse::success(stats))),
        Err(e) => {
            tracing::error!("Failed to get customer statistics: {}", e);
            Ok(Json(ApiResponse::error(
                "Failed to retrieve customer statistics",
            )))
        }
    }
}

/// Get revenue trend data for charts
///
/// GET /api/v1/admin/statistics/revenue?period=&count=
#[derive(Debug, Deserialize)]
pub struct RevenueQuery {
    #[serde(default = "default_revenue_period")]
    pub period: String,
    #[serde(default = "default_revenue_count")]
    pub count: i32,
}

fn default_revenue_period() -> String {
    "day".to_string()
}

fn default_revenue_count() -> i32 {
    30
}

pub async fn get_revenue(
    State(state): State<AppState>,
    Query(query): Query<RevenueQuery>,
) -> Result<Json<ApiResponse<Vec<RevenueDataPoint>>>, StatusCode> {
    let service = create_statistics_service(&state);

    // Parse period
    let period = match query.period.parse::<Period>() {
        Ok(p) => p,
        Err(_) => {
            return Ok(Json(ApiResponse::error(
                "Invalid period. Use: day, week, month, year",
            )));
        }
    };

    match service.get_revenue_by_period(period, query.count).await {
        Ok(data) => Ok(Json(ApiResponse::success(data))),
        Err(e) => {
            tracing::error!("Failed to get revenue data: {}", e);
            Ok(Json(ApiResponse::error("Failed to retrieve revenue data")))
        }
    }
}

/// Get period comparison data
///
/// GET /api/v1/admin/statistics/compare?days=
#[derive(Debug, Deserialize)]
pub struct CompareQuery {
    #[serde(default = "default_compare_days")]
    pub days: i64,
}

fn default_compare_days() -> i64 {
    30
}

pub async fn get_comparison(
    State(state): State<AppState>,
    Query(query): Query<CompareQuery>,
) -> Result<Json<ApiResponse<PeriodComparison>>, StatusCode> {
    let service = create_statistics_service(&state);

    match service.compare_periods(query.days).await {
        Ok(comparison) => Ok(Json(ApiResponse::success(comparison))),
        Err(e) => {
            tracing::error!("Failed to get period comparison: {}", e);
            Ok(Json(ApiResponse::error(
                "Failed to retrieve comparison data",
            )))
        }
    }
}

/// Create statistics service from app state
fn create_statistics_service(state: &AppState) -> StatisticsService<PgStatisticsRepository> {
    let repository = PgStatisticsRepository::new(state.db.pool().clone());
    let service = StatisticsService::with_repository(repository);

    // Add cache if available
    if let Some(ref _redis) = state.redis {
        // Convert RedisPool to Arc<dyn Cache> if needed
        // For now, we skip caching if the cache type doesn't match
        // In a real implementation, you'd ensure the cache implements the Cache trait
    }

    service
}

/// Router for statistics routes
///
/// All routes are prefixed with /api/v1/admin/statistics
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/statistics/dashboard", get(get_dashboard))
        .route("/admin/statistics/sales", get(get_sales))
        .route("/admin/statistics/orders", get(get_orders))
        .route("/admin/statistics/products", get(get_products))
        .route("/admin/statistics/customers", get(get_customers))
        .route("/admin/statistics/revenue", get(get_revenue))
        .route("/admin/statistics/compare", get(get_comparison))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_period() {
        assert_eq!(default_period(), "day");
    }

    #[test]
    fn test_default_limit() {
        assert_eq!(default_limit(), 10);
    }

    #[test]
    fn test_api_response_success() {
        let response: ApiResponse<String> = ApiResponse::success("test data".to_string());
        assert!(response.success);
        assert!(response.data.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_api_response_error() {
        let response: ApiResponse<String> = ApiResponse::error("Something went wrong");
        assert!(!response.success);
        assert!(response.data.is_none());
        assert!(response.error.is_some());
    }
}
