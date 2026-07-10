use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Stock reservation for an order
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StockReservation {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub location_id: Uuid,
    pub order_id: Option<Uuid>,
    pub quantity: i32,
    pub status: ReservationStatus,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl StockReservation {
    pub fn new(
        product_id: Uuid,
        variant_id: Option<Uuid>,
        location_id: Uuid,
        order_id: Option<Uuid>,
        quantity: i32,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            product_id,
            variant_id,
            location_id,
            order_id,
            quantity,
            status: ReservationStatus::Active,
            expires_at,
            created_at: Utc::now(),
        }
    }
}

/// Reservation status
#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type, Default)]
#[sqlx(type_name = "reservation_status", rename_all = "snake_case")]
pub enum ReservationStatus {
    #[default]
    Active,
    Committed,
    Released,
    Expired,
}

/// Reservation query parameters
#[derive(Debug, Clone, Default)]
pub struct ReservationFilter {
    pub product_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub status: Option<ReservationStatus>,
    pub expired_only: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reservation_creation() {
        let product_id = Uuid::new_v4();
        let location_id = Uuid::new_v4();
        let order_id = Some(Uuid::new_v4());
        let expires_at = Utc::now() + chrono::Duration::minutes(30);

        let reservation =
            StockReservation::new(product_id, None, location_id, order_id, 5, expires_at);

        assert_eq!(reservation.product_id, product_id);
        assert_eq!(reservation.quantity, 5);
        assert_eq!(reservation.status, ReservationStatus::Active);
    }
}
