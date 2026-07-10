//! Statistics Repository
//!
//! Provides database access for analytics and statistics queries.

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Utc};
use rust_decimal::Decimal;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use crate::{Error, Result};

/// Time period for grouping statistics
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
}

impl Period {
    pub fn as_str(&self) -> &'static str {
        match self {
            Period::Day => "day",
            Period::Week => "week",
            Period::Month => "month",
            Period::Year => "year",
        }
    }
}

impl std::str::FromStr for Period {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "day" => Ok(Period::Day),
            "week" => Ok(Period::Week),
            "month" => Ok(Period::Month),
            "year" => Ok(Period::Year),
            _ => Err(Error::validation(format!("Invalid period: {}", s))),
        }
    }
}

/// Sales summary for a period
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SalesSummary {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_revenue: Decimal,
    pub total_orders: i64,
    pub total_items_sold: i64,
    pub average_order_value: Decimal,
}

/// Order statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrderStatistics {
    pub total_orders: i64,
    pub total_revenue: Decimal,
    pub average_order_value: Decimal,
    pub status_breakdown: Vec<StatusCount>,
    pub payment_status_breakdown: Vec<StatusCount>,
    pub fulfillment_status_breakdown: Vec<StatusCount>,
}

/// Status count for breakdowns
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatusCount {
    pub status: String,
    pub count: i64,
    pub revenue: Decimal,
}

/// Product performance metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProductPerformance {
    pub product_id: Uuid,
    pub product_name: String,
    pub sku: Option<String>,
    pub units_sold: i64,
    pub revenue: Decimal,
    pub orders_count: i64,
}

/// Customer statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomerStatistics {
    pub total_customers: i64,
    pub new_customers_period: i64,
    pub returning_customers: i64,
    pub customers_with_orders: i64,
    pub average_orders_per_customer: f64,
    pub average_customer_lifetime_value: Decimal,
}

/// Dashboard metrics for quick overview
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardMetrics {
    pub total_revenue: Decimal,
    pub total_orders: i64,
    pub total_customers: i64,
    pub total_products: i64,
    pub pending_orders: i64,
    pub revenue_today: Decimal,
    pub orders_today: i64,
    pub revenue_this_month: Decimal,
    pub orders_this_month: i64,
}

/// Revenue data point for time series
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RevenueDataPoint {
    pub period: String,
    pub revenue: Decimal,
    pub orders: i64,
    pub date: DateTime<Utc>,
}

/// Trend comparison data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrendComparison {
    pub current_value: Decimal,
    pub previous_value: Decimal,
    pub change_amount: Decimal,
    pub change_percentage: f64,
    pub trend_direction: TrendDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrendDirection {
    Up,
    Down,
    Flat,
}

/// Statistics repository trait
#[async_trait]
pub trait StatisticsRepository: Send + Sync {
    /// Get sales summary for a date range grouped by period
    async fn get_sales_summary(
        &self,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
        period: Period,
    ) -> Result<Vec<SalesSummary>>;

    /// Get order statistics for a date range
    async fn get_order_statistics(
        &self,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
    ) -> Result<OrderStatistics>;

    /// Get top performing products
    async fn get_product_performance(
        &self,
        limit: i64,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<Vec<ProductPerformance>>;

    /// Get customer statistics
    async fn get_customer_statistics(&self) -> Result<CustomerStatistics>;

    /// Get dashboard metrics (quick overview)
    async fn get_dashboard_metrics(&self) -> Result<DashboardMetrics>;

    /// Get revenue by period for charting
    async fn get_revenue_by_period(
        &self,
        period: Period,
        periods_count: i32,
    ) -> Result<Vec<RevenueDataPoint>>;

    /// Compare current period to previous period
    async fn compare_periods(
        &self,
        current_from: DateTime<Utc>,
        current_to: DateTime<Utc>,
    ) -> Result<PeriodComparison>;
}

/// Period comparison result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeriodComparison {
    pub revenue: TrendComparison,
    pub orders: TrendComparison,
    pub customers: TrendComparison,
    pub average_order_value: TrendComparison,
}

/// PostgreSQL implementation of StatisticsRepository
pub struct PgStatisticsRepository {
    pool: Pool<Postgres>,
}

impl PgStatisticsRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Helper to get date truncation expression for PostgreSQL
    fn date_trunc(&self, period: Period) -> &'static str {
        match period {
            Period::Day => "day",
            Period::Week => "week",
            Period::Month => "month",
            Period::Year => "year",
        }
    }
}

#[async_trait]
impl StatisticsRepository for PgStatisticsRepository {
    async fn get_sales_summary(
        &self,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
        period: Period,
    ) -> Result<Vec<SalesSummary>> {
        let trunc = self.date_trunc(period);

        let rows = sqlx::query(&format!(
            r#"
            SELECT 
                DATE_TRUNC('{}', created_at) as period_start,
                COALESCE(SUM(total), 0) as total_revenue,
                COUNT(*) as total_orders,
                COALESCE(SUM((SELECT SUM(quantity) FROM order_items WHERE order_id = orders.id)), 0) as total_items_sold
            FROM orders
            WHERE created_at >= $1 AND created_at <= $2
            AND status NOT IN ('cancelled', 'refunded')
            GROUP BY DATE_TRUNC('{}', created_at)
            ORDER BY period_start
            "#,
            trunc, trunc
        ))
        .bind(date_from)
        .bind(date_to)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let mut summaries = Vec::new();
        for row in rows {
            let period_start: DateTime<Utc> =
                row.try_get("period_start").map_err(Error::Database)?;
            let total_revenue: Decimal = row.try_get("total_revenue").map_err(Error::Database)?;
            let total_orders: i64 = row.try_get("total_orders").map_err(Error::Database)?;
            let total_items_sold: i64 = row.try_get("total_items_sold").unwrap_or(0);

            let average_order_value = if total_orders > 0 {
                total_revenue / Decimal::from(total_orders)
            } else {
                Decimal::ZERO
            };

            // Calculate period end based on period type
            let period_end = match period {
                Period::Day => period_start + Duration::days(1),
                Period::Week => period_start + Duration::weeks(1),
                Period::Month => {
                    // Add one month
                    let naive = period_start.naive_utc();
                    let new_month = if naive.month() == 12 {
                        naive
                            .with_month(1)
                            .unwrap()
                            .with_year(naive.year() + 1)
                            .unwrap()
                    } else {
                        naive.with_month(naive.month() + 1).unwrap()
                    };
                    DateTime::from_naive_utc_and_offset(new_month, Utc)
                }
                Period::Year => {
                    let naive = period_start.naive_utc();
                    DateTime::from_naive_utc_and_offset(
                        naive.with_year(naive.year() + 1).unwrap(),
                        Utc,
                    )
                }
            };

            summaries.push(SalesSummary {
                period_start,
                period_end,
                total_revenue,
                total_orders,
                total_items_sold,
                average_order_value,
            });
        }

        Ok(summaries)
    }

    async fn get_order_statistics(
        &self,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
    ) -> Result<OrderStatistics> {
        // Get total orders and revenue
        let totals = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as total_orders,
                COALESCE(SUM(total), 0) as total_revenue
            FROM orders
            WHERE created_at >= $1 AND created_at <= $2
            "#,
        )
        .bind(date_from)
        .bind(date_to)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        let total_orders: i64 = totals.try_get("total_orders").map_err(Error::Database)?;
        let total_revenue: Decimal = totals.try_get("total_revenue").map_err(Error::Database)?;

        let average_order_value = if total_orders > 0 {
            total_revenue / Decimal::from(total_orders)
        } else {
            Decimal::ZERO
        };

        // Get status breakdown
        let status_rows = sqlx::query(
            r#"
            SELECT 
                status::text as status,
                COUNT(*) as count,
                COALESCE(SUM(total), 0) as revenue
            FROM orders
            WHERE created_at >= $1 AND created_at <= $2
            GROUP BY status
            ORDER BY count DESC
            "#,
        )
        .bind(date_from)
        .bind(date_to)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let status_breakdown: Vec<StatusCount> = status_rows
            .into_iter()
            .map(|row| StatusCount {
                status: row.try_get("status").unwrap_or_default(),
                count: row.try_get("count").unwrap_or(0),
                revenue: row.try_get("revenue").unwrap_or(Decimal::ZERO),
            })
            .collect();

        // Get payment status breakdown
        let payment_rows = sqlx::query(
            r#"
            SELECT 
                payment_status::text as status,
                COUNT(*) as count,
                COALESCE(SUM(total), 0) as revenue
            FROM orders
            WHERE created_at >= $1 AND created_at <= $2
            GROUP BY payment_status
            ORDER BY count DESC
            "#,
        )
        .bind(date_from)
        .bind(date_to)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let payment_status_breakdown: Vec<StatusCount> = payment_rows
            .into_iter()
            .map(|row| StatusCount {
                status: row.try_get("status").unwrap_or_default(),
                count: row.try_get("count").unwrap_or(0),
                revenue: row.try_get("revenue").unwrap_or(Decimal::ZERO),
            })
            .collect();

        // Get fulfillment status breakdown
        let fulfillment_rows = sqlx::query(
            r#"
            SELECT 
                fulfillment_status::text as status,
                COUNT(*) as count,
                COALESCE(SUM(total), 0) as revenue
            FROM orders
            WHERE created_at >= $1 AND created_at <= $2
            GROUP BY fulfillment_status
            ORDER BY count DESC
            "#,
        )
        .bind(date_from)
        .bind(date_to)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let fulfillment_status_breakdown: Vec<StatusCount> = fulfillment_rows
            .into_iter()
            .map(|row| StatusCount {
                status: row.try_get("status").unwrap_or_default(),
                count: row.try_get("count").unwrap_or(0),
                revenue: row.try_get("revenue").unwrap_or(Decimal::ZERO),
            })
            .collect();

        Ok(OrderStatistics {
            total_orders,
            total_revenue,
            average_order_value,
            status_breakdown,
            payment_status_breakdown,
            fulfillment_status_breakdown,
        })
    }

    async fn get_product_performance(
        &self,
        limit: i64,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<Vec<ProductPerformance>> {
        let mut query = String::from(
            r#"
            SELECT 
                p.id as product_id,
                p.title as product_name,
                p.sku,
                COALESCE(SUM(oi.quantity), 0) as units_sold,
                COALESCE(SUM(oi.total), 0) as revenue,
                COUNT(DISTINCT oi.order_id) as orders_count
            FROM products p
            LEFT JOIN order_items oi ON p.id = oi.product_id
            LEFT JOIN orders o ON oi.order_id = o.id
            WHERE 1=1
            "#,
        );

        // Add date filtering if provided
        if date_from.is_some() {
            query.push_str(" AND o.created_at >= $1");
        }
        if date_to.is_some() {
            query.push_str(&format!(
                " AND o.created_at <= ${}",
                if date_from.is_some() { 2 } else { 1 }
            ));
        }

        let limit_param = match (date_from, date_to) {
            (Some(_), Some(_)) => 3,
            (Some(_), None) | (None, Some(_)) => 2,
            (None, None) => 1,
        };

        query.push_str(&format!(
            r#"
            GROUP BY p.id, p.title, p.sku
            HAVING COALESCE(SUM(oi.quantity), 0) > 0
            ORDER BY revenue DESC
            LIMIT ${}
            "#,
            limit_param
        ));

        let mut query_builder = sqlx::query_as::<_, ProductPerformanceRow>(&query);

        if let Some(from) = date_from {
            query_builder = query_builder.bind(from);
        }
        if let Some(to) = date_to {
            query_builder = query_builder.bind(to);
        }
        query_builder = query_builder.bind(limit);

        let rows: Vec<ProductPerformanceRow> = query_builder
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_customer_statistics(&self) -> Result<CustomerStatistics> {
        // Total customers
        let total_customers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        // New customers in last 30 days
        let thirty_days_ago = Utc::now() - Duration::days(30);
        let new_customers_period: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE created_at >= $1")
                .bind(thirty_days_ago)
                .fetch_one(&self.pool)
                .await
                .map_err(Error::Database)?;

        // Customers with orders (returning)
        let customers_with_orders: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT customer_id) FROM orders WHERE customer_id IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        let returning_customers = customers_with_orders.saturating_sub(new_customers_period);

        // Average orders per customer
        let avg_orders: f64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(AVG(order_count), 0.0)
            FROM (
                SELECT customer_id, COUNT(*) as order_count
                FROM orders
                WHERE customer_id IS NOT NULL
                GROUP BY customer_id
            ) subq
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Average customer lifetime value
        let avg_clv: Decimal = sqlx::query_scalar(
            r#"
            SELECT COALESCE(AVG(customer_total), 0)
            FROM (
                SELECT customer_id, SUM(total) as customer_total
                FROM orders
                WHERE customer_id IS NOT NULL
                AND status NOT IN ('cancelled', 'refunded')
                GROUP BY customer_id
            ) subq
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(CustomerStatistics {
            total_customers,
            new_customers_period,
            returning_customers,
            customers_with_orders,
            average_orders_per_customer: avg_orders,
            average_customer_lifetime_value: avg_clv,
        })
    }

    async fn get_dashboard_metrics(&self) -> Result<DashboardMetrics> {
        // Total metrics
        let totals = sqlx::query(
            r#"
            SELECT 
                COALESCE(SUM(total), 0) as total_revenue,
                COUNT(*) as total_orders
            FROM orders
            WHERE status NOT IN ('cancelled', 'refunded')
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        let total_revenue: Decimal = totals.try_get("total_revenue").map_err(Error::Database)?;
        let total_orders: i64 = totals.try_get("total_orders").map_err(Error::Database)?;

        // Total customers
        let total_customers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        // Total products
        let total_products: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE is_active = true")
                .fetch_one(&self.pool)
                .await
                .map_err(Error::Database)?;

        // Pending orders
        let pending_orders: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orders WHERE status IN ('pending', 'processing')",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Today's metrics
        let today_start: DateTime<Utc> = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();

        let today_metrics = sqlx::query(
            r#"
            SELECT 
                COALESCE(SUM(total), 0) as revenue_today,
                COUNT(*) as orders_today
            FROM orders
            WHERE created_at >= $1
            AND status NOT IN ('cancelled', 'refunded')
            "#,
        )
        .bind(today_start)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        let revenue_today: Decimal = today_metrics
            .try_get("revenue_today")
            .map_err(Error::Database)?;
        let orders_today: i64 = today_metrics
            .try_get("orders_today")
            .map_err(Error::Database)?;

        // This month's metrics
        let month_start: DateTime<Utc> = Utc::now()
            .date_naive()
            .with_day(1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();

        let month_metrics = sqlx::query(
            r#"
            SELECT 
                COALESCE(SUM(total), 0) as revenue_this_month,
                COUNT(*) as orders_this_month
            FROM orders
            WHERE created_at >= $1
            AND status NOT IN ('cancelled', 'refunded')
            "#,
        )
        .bind(month_start)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        let revenue_this_month: Decimal = month_metrics
            .try_get("revenue_this_month")
            .map_err(Error::Database)?;
        let orders_this_month: i64 = month_metrics
            .try_get("orders_this_month")
            .map_err(Error::Database)?;

        Ok(DashboardMetrics {
            total_revenue,
            total_orders,
            total_customers,
            total_products,
            pending_orders,
            revenue_today,
            orders_today,
            revenue_this_month,
            orders_this_month,
        })
    }

    async fn get_revenue_by_period(
        &self,
        period: Period,
        periods_count: i32,
    ) -> Result<Vec<RevenueDataPoint>> {
        let trunc = self.date_trunc(period);

        let rows = sqlx::query(&format!(
            r#"
            SELECT 
                DATE_TRUNC('{}', created_at) as period,
                COALESCE(SUM(total), 0) as revenue,
                COUNT(*) as orders
            FROM orders
            WHERE created_at >= DATE_TRUNC('{}', NOW() - INTERVAL '{} {}')
            AND status NOT IN ('cancelled', 'refunded')
            GROUP BY DATE_TRUNC('{}', created_at)
            ORDER BY period
            "#,
            trunc, trunc, periods_count, trunc, trunc
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let mut data_points = Vec::new();
        for row in rows {
            let date: DateTime<Utc> = row.try_get("period").map_err(Error::Database)?;
            let revenue: Decimal = row.try_get("revenue").map_err(Error::Database)?;
            let orders: i64 = row.try_get("orders").map_err(Error::Database)?;

            let period_label = match period {
                Period::Day => date.format("%Y-%m-%d").to_string(),
                Period::Week => format!("Week {}", date.iso_week().week()),
                Period::Month => date.format("%Y-%m").to_string(),
                Period::Year => date.format("%Y").to_string(),
            };

            data_points.push(RevenueDataPoint {
                period: period_label,
                revenue,
                orders,
                date,
            });
        }

        Ok(data_points)
    }

    async fn compare_periods(
        &self,
        current_from: DateTime<Utc>,
        current_to: DateTime<Utc>,
    ) -> Result<PeriodComparison> {
        let duration = current_to - current_from;
        let previous_from = current_from - duration;
        let previous_to = current_from;

        // Current period metrics
        let current = sqlx::query(
            r#"
            SELECT 
                COALESCE(SUM(total), 0) as revenue,
                COUNT(*) as orders,
                COUNT(DISTINCT customer_id) as customers,
                COALESCE(AVG(total), 0) as aov
            FROM orders
            WHERE created_at >= $1 AND created_at <= $2
            AND status NOT IN ('cancelled', 'refunded')
            "#,
        )
        .bind(current_from)
        .bind(current_to)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        let current_revenue: Decimal = current.try_get("revenue").map_err(Error::Database)?;
        let current_orders: i64 = current.try_get("orders").map_err(Error::Database)?;
        let current_customers: i64 = current.try_get("customers").map_err(Error::Database)?;
        let current_aov: Decimal = current.try_get("aov").map_err(Error::Database)?;

        // Previous period metrics
        let previous = sqlx::query(
            r#"
            SELECT 
                COALESCE(SUM(total), 0) as revenue,
                COUNT(*) as orders,
                COUNT(DISTINCT customer_id) as customers,
                COALESCE(AVG(total), 0) as aov
            FROM orders
            WHERE created_at >= $1 AND created_at <= $2
            AND status NOT IN ('cancelled', 'refunded')
            "#,
        )
        .bind(previous_from)
        .bind(previous_to)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        let previous_revenue: Decimal = previous.try_get("revenue").map_err(Error::Database)?;
        let previous_orders: i64 = previous.try_get("orders").map_err(Error::Database)?;
        let previous_customers: i64 = previous.try_get("customers").map_err(Error::Database)?;
        let previous_aov: Decimal = previous.try_get("aov").map_err(Error::Database)?;

        Ok(PeriodComparison {
            revenue: calculate_trend(current_revenue, previous_revenue),
            orders: calculate_trend(
                Decimal::from(current_orders),
                Decimal::from(previous_orders),
            ),
            customers: calculate_trend(
                Decimal::from(current_customers),
                Decimal::from(previous_customers),
            ),
            average_order_value: calculate_trend(current_aov, previous_aov),
        })
    }
}

/// Helper struct for product performance query
#[derive(sqlx::FromRow)]
struct ProductPerformanceRow {
    product_id: Uuid,
    product_name: String,
    sku: Option<String>,
    units_sold: Option<i64>,
    revenue: Option<Decimal>,
    orders_count: Option<i64>,
}

impl From<ProductPerformanceRow> for ProductPerformance {
    fn from(row: ProductPerformanceRow) -> Self {
        Self {
            product_id: row.product_id,
            product_name: row.product_name,
            sku: row.sku,
            units_sold: row.units_sold.unwrap_or(0),
            revenue: row.revenue.unwrap_or(Decimal::ZERO),
            orders_count: row.orders_count.unwrap_or(0),
        }
    }
}

/// Calculate trend comparison between two values
fn calculate_trend(current: Decimal, previous: Decimal) -> TrendComparison {
    let change_amount = current - previous;
    let change_percentage = if previous > Decimal::ZERO {
        ((change_amount / previous) * Decimal::from(100))
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0)
    } else if current > Decimal::ZERO {
        100.0
    } else {
        0.0
    };

    let trend_direction = if change_amount > Decimal::ZERO {
        TrendDirection::Up
    } else if change_amount < Decimal::ZERO {
        TrendDirection::Down
    } else {
        TrendDirection::Flat
    };

    TrendComparison {
        current_value: current,
        previous_value: previous,
        change_amount,
        change_percentage,
        trend_direction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_period_from_str() {
        assert!(matches!("day".parse::<Period>(), Ok(Period::Day)));
        assert!(matches!("week".parse::<Period>(), Ok(Period::Week)));
        assert!(matches!("month".parse::<Period>(), Ok(Period::Month)));
        assert!(matches!("year".parse::<Period>(), Ok(Period::Year)));
        assert!("invalid".parse::<Period>().is_err());
    }

    #[test]
    fn test_calculate_trend() {
        let trend = calculate_trend(Decimal::from(150), Decimal::from(100));
        assert_eq!(trend.change_amount, Decimal::from(50));
        assert!(trend.change_percentage > 49.0 && trend.change_percentage < 51.0);
        assert_eq!(trend.trend_direction, TrendDirection::Up);

        let trend = calculate_trend(Decimal::from(100), Decimal::from(150));
        assert_eq!(trend.change_amount, Decimal::from(-50));
        assert_eq!(trend.trend_direction, TrendDirection::Down);

        let trend = calculate_trend(Decimal::from(100), Decimal::from(100));
        assert_eq!(trend.change_amount, Decimal::ZERO);
        assert_eq!(trend.trend_direction, TrendDirection::Flat);
    }
}
