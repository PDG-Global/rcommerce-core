//! Statistics Service
//!
//! Business logic layer for statistics and analytics operations.
//! Provides data aggregation and formatting.

use chrono::{DateTime, Duration, Utc};

use crate::{
    repository::statistics_repository::{
        CustomerStatistics, DashboardMetrics, OrderStatistics, Period, PeriodComparison,
        PgStatisticsRepository, ProductPerformance, RevenueDataPoint, SalesSummary,
        StatisticsRepository, TrendComparison, TrendDirection,
    },
    Result,
};

/// Statistics service for business logic
pub struct StatisticsService<R: StatisticsRepository> {
    repository: R,
}

impl StatisticsService<PgStatisticsRepository> {
    /// Create a new statistics service with PostgreSQL repository
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        let repository = PgStatisticsRepository::new(pool);
        Self { repository }
    }

    /// Create with a pre-configured repository
    pub fn with_repository(repository: PgStatisticsRepository) -> Self {
        Self { repository }
    }
}

impl<R: StatisticsRepository> StatisticsService<R> {
    /// Get sales summary
    pub async fn get_sales_summary(
        &self,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
        period: Period,
    ) -> Result<Vec<SalesSummary>> {
        self.repository
            .get_sales_summary(date_from, date_to, period)
            .await
    }

    /// Get order statistics
    pub async fn get_order_statistics(
        &self,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
    ) -> Result<OrderStatistics> {
        self.repository
            .get_order_statistics(date_from, date_to)
            .await
    }

    /// Get product performance
    pub async fn get_product_performance(
        &self,
        limit: i64,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<Vec<ProductPerformance>> {
        self.repository
            .get_product_performance(limit, date_from, date_to)
            .await
    }

    /// Get customer statistics
    pub async fn get_customer_statistics(&self) -> Result<CustomerStatistics> {
        self.repository.get_customer_statistics().await
    }

    /// Get dashboard metrics
    pub async fn get_dashboard_metrics(&self) -> Result<DashboardMetrics> {
        self.repository.get_dashboard_metrics().await
    }

    /// Get revenue by period for charting
    pub async fn get_revenue_by_period(
        &self,
        period: Period,
        periods_count: i32,
    ) -> Result<Vec<RevenueDataPoint>> {
        self.repository
            .get_revenue_by_period(period, periods_count)
            .await
    }

    /// Compare current period to previous period
    pub async fn compare_periods(&self, days: i64) -> Result<PeriodComparison> {
        let current_to = Utc::now();
        let current_from = current_to - Duration::days(days);

        self.repository
            .compare_periods(current_from, current_to)
            .await
    }

    /// Get comprehensive dashboard data
    pub async fn get_full_dashboard(&self) -> Result<FullDashboardData> {
        let metrics = self.get_dashboard_metrics().await?;
        let comparison = self.compare_periods(30).await?;
        let revenue_trend = self.get_revenue_by_period(Period::Day, 30).await?;
        let top_products = self.get_product_performance(5, None, None).await?;

        Ok(FullDashboardData {
            metrics,
            comparison,
            revenue_trend,
            top_products,
        })
    }

    /// Get sales report with all details
    pub async fn get_sales_report(
        &self,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
        period: Period,
    ) -> Result<SalesReport> {
        let summary = self.get_sales_summary(date_from, date_to, period).await?;
        let order_stats = self.get_order_statistics(date_from, date_to).await?;
        let comparison = self.repository.compare_periods(date_from, date_to).await?;

        Ok(SalesReport {
            date_from,
            date_to,
            period,
            summary,
            order_statistics: order_stats,
            comparison,
        })
    }
}

/// Full dashboard data response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FullDashboardData {
    pub metrics: DashboardMetrics,
    pub comparison: PeriodComparison,
    pub revenue_trend: Vec<RevenueDataPoint>,
    pub top_products: Vec<ProductPerformance>,
}

/// Sales report response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SalesReport {
    pub date_from: DateTime<Utc>,
    pub date_to: DateTime<Utc>,
    pub period: Period,
    pub summary: Vec<SalesSummary>,
    pub order_statistics: OrderStatistics,
    pub comparison: PeriodComparison,
}

/// Statistics response wrapper for API
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatisticsResponse<T> {
    pub data: T,
    pub generated_at: DateTime<Utc>,
    pub cache_hit: bool,
}

impl<T> StatisticsResponse<T> {
    pub fn new(data: T, cache_hit: bool) -> Self {
        Self {
            data,
            generated_at: Utc::now(),
            cache_hit,
        }
    }
}

/// Helper to format trend data for display
pub fn format_trend(trend: &TrendComparison) -> String {
    let direction = match trend.trend_direction {
        TrendDirection::Up => "↑",
        TrendDirection::Down => "↓",
        TrendDirection::Flat => "→",
    };

    format!(
        "{} {:.1}% (${:.2})",
        direction, trend.change_percentage, trend.change_amount
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_format_trend() {
        let trend = TrendComparison {
            current_value: Decimal::from(150),
            previous_value: Decimal::from(100),
            change_amount: Decimal::from(50),
            change_percentage: 50.0,
            trend_direction: TrendDirection::Up,
        };

        let formatted = format_trend(&trend);
        assert!(formatted.contains("↑"));
        assert!(formatted.contains("50.0%"));
    }
}
