use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::Order;
use crate::Result;
use serde::{Deserialize, Serialize};

/// Order status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "order_status", rename_all = "snake_case")]
pub enum OrderStatus {
    Pending,    // Order created, awaiting payment
    Confirmed,  // Payment received, order confirmed
    Processing, // Order is being prepared/picked
    OnHold,     // Order is on hold
    Shipped,    // Order has been shipped
    Delivered,  // Order has been delivered
    Completed,  // Order is complete
    // The Postgres enum uses British spelling; sqlx's snake_case rename
    // would map this variant to "canceled", which is not a database value.
    #[sqlx(rename = "cancelled")]
    Canceled, // Order has been canceled
    Refunded, // Order has been refunded
}

impl OrderStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Completed | OrderStatus::Canceled | OrderStatus::Refunded
        )
    }

    pub fn can_transition_to(&self, new_status: OrderStatus) -> bool {
        use OrderStatus::*;

        match (self, new_status) {
            // Can cancel from most states (except terminal states)
            (_, Canceled) if !self.is_terminal() => true,

            // Pending transitions
            (Pending, Confirmed) => true,
            (Pending, Canceled) => true,

            // Confirmed transitions
            (Confirmed, Processing) => true,
            (Confirmed, Canceled) => true,

            // Processing transitions
            (Processing, Shipped) => true,
            (Processing, OnHold) => true,

            // OnHold transitions
            (OnHold, Processing) => true,
            (OnHold, Canceled) => true,

            // Shipped transitions
            (Shipped, Delivered) => true,

            // Delivered transitions
            (Delivered, Completed) => true,

            // Completed transitions
            (Completed, Refunded) => true,

            // Refunded is terminal
            (Refunded, _) => false,

            // No other transitions allowed
            _ => false,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "Order created, awaiting payment",
            OrderStatus::Confirmed => "Payment received, order confirmed",
            OrderStatus::Processing => "Order is being prepared",
            OrderStatus::OnHold => "Order is on hold",
            OrderStatus::Shipped => "Order has been shipped",
            OrderStatus::Delivered => "Order has been delivered",
            OrderStatus::Completed => "Order is complete",
            OrderStatus::Canceled => "Order has been canceled",
            OrderStatus::Refunded => "Order has been refunded",
        }
    }
}

/// Order event for event sourcing/dispatching
#[derive(Debug, Clone)]
pub enum OrderEvent {
    OrderCreated {
        order_id: Uuid,
        customer_id: Option<Uuid>,
        total: Decimal,
        currency: String,
    },
    OrderStatusChanged {
        order_id: Uuid,
        old_status: OrderStatus,
        new_status: OrderStatus,
        reason: Option<String>,
    },
    PaymentReceived {
        order_id: Uuid,
        payment_id: String,
        amount: Decimal,
        currency: String,
    },
    PaymentFailed {
        order_id: Uuid,
        payment_id: String,
        error: String,
    },
    OrderShipped {
        order_id: Uuid,
        tracking_number: String,
        carrier: String,
    },
    OrderDelivered {
        order_id: Uuid,
        delivered_at: DateTime<Utc>,
    },
    OrderCanceled {
        order_id: Uuid,
        reason: String,
    },
    OrderRefunded {
        order_id: Uuid,
        refund_id: String,
        amount: Decimal,
        reason: String,
    },
    InventoryReserved {
        order_id: Uuid,
        product_ids: Vec<Uuid>,
    },
    InventoryReleased {
        order_id: Uuid,
        product_ids: Vec<Uuid>,
    },
}

/// Order transition for audit trail
#[derive(Debug, Clone)]
pub struct OrderTransition {
    pub order_id: Uuid,
    pub from_status: OrderStatus,
    pub to_status: OrderStatus,
    pub reason: Option<String>,
    pub user_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl OrderTransition {
    pub fn new(order_id: Uuid, from: OrderStatus, to: OrderStatus, user_id: Option<Uuid>) -> Self {
        Self {
            order_id,
            from_status: from,
            to_status: to,
            reason: None,
            user_id,
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
        }
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = Some(reason);
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Order event dispatcher for pub/sub pattern
#[derive(Default)]
pub struct OrderEventDispatcher {
    // In a real implementation, this would have:
    // - Event bus connection
    // - Registered event handlers
    // - Async dispatcher
}

impl OrderEventDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dispatch order created event
    pub async fn order_created(&self, order: &Order) -> Result<()> {
        let event = OrderEvent::OrderCreated {
            order_id: order.id,
            customer_id: order.customer_id,
            total: order.total,
            currency: order.currency.clone(),
        };

        self.dispatch(event).await
    }

    /// Dispatch order status changed event
    pub async fn order_status_changed(&self, order: &Order, old_status: OrderStatus) -> Result<()> {
        let event = OrderEvent::OrderStatusChanged {
            order_id: order.id,
            old_status,
            new_status: order.status,
            reason: None, // TODO: Add reason parameter
        };

        self.dispatch(event).await
    }

    /// Dispatch payment received event
    pub async fn payment_received(
        &self,
        order: &Order,
        payment_id: String,
        amount: Decimal,
    ) -> Result<()> {
        let event = OrderEvent::PaymentReceived {
            order_id: order.id,
            payment_id,
            amount,
            currency: order.currency.clone(),
        };

        self.dispatch(event).await
    }

    /// Dispatch payment failed event
    pub async fn payment_failed(
        &self,
        order: &Order,
        payment_id: String,
        error: String,
    ) -> Result<()> {
        let event = OrderEvent::PaymentFailed {
            order_id: order.id,
            payment_id,
            error,
        };

        self.dispatch(event).await
    }

    /// Dispatch order shipped event
    pub async fn order_shipped(
        &self,
        order: &Order,
        tracking_number: String,
        carrier: String,
    ) -> Result<()> {
        let event = OrderEvent::OrderShipped {
            order_id: order.id,
            tracking_number,
            carrier,
        };

        self.dispatch(event).await
    }

    /// Dispatch order delivered event
    pub async fn order_delivered(&self, order: &Order) -> Result<()> {
        let event = OrderEvent::OrderDelivered {
            order_id: order.id,
            delivered_at: Utc::now(),
        };

        self.dispatch(event).await
    }

    /// Dispatch order canceled event
    pub async fn order_canceled(&self, order: &Order, reason: String) -> Result<()> {
        let event = OrderEvent::OrderCanceled {
            order_id: order.id,
            reason,
        };

        self.dispatch(event).await
    }

    /// Dispatch order refunded event
    pub async fn order_refunded(
        &self,
        order: &Order,
        refund_id: String,
        amount: Decimal,
        reason: String,
    ) -> Result<()> {
        let event = OrderEvent::OrderRefunded {
            order_id: order.id,
            refund_id,
            amount,
            reason,
        };

        self.dispatch(event).await
    }

    /// Dispatch fulfillment created event
    pub async fn fulfillment_created(&self, fulfillment: &crate::order::Fulfillment) -> Result<()> {
        log::info!(
            "Fulfillment created: {} for order {}",
            fulfillment.id,
            fulfillment.order_id
        );

        // In a real implementation, this would dispatch to event bus
        Ok(())
    }

    /// Notify inventory service about reservation
    pub async fn inventory_reserved(&self, order_id: Uuid, product_ids: Vec<Uuid>) -> Result<()> {
        let event = OrderEvent::InventoryReserved {
            order_id,
            product_ids,
        };

        self.dispatch(event).await
    }

    /// Notify inventory service about release
    pub async fn inventory_released(&self, order_id: Uuid, product_ids: Vec<Uuid>) -> Result<()> {
        let event = OrderEvent::InventoryReleased {
            order_id,
            product_ids,
        };

        self.dispatch(event).await
    }

    /// Internal dispatch method
    async fn dispatch(&self, event: OrderEvent) -> Result<()> {
        // TODO: Implement actual event dispatching
        // - Serialize event
        // - Send to message queue (RabbitMQ, Redis, etc.)
        // - Call registered handlers

        log::info!("Dispatching order event: {:?}", event);

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OrderLifecycleError {
    #[error("Invalid status transition from {from:?} to {to:?}")]
    InvalidStatusTransition { from: OrderStatus, to: OrderStatus },

    #[error("Order is in terminal state {status:?}")]
    TerminalStatus { status: OrderStatus },

    #[error("Payment required before status change")]
    PaymentRequired,

    #[error("Fulfillment required before delivery")]
    FulfillmentRequired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_transitions() {
        assert!(OrderStatus::Pending.can_transition_to(OrderStatus::Confirmed));
        assert!(OrderStatus::Confirmed.can_transition_to(OrderStatus::Processing));
        assert!(!OrderStatus::Pending.can_transition_to(OrderStatus::Shipped));
        assert!(OrderStatus::Confirmed.can_transition_to(OrderStatus::Canceled));
        assert!(OrderStatus::Processing.can_transition_to(OrderStatus::Canceled));
        assert!(OrderStatus::Shipped.can_transition_to(OrderStatus::Delivered));
        assert!(!OrderStatus::Completed.can_transition_to(OrderStatus::Canceled));
    }

    #[test]
    fn test_terminal_states() {
        assert!(OrderStatus::Completed.is_terminal());
        assert!(OrderStatus::Canceled.is_terminal());
        assert!(OrderStatus::Refunded.is_terminal());
        assert!(!OrderStatus::Pending.is_terminal());
        assert!(!OrderStatus::Confirmed.is_terminal());
    }
}
