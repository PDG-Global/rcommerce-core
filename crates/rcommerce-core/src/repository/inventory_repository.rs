//! Inventory Repository
//!
//! Database repository for inventory-related operations following the repository pattern.

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    inventory::{
        reservation::ReservationStatus, tracking::StockMovementType, InventoryLevel,
        InventoryLocation, StockMovement, StockReservation,
    },
    Error, Result,
};

/// Inventory repository trait - database agnostic
#[async_trait]
pub trait InventoryRepository: Send + Sync + 'static {
    /// Get inventory level for a product
    async fn get_inventory_level(
        &self,
        product_id: Uuid,
        location_id: Option<Uuid>,
    ) -> Result<Option<InventoryLevel>>;

    /// Update inventory level
    async fn update_inventory_level(&self, level: &InventoryLevel) -> Result<InventoryLevel>;

    /// Create inventory level
    async fn create_inventory_level(&self, level: &InventoryLevel) -> Result<InventoryLevel>;

    /// List inventory levels for a location
    async fn list_inventory_levels(&self, location_id: Uuid) -> Result<Vec<InventoryLevel>>;

    /// Get low stock items
    async fn get_low_stock_items(&self, threshold: i32) -> Result<Vec<InventoryLevel>>;

    /// Create stock reservation
    async fn create_reservation(&self, reservation: &StockReservation) -> Result<StockReservation>;

    /// Find reservation by ID
    async fn find_reservation_by_id(&self, id: Uuid) -> Result<Option<StockReservation>>;

    /// Find active reservations for order
    async fn find_reservations_by_order(&self, order_id: Uuid) -> Result<Vec<StockReservation>>;

    /// Update reservation status
    async fn update_reservation_status(
        &self,
        id: Uuid,
        status: ReservationStatus,
    ) -> Result<StockReservation>;

    /// Commit reservation (convert to actual stock movement)
    async fn commit_reservation(&self, id: Uuid) -> Result<StockReservation>;

    /// Release reservation
    async fn release_reservation(&self, id: Uuid) -> Result<StockReservation>;

    /// Record stock movement
    async fn record_movement(&self, movement: &StockMovement) -> Result<StockMovement>;

    /// Get stock movements for a product
    async fn get_stock_movements(&self, product_id: Uuid, limit: i64)
        -> Result<Vec<StockMovement>>;

    /// Get inventory location by ID
    async fn get_location(&self, id: Uuid) -> Result<Option<InventoryLocation>>;

    /// List all inventory locations
    async fn list_locations(&self) -> Result<Vec<InventoryLocation>>;

    /// Create inventory location
    async fn create_location(&self, location: &InventoryLocation) -> Result<InventoryLocation>;

    /// Update inventory location
    async fn update_location(&self, location: &InventoryLocation) -> Result<InventoryLocation>;

    /// Delete inventory location
    async fn delete_location(&self, id: Uuid) -> Result<bool>;

    /// Check if product is in stock
    async fn is_in_stock(&self, product_id: Uuid, quantity: i32) -> Result<bool>;

    /// Adjust stock quantity
    async fn adjust_stock(
        &self,
        product_id: Uuid,
        location_id: Uuid,
        adjustment: i32,
        reason: &str,
    ) -> Result<InventoryLevel>;
}

/// PostgreSQL implementation of InventoryRepository
pub struct PostgresInventoryRepository {
    db: sqlx::PgPool,
}

impl PostgresInventoryRepository {
    /// Create a new PostgreSQL inventory repository
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl InventoryRepository for PostgresInventoryRepository {
    async fn get_inventory_level(
        &self,
        product_id: Uuid,
        location_id: Option<Uuid>,
    ) -> Result<Option<InventoryLevel>> {
        let level = if let Some(loc_id) = location_id {
            sqlx::query_as::<_, InventoryLevel>(
                "SELECT * FROM inventory_levels WHERE product_id = $1 AND location_id = $2",
            )
            .bind(product_id)
            .bind(loc_id)
            .fetch_optional(&self.db)
            .await
        } else {
            sqlx::query_as::<_, InventoryLevel>(
                "SELECT * FROM inventory_levels WHERE product_id = $1 LIMIT 1",
            )
            .bind(product_id)
            .fetch_optional(&self.db)
            .await
        }
        .map_err(|e| Error::Other(format!("Failed to fetch inventory level: {}", e)))?;

        Ok(level)
    }

    async fn update_inventory_level(&self, level: &InventoryLevel) -> Result<InventoryLevel> {
        let level = sqlx::query_as::<_, InventoryLevel>(
            r#"
            UPDATE inventory_levels 
            SET available_quantity = $1,
                reserved_quantity = $2,
                incoming_quantity = $3,
                reorder_point = $4,
                reorder_quantity = $5,
                cost_per_unit = $6,
                updated_at = NOW()
            WHERE id = $7
            RETURNING *
            "#,
        )
        .bind(level.available_quantity)
        .bind(level.reserved_quantity)
        .bind(level.incoming_quantity)
        .bind(level.reorder_point)
        .bind(level.reorder_quantity)
        .bind(level.cost_per_unit)
        .bind(level.id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update inventory level: {}", e)))?;

        Ok(level)
    }

    async fn create_inventory_level(&self, level: &InventoryLevel) -> Result<InventoryLevel> {
        let level = sqlx::query_as::<_, InventoryLevel>(
            r#"
            INSERT INTO inventory_levels (
                id, product_id, variant_id, location_id, 
                available_quantity, reserved_quantity, incoming_quantity,
                reorder_point, reorder_quantity, cost_per_unit
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(level.id)
        .bind(level.product_id)
        .bind(level.variant_id)
        .bind(level.location_id)
        .bind(level.available_quantity)
        .bind(level.reserved_quantity)
        .bind(level.incoming_quantity)
        .bind(level.reorder_point)
        .bind(level.reorder_quantity)
        .bind(level.cost_per_unit)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create inventory level: {}", e)))?;

        Ok(level)
    }

    async fn list_inventory_levels(&self, location_id: Uuid) -> Result<Vec<InventoryLevel>> {
        let levels = sqlx::query_as::<_, InventoryLevel>(
            "SELECT * FROM inventory_levels WHERE location_id = $1",
        )
        .bind(location_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch inventory levels: {}", e)))?;

        Ok(levels)
    }

    async fn get_low_stock_items(&self, threshold: i32) -> Result<Vec<InventoryLevel>> {
        let levels = sqlx::query_as::<_, InventoryLevel>(
            "SELECT * FROM inventory_levels WHERE available_quantity <= $1",
        )
        .bind(threshold)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch low stock items: {}", e)))?;

        Ok(levels)
    }

    async fn create_reservation(&self, reservation: &StockReservation) -> Result<StockReservation> {
        let reservation = sqlx::query_as::<_, StockReservation>(
            r#"
            INSERT INTO stock_reservations (
                id, product_id, variant_id, location_id, order_id,
                quantity, status, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(reservation.id)
        .bind(reservation.product_id)
        .bind(reservation.variant_id)
        .bind(reservation.location_id)
        .bind(reservation.order_id)
        .bind(reservation.quantity)
        .bind(format!("{:?}", reservation.status).to_lowercase())
        .bind(reservation.expires_at)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create reservation: {}", e)))?;

        Ok(reservation)
    }

    async fn find_reservation_by_id(&self, id: Uuid) -> Result<Option<StockReservation>> {
        let reservation =
            sqlx::query_as::<_, StockReservation>("SELECT * FROM stock_reservations WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| Error::Other(format!("Failed to fetch reservation: {}", e)))?;

        Ok(reservation)
    }

    async fn find_reservations_by_order(&self, order_id: Uuid) -> Result<Vec<StockReservation>> {
        let reservations = sqlx::query_as::<_, StockReservation>(
            "SELECT * FROM stock_reservations WHERE order_id = $1 AND status = 'active'",
        )
        .bind(order_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch reservations: {}", e)))?;

        Ok(reservations)
    }

    async fn update_reservation_status(
        &self,
        id: Uuid,
        status: ReservationStatus,
    ) -> Result<StockReservation> {
        let reservation = sqlx::query_as::<_, StockReservation>(
            "UPDATE stock_reservations SET status = $1, updated_at = NOW() WHERE id = $2 RETURNING *"
        )
        .bind(format!("{:?}", status).to_lowercase())
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update reservation status: {}", e)))?;

        Ok(reservation)
    }

    async fn commit_reservation(&self, id: Uuid) -> Result<StockReservation> {
        let reservation = sqlx::query_as::<_, StockReservation>(
            "UPDATE stock_reservations SET status = 'committed', updated_at = NOW() WHERE id = $1 RETURNING *"
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to commit reservation: {}", e)))?;

        Ok(reservation)
    }

    async fn release_reservation(&self, id: Uuid) -> Result<StockReservation> {
        let reservation = sqlx::query_as::<_, StockReservation>(
            "UPDATE stock_reservations SET status = 'released', updated_at = NOW() WHERE id = $1 RETURNING *"
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to release reservation: {}", e)))?;

        Ok(reservation)
    }

    async fn record_movement(&self, movement: &StockMovement) -> Result<StockMovement> {
        let movement = sqlx::query_as::<_, StockMovement>(
            r#"
            INSERT INTO stock_movements (
                id, product_id, variant_id, location_id, quantity, movement_type, 
                cost_per_unit, reference, notes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(movement.id)
        .bind(movement.product_id)
        .bind(movement.variant_id)
        .bind(movement.location_id)
        .bind(movement.quantity)
        .bind(format!("{:?}", movement.movement_type).to_lowercase())
        .bind(movement.cost_per_unit)
        .bind(&movement.reference)
        .bind(movement.notes.as_ref())
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to record movement: {}", e)))?;

        Ok(movement)
    }

    async fn get_stock_movements(
        &self,
        product_id: Uuid,
        limit: i64,
    ) -> Result<Vec<StockMovement>> {
        let movements = sqlx::query_as::<_, StockMovement>(
            "SELECT * FROM stock_movements WHERE product_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(product_id)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch stock movements: {}", e)))?;

        Ok(movements)
    }

    async fn get_location(&self, id: Uuid) -> Result<Option<InventoryLocation>> {
        let location = sqlx::query_as::<_, InventoryLocation>(
            "SELECT * FROM inventory_locations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch location: {}", e)))?;

        Ok(location)
    }

    async fn list_locations(&self) -> Result<Vec<InventoryLocation>> {
        let locations = sqlx::query_as::<_, InventoryLocation>(
            "SELECT * FROM inventory_locations ORDER BY name",
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch locations: {}", e)))?;

        Ok(locations)
    }

    async fn create_location(&self, location: &InventoryLocation) -> Result<InventoryLocation> {
        let location = sqlx::query_as::<_, InventoryLocation>(
            r#"
            INSERT INTO inventory_locations (id, name, code, address, is_active)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(location.id)
        .bind(&location.name)
        .bind(&location.code)
        .bind(&location.address)
        .bind(location.is_active)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create location: {}", e)))?;

        Ok(location)
    }

    async fn update_location(&self, location: &InventoryLocation) -> Result<InventoryLocation> {
        let location = sqlx::query_as::<_, InventoryLocation>(
            r#"
            UPDATE inventory_locations 
            SET name = $1, code = $2, address = $3, is_active = $4, updated_at = NOW()
            WHERE id = $5
            RETURNING *
            "#,
        )
        .bind(&location.name)
        .bind(&location.code)
        .bind(&location.address)
        .bind(location.is_active)
        .bind(location.id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update location: {}", e)))?;

        Ok(location)
    }

    async fn delete_location(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM inventory_locations WHERE id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to delete location: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn is_in_stock(&self, product_id: Uuid, quantity: i32) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM inventory_levels WHERE product_id = $1 AND available_quantity >= $2"
        )
        .bind(product_id)
        .bind(quantity)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to check stock: {}", e)))?;

        Ok(count > 0)
    }

    async fn adjust_stock(
        &self,
        product_id: Uuid,
        location_id: Uuid,
        adjustment: i32,
        reason: &str,
    ) -> Result<InventoryLevel> {
        // Get current level
        let mut level = self
            .get_inventory_level(product_id, Some(location_id))
            .await?
            .ok_or_else(|| Error::not_found("Inventory level not found"))?;

        // Apply adjustment to available quantity
        level.available_quantity += adjustment;
        level.updated_at = Utc::now();

        // Save updated level
        let level = self.update_inventory_level(&level).await?;

        // Record the movement
        let movement_type = if adjustment >= 0 {
            StockMovementType::In
        } else {
            StockMovementType::Out
        };

        let movement = StockMovement {
            id: Uuid::new_v4(),
            product_id,
            variant_id: None,
            location_id,
            quantity: adjustment.abs(),
            movement_type,
            cost_per_unit: None,
            reference: reason.to_string(),
            notes: Some(reason.to_string()),
            created_at: Utc::now(),
        };
        self.record_movement(&movement).await?;

        Ok(level)
    }
}
