use rust_decimal::Decimal;
use uuid::Uuid;

use crate::inventory::{
    InventoryConfig, InventoryLevel, InventoryLocation, LocationInventory, ProductInventory,
    StockMovement, StockReservation,
};
use crate::repository::Database;
use crate::{Error, Result};

pub struct InventoryService {
    db: Database,
    config: InventoryConfig,
}

impl Clone for InventoryService {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            config: self.config.clone(),
        }
    }
}

impl InventoryService {
    pub fn new(db: Database, config: InventoryConfig) -> Self {
        Self { db, config }
    }

    /// Get inventory levels for a product
    pub async fn get_product_inventory(&self, product_id: Uuid) -> Result<ProductInventory> {
        // Get inventory from all locations
        let inventory_levels = sqlx::query_as::<_, InventoryLevel>(
            "SELECT * FROM inventory_levels WHERE product_id = $1",
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        // Get stock reservations
        let reservations = sqlx::query_as::<_, StockReservation>(
            "SELECT * FROM stock_reservations WHERE product_id = $1 AND status = 'active'",
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        // Calculate totals
        let total_available: i32 = inventory_levels.iter().map(|l| l.available_quantity).sum();
        let total_reserved: i32 = reservations.iter().map(|r| r.quantity).sum();
        let total_incoming: i32 = inventory_levels.iter().map(|l| l.incoming_quantity).sum();

        // Build location inventory
        let mut locations = Vec::new();
        for level in inventory_levels {
            let location_reservations: i32 = reservations
                .iter()
                .filter(|r| r.location_id == level.location_id)
                .map(|r| r.quantity)
                .sum();

            locations.push(LocationInventory {
                location_id: level.location_id,
                location_name: self.get_location_name(level.location_id).await?,
                available: level.available_quantity,
                reserved: location_reservations,
                incoming: level.incoming_quantity,
            });
        }

        Ok(ProductInventory {
            product_id,
            total_available,
            total_reserved,
            total_incoming,
            low_stock_threshold: self.config.low_stock_threshold as i32,
            locations,
        })
    }

    /// Reserve stock for an order
    pub async fn reserve_stock(&self, reservation: StockReservation) -> Result<StockReservation> {
        // Check if enough stock is available
        let available = self
            .get_available_stock(
                reservation.product_id,
                reservation.location_id,
                reservation.variant_id,
            )
            .await?;

        if available < reservation.quantity {
            return Err(Error::validation(format!(
                "Insufficient stock. Available: {}, Requested: {}",
                available, reservation.quantity
            )));
        }

        // Create reservation
        let reservation_id = uuid::Uuid::new_v4();
        let expires_at = chrono::Utc::now()
            + chrono::Duration::minutes(self.config.reservation_timeout_minutes as i64);

        let reservation = sqlx::query_as::<_, StockReservation>(
            r#"
            INSERT INTO stock_reservations 
            (id, product_id, variant_id, location_id, order_id, quantity, expires_at, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'active')
            RETURNING *
            "#,
        )
        .bind(reservation_id)
        .bind(reservation.product_id)
        .bind(reservation.variant_id)
        .bind(reservation.location_id)
        .bind(reservation.order_id)
        .bind(reservation.quantity)
        .bind(expires_at)
        .fetch_one(self.db.pool())
        .await?;

        Ok(reservation)
    }

    /// Release reserved stock
    pub async fn release_reservation(&self, reservation_id: uuid::Uuid) -> Result<bool> {
        let result = sqlx::query("UPDATE stock_reservations SET status = 'released' WHERE id = $1")
            .bind(reservation_id)
            .execute(self.db.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Commit reserved stock (finalize the reservation)
    pub async fn commit_reservation(&self, reservation_id: uuid::Uuid) -> Result<bool> {
        let reservation = self
            .get_reservation(reservation_id)
            .await?
            .ok_or_else(|| Error::not_found("Reservation not found"))?;

        // Update inventory levels
        let result = sqlx::query(
            r#"
            UPDATE inventory_levels 
            SET available_quantity = available_quantity - $1,
                reserved_quantity = reserved_quantity + $1
            WHERE product_id = $2 AND location_id = $3
                AND variant_id IS NOT DISTINCT FROM $4
            "#,
        )
        .bind(reservation.quantity)
        .bind(reservation.product_id)
        .bind(reservation.location_id)
        .bind(reservation.variant_id)
        .execute(self.db.pool())
        .await?;

        // Mark reservation as committed
        sqlx::query("UPDATE stock_reservations SET status = 'committed' WHERE id = $1")
            .bind(reservation_id)
            .execute(self.db.pool())
            .await?;

        if result.rows_affected() == 0 {
            tracing::warn!(
                reservation_id = %reservation_id,
                product_id = %reservation.product_id,
                variant_id = ?reservation.variant_id,
                location_id = %reservation.location_id,
                quantity = reservation.quantity,
                "commit_reservation matched no inventory_levels row; \
                 reservation committed but stock was not decremented \
                 (e.g. NULL-variant reservation on a variants-only product)"
            );
        }

        Ok(result.rows_affected() > 0)
    }

    /// Update inventory levels (receiving new stock)
    pub async fn receive_stock(
        &self,
        product_id: uuid::Uuid,
        location_id: uuid::Uuid,
        quantity: i32,
        cost_per_unit: Option<Decimal>,
    ) -> Result<StockMovement> {
        // Update inventory levels: UPDATE first, INSERT only if no row exists.
        // (An ON CONFLICT target cannot match NULL variant_id rows, so an
        // upsert silently never conflicts for simple products.)
        let updated = sqlx::query(
            r#"
            UPDATE inventory_levels 
            SET available_quantity = available_quantity + $3,
                updated_at = NOW()
            WHERE product_id = $1 AND location_id = $2 AND variant_id IS NULL
            "#,
        )
        .bind(product_id)
        .bind(location_id)
        .bind(quantity)
        .execute(self.db.pool())
        .await?;

        if updated.rows_affected() == 0 {
            sqlx::query(
                r#"
                INSERT INTO inventory_levels (id, product_id, location_id, available_quantity, incoming_quantity)
                VALUES ($1, $2, $3, $4, 0)
                "#,
            )
            .bind(uuid::Uuid::new_v4())
            .bind(product_id)
            .bind(location_id)
            .bind(quantity)
            .execute(self.db.pool())
            .await?;
        }

        // Create stock movement record
        let movement_id = uuid::Uuid::new_v4();
        let movement = sqlx::query_as::<_, StockMovement>(
            r#"
            INSERT INTO stock_movements (id, product_id, location_id, quantity, movement_type, cost_per_unit, reference)
            VALUES ($1, $2, $3, $4, 'in', $5, 'stock_receipt')
            RETURNING *
            "#
        )
        .bind(movement_id)
        .bind(product_id)
        .bind(location_id)
        .bind(quantity)
        .bind(cost_per_unit)
        .fetch_one(self.db.pool())
        .await?;

        Ok(movement)
    }

    /// Check for low stock and trigger alerts
    pub async fn check_low_stock(&self, product_id: uuid::Uuid) -> Result<Option<LowStockAlert>> {
        let inventory = self.get_product_inventory(product_id).await?;

        let threshold = self.config.low_stock_threshold as i32;
        let critical_threshold = (threshold as f32 * 0.5) as i32; // 50% of low stock threshold

        let alert_level = if inventory.total_available <= critical_threshold {
            StockAlertLevel::Critical
        } else if inventory.total_available <= threshold {
            StockAlertLevel::Low
        } else {
            return Ok(None); // No alert needed
        };

        Ok(Some(LowStockAlert {
            product_id,
            current_stock: inventory.total_available,
            threshold,
            alert_level,
            recommended_reorder_quantity: self
                .calculate_reorder_quantity(inventory.total_available, threshold),
            created_at: chrono::Utc::now(),
        }))
    }

    /// Get active reservations that have expired
    pub async fn get_expired_reservations(&self) -> Result<Vec<StockReservation>> {
        let reservations = sqlx::query_as::<_, StockReservation>(
            r#"
            SELECT * FROM stock_reservations 
            WHERE status = 'active' AND expires_at < NOW()
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(self.db.pool())
        .await?;

        Ok(reservations)
    }

    /// Clean up expired reservations
    pub async fn cleanup_expired_reservations(&self) -> Result<i64> {
        let expired = self.get_expired_reservations().await?;
        let mut released_count = 0;

        for reservation in expired {
            if self.release_reservation(reservation.id).await? {
                released_count += 1;
            }
        }

        Ok(released_count)
    }

    /// Helper: Get available stock for a product/variant at a location.
    ///
    /// Sums across matching rows (legacy databases may hold duplicate rows per
    /// key) and filters by variant so a variant purchase is never validated
    /// against another variant's stock.
    async fn get_available_stock(
        &self,
        product_id: uuid::Uuid,
        location_id: uuid::Uuid,
        variant_id: Option<uuid::Uuid>,
    ) -> Result<i32> {
        let available: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels \
             WHERE product_id = $1 AND location_id = $2 \
             AND variant_id IS NOT DISTINCT FROM $3",
        )
        .bind(product_id)
        .bind(location_id)
        .bind(variant_id)
        .fetch_one(self.db.pool())
        .await?;

        Ok(i32::try_from(available).unwrap_or(i32::MAX))
    }

    /// Get available stock for a product (optionally a specific variant) across all locations.
    pub async fn get_available_stock_for_product(
        &self,
        product_id: uuid::Uuid,
        variant_id: Option<uuid::Uuid>,
    ) -> Result<i32> {
        let available: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels \
             WHERE product_id = $1 AND variant_id IS NOT DISTINCT FROM $2",
        )
        .bind(product_id)
        .bind(variant_id)
        .fetch_one(self.db.pool())
        .await?;

        Ok(i32::try_from(available).unwrap_or(i32::MAX))
    }

    /// Helper: Get location name
    async fn get_location_name(&self, location_id: uuid::Uuid) -> Result<String> {
        let location = sqlx::query_as::<_, InventoryLocation>(
            "SELECT * FROM inventory_locations WHERE id = $1",
        )
        .bind(location_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(location
            .map(|l| l.name)
            .unwrap_or_else(|| "Unknown".to_string()))
    }

    /// Helper: Calculate reorder quantity
    fn calculate_reorder_quantity(&self, current_stock: i32, threshold: i32) -> i32 {
        let ideal_stock = threshold * 5; // Reorder to 5x threshold
        (ideal_stock - current_stock).max(0)
    }

    /// Helper: Get reservation by ID
    async fn get_reservation(
        &self,
        reservation_id: uuid::Uuid,
    ) -> Result<Option<StockReservation>> {
        let reservation =
            sqlx::query_as::<_, StockReservation>("SELECT * FROM stock_reservations WHERE id = $1")
                .bind(reservation_id)
                .fetch_optional(self.db.pool())
                .await?;

        Ok(reservation)
    }
}

#[async_trait::async_trait]
impl crate::services::Service for InventoryService {
    async fn health_check(&self) -> Result<()> {
        // Check database connectivity
        let _ = sqlx::query("SELECT 1").fetch_one(self.db.pool()).await?;
        Ok(())
    }
}

/// Low stock alert
#[derive(Debug, Clone)]
pub struct LowStockAlert {
    pub product_id: uuid::Uuid,
    pub current_stock: i32,
    pub threshold: i32,
    pub alert_level: StockAlertLevel,
    pub recommended_reorder_quantity: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum StockAlertLevel {
    Low,      // Below threshold but above critical
    Critical, // Below 50% of threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inventory_config_default() {
        let config = InventoryConfig::default();
        assert_eq!(config.low_stock_threshold, 20);
        assert!(config.enable_restock_alerts);
        assert!(config.enable_reservations);
        assert_eq!(config.reservation_timeout_minutes, 30);
    }
}
