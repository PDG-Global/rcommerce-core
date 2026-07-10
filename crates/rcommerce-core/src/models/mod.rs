use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

pub mod address;
pub mod cart;
pub mod coupon;
pub mod customer;
pub mod order;
pub mod product;
pub mod subscription;

// Re-export common models
pub use address::*;
pub use cart::*;
pub use coupon::*;
pub use customer::*;
pub use order::*;
pub use product::*;
pub use subscription::*;

/// Common trait for all entities
pub trait Entity: Send + Sync {
    fn id(&self) -> Uuid;
    fn created_at(&self) -> DateTime<Utc>;
    fn updated_at(&self) -> DateTime<Utc>;
}

/// Pagination parameters
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Pagination {
    #[validate(range(min = 1, max = 100))]
    pub page: i64,

    #[validate(range(min = 1, max = 500))]
    pub per_page: i64,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

impl Pagination {
    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.per_page
    }
}

/// Sort direction
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Sort parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortParams {
    pub field: String,
    pub direction: SortDirection,
}

/// Currency representation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type, Default)]
#[sqlx(type_name = "currency", rename_all = "UPPERCASE")]
pub enum Currency {
    #[default]
    USD,
    EUR,
    GBP,
    JPY,
    AUD,
    CAD,
    CNY,
    HKD,
    SGD,
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Currency::USD => write!(f, "USD"),
            Currency::EUR => write!(f, "EUR"),
            Currency::GBP => write!(f, "GBP"),
            Currency::JPY => write!(f, "JPY"),
            Currency::AUD => write!(f, "AUD"),
            Currency::CAD => write!(f, "CAD"),
            Currency::CNY => write!(f, "CNY"),
            Currency::HKD => write!(f, "HKD"),
            Currency::SGD => write!(f, "SGD"),
        }
    }
}

impl std::str::FromStr for Currency {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "USD" => Ok(Currency::USD),
            "EUR" => Ok(Currency::EUR),
            "GBP" => Ok(Currency::GBP),
            "JPY" => Ok(Currency::JPY),
            "AUD" => Ok(Currency::AUD),
            "CAD" => Ok(Currency::CAD),
            "CNY" => Ok(Currency::CNY),
            "HKD" => Ok(Currency::HKD),
            "SGD" => Ok(Currency::SGD),
            _ => Err(format!("Unknown currency: {}", s)),
        }
    }
}

/// Weight units
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "weight_unit", rename_all = "snake_case")]
pub enum WeightUnit {
    G,
    Kg,
    Oz,
    Lb,
}

/// Length units
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "length_unit", rename_all = "snake_case")]
pub enum LengthUnit {
    Cm,
    M,
    In,
    Ft,
}

/// Inventory policy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "inventory_policy", rename_all = "snake_case")]
pub enum InventoryPolicy {
    #[default]
    Deny,
    Continue,
}

/// Product image representation
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProductImage {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub position: i32,
    pub src: String,
    pub alt_text: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Product category
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProductCategory {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Product tag
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProductTag {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
