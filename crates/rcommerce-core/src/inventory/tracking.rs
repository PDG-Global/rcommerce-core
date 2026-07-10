use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

/// Current inventory level for a product at a location
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InventoryLevel {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub location_id: Uuid,
    pub available_quantity: i32,
    pub reserved_quantity: i32,
    pub incoming_quantity: i32,
    pub reorder_point: i32,
    pub reorder_quantity: i32,
    pub cost_per_unit: Option<Decimal>,
    pub last_counted_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl InventoryLevel {
    pub fn is_low_stock(&self) -> bool {
        self.available_quantity <= self.reorder_point
    }

    pub fn stock_status(&self) -> StockStatus {
        if self.available_quantity <= 0 {
            StockStatus::OutOfStock
        } else if self.available_quantity <= self.reorder_point {
            StockStatus::LowStock
        } else {
            StockStatus::InStock
        }
    }

    pub fn total_quantity(&self) -> i32 {
        self.available_quantity + self.reserved_quantity + self.incoming_quantity
    }
}

/// Stock movement record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StockMovement {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub location_id: Uuid,
    pub quantity: i32,
    pub movement_type: StockMovementType,
    pub cost_per_unit: Option<Decimal>,
    pub reference: String,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl StockMovement {
    pub fn new_incoming(
        product_id: Uuid,
        variant_id: Option<Uuid>,
        location_id: Uuid,
        quantity: i32,
        cost_per_unit: Option<Decimal>,
        reference: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            product_id,
            variant_id,
            location_id,
            quantity,
            movement_type: StockMovementType::In,
            cost_per_unit,
            reference,
            notes: None,
            created_at: Utc::now(),
        }
    }

    pub fn new_outgoing(
        product_id: Uuid,
        variant_id: Option<Uuid>,
        location_id: Uuid,
        quantity: i32,
        reference: String,
        notes: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            product_id,
            variant_id,
            location_id,
            quantity: -quantity, // Negative for outgoing
            movement_type: StockMovementType::Out,
            cost_per_unit: None,
            reference,
            notes: Some(notes),
            created_at: Utc::now(),
        }
    }
}

/// Stock status enum
#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(type_name = "stock_status", rename_all = "snake_case")]
pub enum StockStatus {
    InStock,
    LowStock,
    OutOfStock,
    OnOrder,
}

/// Stock movement type
#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(type_name = "stock_movement_type", rename_all = "snake_case")]
pub enum StockMovementType {
    In,       // Stock received
    Out,      // Stock sold/adjusted
    Return,   // Stock returned
    Lost,     // Stock lost/damaged
    Found,    // Stock found
    Transfer, // Stock transferred
}

/// Stock adjustment (manual correction)
#[derive(Debug, Clone)]
pub struct StockAdjustment {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub location_id: Uuid,
    pub quantity_before: i32,
    pub quantity_after: i32,
    pub reason: String,
    pub notes: Option<String>,
    pub adjusted_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Inventory valuation
#[derive(Debug, Clone)]
pub struct InventoryValuation {
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub quantity_on_hand: i32,
    pub average_cost: Decimal,
    pub total_value: Decimal,
    pub calculated_at: DateTime<Utc>,
}

impl InventoryValuation {
    pub fn calculate(level: &InventoryLevel) -> Self {
        let total_value =
            level.cost_per_unit.unwrap_or(dec!(0)) * Decimal::from(level.total_quantity());

        Self {
            product_id: level.product_id,
            location_id: level.location_id,
            quantity_on_hand: level.total_quantity(),
            average_cost: level.cost_per_unit.unwrap_or(dec!(0)),
            total_value,
            calculated_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inventory_level_stock_status() {
        let level = InventoryLevel {
            id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
            variant_id: None,
            location_id: Uuid::new_v4(),
            available_quantity: 5,
            reserved_quantity: 0,
            incoming_quantity: 0,
            reorder_point: 10,
            reorder_quantity: 20,
            cost_per_unit: Some(dec!(10.00)),
            last_counted_at: None,
            updated_at: Utc::now(),
        };

        assert!(level.is_low_stock());
        assert_eq!(level.stock_status(), StockStatus::LowStock);
    }
}
