//! Repository module for database access
//!
//! PostgreSQL is the supported database.

pub mod postgres;
pub mod traits;

// Cart, Coupon, API Key, Statistics, Order, Inventory, Fulfillment, Notification, and Category repositories
pub mod api_key_repository;
pub mod cart_repository;
pub mod category_repository;
pub mod coupon_repository;
pub mod fulfillment_repository;
pub mod inventory_repository;
pub mod notification_repository;
pub mod order_repository;
pub mod statistics_repository;
pub mod subscription_repository;
pub mod tag_repository;

// Re-export cart, coupon, api_key, subscription, statistics, order, inventory, and fulfillment traits
pub use api_key_repository::{
    ApiKeyRecord, ApiKeyRepository, CreateApiKeyRequest, PostgresApiKeyRepository,
};
pub use cart_repository::{CartRepository, PgCartRepository};
pub use category_repository::{CategoryRepository, CategoryTreeNode, PostgresCategoryRepository};
pub use coupon_repository::{CouponRepository, PgCouponRepository};
pub use fulfillment_repository::{FulfillmentRepository, PostgresFulfillmentRepository};
pub use inventory_repository::{InventoryRepository, PostgresInventoryRepository};
pub use notification_repository::{NotificationRepository, PostgresNotificationRepository};
pub use order_repository::{OrderFilter, OrderRepository, PostgresOrderRepository};
pub use statistics_repository::{
    CustomerStatistics, DashboardMetrics, OrderStatistics, Period, PeriodComparison,
    PgStatisticsRepository, ProductPerformance, RevenueDataPoint, SalesSummary,
    StatisticsRepository, StatusCount, TrendComparison, TrendDirection,
};
pub use subscription_repository::{PostgresSubscriptionRepository, SubscriptionRepository};
pub use tag_repository::{PostgresTagRepository, TagRepository};

// PostgreSQL exports
pub use postgres::{
    create_pool, PostgresCustomerRepository as CustomerRepository, PostgresDb as Database,
    PostgresNewsletterRepository, PostgresProductRepository as ProductRepository,
    PostgresProductReviewRepository,
};

// Re-export traits
pub use traits::{CustomerRepositoryTrait, ProductRepositoryTrait};
