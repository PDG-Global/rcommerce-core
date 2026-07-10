# Order Management System

## Overview

The Order Management System (OMS) is the core component that handles the entire lifecycle of an order from creation through fulfillment and completion. It provides comprehensive tools for order editing, status management, fraud detection, and post-purchase operations including returns and refunds.

**Key Capabilities:**
- Order lifecycle management with customizable statuses
- Order editing (add/remove items, adjust pricing)
- Order splitting and combining
- Fraud detection and risk scoring
- Returns & refunds management
- Order timeline and audit trail
- Bulk operations
- Advanced search and filtering

## Order Lifecycle

```
┌─────────────┐
│   Cart      │
└──────┬──────┘
       │ Create Order
       ▼
┌─────────────┐
│   Pending   │ ← Payment started but not completed
└──────┬──────┘
       │ Payment Success/Failure
       ▼
┌─────────────┐      ┌─────────────┐
│  Confirmed  │ ───→ │  On Hold    │ ← Manual review needed
└──────┬──────┘      └──────┬──────┘
       │                    │
       │                    └─────────────────┐
       │ Start Fulfillment                      │
       ▼                                        │
┌─────────────┐                                 │
│ Processing  │ ← Order being prepared          │
└──────┬──────┘                                 │
       │ Shipped                                 │
       ▼                                        │
┌─────────────┐                                 │
│ Completed   │ ← Successfully delivered        │
└──────┬──────┘                                 │
       │ Client confirms receipt                 │
       ▼                                        │
┌─────────────┐      ┌─────────────┐
│ Cancelled   │ ←    │  Refunded   │ ← Returned or cancelled
└─────────────┘      └─────────────┘
```

### Order Status Flow

| Status | Description | Can Edit | Can Ship | Can Refund |
|--------|-------------|----------|----------|------------|
| **Pending** | Payment initiated, awaiting confirmation |  Yes | ❌ No |  Yes |
| **Confirmed** | Payment successful, ready for fulfillment |  Yes |  Yes |  Yes |
| **Processing** | Order being picked/packed | ⚠️ Limited | ⚠️ Partial |  Yes |
| **On Hold** | Manual review required |  Yes | ❌ No |  Yes |
| **Completed** | Shipped and delivered | ❌ No | ❌ No |  Yes |
| **Cancelled** | Order cancelled | ❌ No | ❌ No | ❌ No |
| **Refunded** | Fully refunded | ❌ No | ❌ No | ❌ No |

## Core Data Models

### Order Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub order_number: String,
    pub customer_id: Uuid,
    pub customer_email: String,
    pub billing_address: Address,
    pub shipping_address: Address,
    pub line_items: Vec<OrderLineItem>,
    pub fulfillments: Vec<Fulfillment>,
    pub payments: Vec<Payment>,
    pub returns: Vec<Return>,
    pub refunds: Vec<Refund>,
    pub discounts: Vec<AppliedDiscount>,
    pub subtotal: Decimal,
    pub tax_amount: Decimal,
    pub shipping_amount: Decimal,
    pub discount_amount: Decimal,
    pub total: Decimal,
    pub currency: String,
    pub status: OrderStatus,
    pub payment_status: PaymentStatus,
    pub fulfillment_status: FulfillmentStatus,
    pub fraud_score: Option<i32>,
    pub fraud_reasons: Vec<String>,
    pub notes: Vec<OrderNote>,
    pub tags: Vec<String>,
    pub meta_data: serde_json::Value,
    pub source: OrderSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancelled_reason: Option<String>,
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "order_status", rename_all = "snake_case")]
pub enum OrderStatus {
    Pending,      // Order created, awaiting payment
    Confirmed,    // Payment confirmed, ready to fulfill
    Processing,   // Order being prepared for shipment
    OnHold,       // Manual review or exception
    Shipped,      // Order shipped (partially or fully)
    Completed,    // Fully delivered and confirmed
    Cancelled,    // Order cancelled
    Refunded,     // Fully refunded
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "payment_status", rename_all = "snake_case")]
pub enum PaymentStatus {
    Pending,      // Awaiting payment
    Authorized,   // Payment authorized (not captured)
    Paid,         // Payment completed
    PartiallyRefunded, // Partial refund issued
    FullyRefunded, // Fully refunded
    Failed,       // Payment failed
    Cancelled,    // Payment cancelled
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "fulfillment_status", rename_all = "snake_case")]
pub enum FulfillmentStatus {
    NotFulfilled,   // Not yet shipped
    PartiallyFulfilled, // Some items shipped
    Fulfilled,      // All items shipped
    Delivered,      // All items delivered
    Returned,       // All items returned
    PartiallyReturned, // Some items returned
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderLineItem {
    pub id: Uuid,
    pub order_id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub name: String,
    pub sku: Option<String>,
    pub quantity: i32,
    pub quantity_fulfilled: i32,
    pub quantity_returned: i32,
    pub unit_price: Decimal,
    pub original_unit_price: Decimal,
    pub tax_amount: Decimal,
    pub discount_amount: Decimal,
    pub total: Decimal,
    pub weight: Option<f64>,
    pub requires_shipping: bool,
    pub is_gift_card: bool,
    pub is_discountable: bool,
    pub is_taxable: bool,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderNote {
    pub id: Uuid,
    pub order_id: Uuid,
    pub author_id: Option<Uuid>,
    pub author_name: String,
    pub content: String,
    pub is_customer_visible: bool,
    pub created_at: DateTime<Utc>,
}
```

## Order Service Implementation

```rust
#[async_trait]
pub trait OrderService: Send + Sync + 'static {
    // Create order
    async fn create_order(&self, input: CreateOrderInput) -> Result<Order>;
    
    // Get order
    async fn get_order(&self, id: Uuid) -> Result<Option<Order>>;
    async fn get_order_by_number(&self, order_number: &str) -> Result<Option<Order>>;
    
    // Update order
    async fn update_order(&self, id: Uuid, update: UpdateOrderInput) -> Result<Order>;
    async fn update_order_status(&self, id: Uuid, status: OrderStatus) -> Result<Order>;
    async fn update_order_metadata(&self, id: Uuid, metadata: serde_json::Value) -> Result<Order>;
    
    // List orders
    async fn list_orders(&self, filter: OrderFilter) -> Result<Vec<Order>>;
    async fn count_orders(&self, filter: OrderFilter) -> Result<i64>;
    
    // Order editing
    async fn add_line_item(&self, order_id: Uuid, item: NewLineItem) -> Result<Order>;
    async fn remove_line_item(&self, order_id: Uuid, item_id: Uuid) -> Result<Order>;
    async fn update_line_item(&self, order_id: Uuid, item_id: Uuid, update: UpdateLineItem) -> Result<Order>;
    async void adjust_line_item(&self, order_id: Uuid, item_id: Uuid, new_quantity: i32) -> Result<Order>;
    
    // Other modifications
    async fn update_shipping_address(&self, order_id: Uuid, address: Address) -> Result<Order>;
    async fn update_billing_address(&self, order_id: Uuid, address: Address) -> Result<Order>;
    async fn apply_discount(&self, order_id: Uuid, discount_code: String) -> Result<Order>;
    async void apply_manual_discount(&self, order_id: Uuid, amount: Decimal, reason: String) -> Result<Order>;
    async fn remove_discount(&self, order_id: Uuid, discount_id: Uuid) -> Result<Order>;
    
    // Cancellation & completion
    async fn cancel_order(&self, id: Uuid, reason: String) -> Result<Order>;
    async fn complete_order(&self, id: Uuid) -> Result<Order>;
    
    // Notes
    async fn add_note(&self, order_id: Uuid, note: CreateNoteInput) -> Result<OrderNote>;
    async fn get_notes(&self, order_id: Uuid) -> Result<Vec<OrderNote>>;
    
    // Order splitting
    async fn split_order(&self, order_id: Uuid, split_items: Vec<SplitItem>) -> Result<Vec<Order>>;
    async fn combine_orders(&self, order_ids: [Uuid; 2]) -> Result<Order>;
    
    // Fraud
    async fn review_for_fraud(&self, order_id: Uuid) -> Result<FraudCheckResult>;
    async fn block_order(&self, order_id: Uuid, reason: String) -> Result<Order>;
    async fn approve_order(&self, order_id: Uuid) -> Result<Order>;
    
    // Refunds & returns
    async fn create_return(&self, order_id: Uuid, return_input: CreateReturnInput) -> Result<Return>;
    async fn process_refund(&self, order_id: Uuid, refund_input: CreateRefundInput) -> Result<Refund>;
    
    // Statistics
    async fn get_order_statistics(&self, filter: OrderFilter) -> Result<OrderStatistics>;
    async fn get_sales_report(&self, period: DateRange) -> Result<SalesReport>;
}

pub struct OrderServiceImpl {
    order_repo: Arc<dyn OrderRepository>,
    customer_repo: Arc<dyn CustomerRepository>,
    product_repo: Arc<dyn ProductRepository>,
    inventory_service: Arc<dyn InventoryService>,
    payment_service: Arc<dyn PaymentService>,
    shipping_service: Arc<dyn ShippingService>,
    tax_service: Arc<dyn TaxService>,
    discount_service: Arc<dyn DiscountService>,
    fraud_service: Arc<dyn FraudDetectionService>,
    event_dispatcher: Arc<dyn EventDispatcher>,
    logger: Logger,
}

impl OrderServiceImpl {
    pub fn new(
        order_repo: Arc<dyn OrderRepository>,
        customer_repo: Arc<dyn CustomerRepository>,
        product_repo: Arc<dyn ProductRepository>,
        inventory_service: Arc<dyn InventoryService>,
        payment_service: Arc<dyn PaymentService>,
        shipping_service: Arc<dyn ShippingService>,
        tax_service: Arc<dyn TaxService>,
        discount_service: Arc<dyn DiscountService>,
        fraud_service: Arc<dyn FraudDetectionService>,
        event_dispatcher: Arc<dyn EventDispatcher>,
    ) -> Self {
        Self {
            order_repo,
            customer_repo,
            product_repo,
            inventory_service,
            payment_service,
            shipping_service,
            tax_service,
            discount_service,
            fraud_service,
            event_dispatcher,
            logger: Logger::new("order_service"),
        }
    }
}
```

## Order Creation & Validation

```rust
#[async_trait]
impl OrderService for OrderServiceImpl {
    async fn create_order(&self, input: CreateOrderInput) -> Result<Order> {
        // 1. Validate customer
        let customer = if let Some(customer_id) = input.customer_id {
            self.customer_repo.find_by_id(customer_id).await?
                .ok_or_else(|| Error::CustomerNotFound(customer_id))?
        } else if let Some(email) = &input.email {
            // Create guest customer
            self.customer_repo.create(Customer::guest(email.clone())).await?
        } else {
            return Err(Error::MissingCustomerInfo);
        };
        
        // 2. Validate line items
        let mut line_items = Vec::new();
        let mut subtotal = Decimal::ZERO;
        let mut total_weight = 0.0;
        let mut requires_shipping = false;
        
        for item_input in input.line_items {
            let product = self.product_repo.find_by_id(item_input.product_id).await?
                .ok_or_else(|| Error::ProductNotFound(item_input.product_id))?;
            
            let variant = if let Some(variant_id) = item_input.variant_id {
                Some(product.variants.iter()
                    .find(|v| v.id == variant_id)
                    .ok_or_else(|| Error::VariantNotFound(variant_id))?)
            } else {
                None
            };
            
            // Check inventory
            let available_quantity = self.inventory_service
                .check_availability(
                    item_input.product_id,
                    item_input.variant_id,
                    item_input.quantity
                )
                .await?;
            
            if available_quantity < item_input.quantity {
                return Err(Error::InsufficientInventory {
                    product_id: item_input.product_id,
                    requested: item_input.quantity,
                    available: available_quantity,
                });
            }
            
            // Calculate pricing
            let unit_price = variant.map(|v| v.price).unwrap_or(product.price);
            let total = unit_price * Decimal::from(item_input.quantity);
            
            let line_item = OrderLineItem {
                id: Uuid::new_v4(),
                order_id: Uuid::new_v4(), // Temporary, will be set later
                product_id: item_input.product_id,
                variant_id: item_input.variant_id,
                name: item_input.name.unwrap_or_else(|| product.name.clone()),
                sku: variant.and_then(|v| v.sku.clone()).or_else(|| product.sku.clone()),
                quantity: item_input.quantity,
                quantity_fulfilled: 0,
                quantity_returned: 0,
                unit_price,
                original_unit_price: unit_price,
                tax_amount: Decimal::ZERO, // Will be calculated
                discount_amount: Decimal::ZERO, // Will be applied
                total,
                weight: variant.and_then(|v| v.weight).or_else(|| product.weight),
                requires_shipping: product.requires_shipping,
                is_gift_card: product.is_gift_card,
                is_discountable: product.is_discountable,
                is_taxable: product.is_taxable,
                metadata: item_input.metadata.unwrap_or_else(|| json!({})),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            
            subtotal += total;
            total_weight += line_item.weight.unwrap_or(0.0) * item_input.quantity as f64;
            requires_shipping = requires_shipping || product.requires_shipping;
            
            line_items.push(line_item);
        }
        
        // 3. Calculate totals
        let discount_amount = if let Some(discount_code) = &input.discount_code {
            self.calculate_discount(discount_code, &line_items, subtotal).await?
        } else {
            Decimal::ZERO
        };
        
        let subtotal_after_discount = subtotal - discount_amount;
        
        // Calculate tax
        let tax_amount = self.tax_service.calculate_tax(
            &input.shipping_address,
            &line_items,
            subtotal_after_discount,
            requires_shipping
        ).await?;
        
        // Calculate shipping
        let shipping_amount = if requires_shipping {
            self.calculate_shipping(
                &input.shipping_address,
                total_weight,
                line_items.len()
            ).await?
        } else {
            Decimal::ZERO
        };
        
        let total = subtotal_after_discount + tax_amount + shipping_amount;
        
        // 4. Create order number
        let order_number = self.generate_order_number().await?;
        
        // 5. Build order
        let mut order = Order {
            id: Uuid::new_v4(),
            order_number,
            customer_id: customer.id,
            customer_email: customer.email,
            billing_address: input.billing_address.clone(),
            shipping_address: input.shipping_address,
            line_items,
            fulfillments: vec![],
            payments: vec![],
            returns: vec![],
            refunds: vec![],
            discounts: if discount_amount > Decimal::ZERO {
                vec![AppliedDiscount {
                    code: input.discount_code.clone().unwrap(),
                    amount: discount_amount,
                    description: String::new(),
                }]
            } else {
                vec![]
            },
            subtotal,
            tax_amount,
            shipping_amount,
            discount_amount,
            total,
            currency: input.currency.unwrap_or_else(|| "USD".to_string()),
            status: OrderStatus::Pending,
            payment_status: PaymentStatus::Pending,
            fulfillment_status: FulfillmentStatus::NotFulfilled,
            fraud_score: None,
            fraud_reasons: vec![],
            notes: vec![],
            tags: input.tags.unwrap_or_default(),
            meta_data: input.meta_data.unwrap_or_else(|| json!({})),
            source: input.source,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            confirmed_at: None,
            completed_at: None,
            cancelled_at: None,
            cancelled_reason: None,
        };
        
        // 6. Reserve inventory
        for item in &order.line_items {
            self.inventory_service.reserve(
                item.product_id,
                item.variant_id,
                item.quantity
            ).await?;
        }
        
        // 7. Save order
        order = self.order_repo.create(order).await?;
        
        // 8. Fraud check
        if let Some(fraud_check) = self.fraud_service.check_order(&order).await? {
            order.fraud_score = Some(fraud_check.score);
            order.fraud_reasons = fraud_check.reasons;
            
            if fraud_check.recommendation == FraudRecommendation::Review {
                order.status = OrderStatus::OnHold;
                order = self.order_repo.update(order).await?;
            }
        }
        
        // 9. Dispatch events
        self.event_dispatcher.dispatch(
            Event::OrderCreated {
                order_id: order.id,
                order_number: order.order_number.clone(),
                customer_id: order.customer_id,
                total: order.total,
            }
        ).await?;
        
        if order.status == OrderStatus::OnHold {
            self.event_dispatcher.dispatch(
                Event::OrderOnHold {
                    order_id: order.id,
                    fraud_score: order.fraud_score,
                    reasons: order.fraud_reasons.clone(),
                }
            ).await?;
        }
        
        // 10. Update customer stats
        let _ = self.update_customer_stats(order.customer_id).await;
        
        Ok(order)
    }
    
    fn generate_order_number(&self) -> String {
        // Simple incrementing order number
        // In production, use a distributed counter or database sequence
        let timestamp = Utc::now().timestamp();
        format!("ORD-{:010}", timestamp % 1_000_000_000)
    }
}
```

## Order Editing

```rust
impl OrderServiceImpl {
    pub async fn add_line_item(
        &self,
        order_id: Uuid,
        new_item: NewLineItem,
    ) -> Result<Order> {
        let mut order = self.order_repo.find_by_id(order_id).await?
            .ok_or_else(|| Error::OrderNotFound(order_id))?;
        
        // Check if order can be edited
        if !self.can_edit_order(&order) {
            return Err(Error::OrderCannotBeEdited {
                order_id,
                status: order.status,
            });
        }
        
        // Get product
        let product = self.product_repo.find_by_id(new_item.product_id).await?
            .ok_or_else(|| Error::ProductNotFound(new_item.product_id))?;
        
        let variant = if let Some(variant_id) = new_item.variant_id {
            Some(product.variants.iter()
                .find(|v| v.id == variant_id)
                .ok_or_else(|| Error::VariantNotFound(variant_id))?)
        } else {
            None
        };
        
        // Check inventory
        let available = self.inventory_service
            .check_availability(new_item.product_id, new_item.variant_id, new_item.quantity)
            .await?;
        
        if available < new_item.quantity {
            return Err(Error::InsufficientInventory {
                product_id: new_item.product_id,
                requested: new_item.quantity,
                available,
            });
        }
        
        // Reserve inventory
        self.inventory_service.reserve(
            new_item.product_id,
            new_item.variant_id,
            new_item.quantity
        ).await?;
        
        // Create line item
        let unit_price = variant.map(|v| v.price).unwrap_or(product.price);
        let total = unit_price * Decimal::from(new_item.quantity);
        
        let line_item = OrderLineItem {
            id: Uuid::new_v4(),
            order_id,
            product_id: new_item.product_id,
            variant_id: new_item.variant_id,
            name: new_item.name.unwrap_or_else(|| product.name.clone()),
            sku: variant.and_then(|v| v.sku.clone()).or_else(|| product.sku.clone()),
            quantity: new_item.quantity,
            quantity_fulfilled: 0,
            quantity_returned: 0,
            unit_price,
            original_unit_price: unit_price,
            tax_amount: Decimal::ZERO, // Will be recalculated
            discount_amount: Decimal::ZERO, // Will be recalculated if discount applies
            total,
            weight: variant.and_then(|v| v.weight).or_else(|| product.weight),
            requires_shipping: product.requires_shipping,
            is_gift_card: product.is_gift_card,
            is_discountable: product.is_discountable,
            is_taxable: product.is_taxable,
            metadata: new_item.metadata.unwrap_or_else(|| json!({})),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        order.line_items.push(line_item);
        
        // Recalculate totals
        self.recalculate_order_totals(&mut order).await?;
        
        // Add note about the change
        order.notes.push(OrderNote {
            id: Uuid::new_v4(),
            order_id,
            author_id: None, // Will be set by authenticated user
            author_name: "System".to_string(),
            content: format!("Added item: {} (Qty: {})", product.name, new_item.quantity),
            is_customer_visible: false,
            created_at: Utc::now(),
        });
        
        // Save order
        order = self.order_repo.update(order).await?;
        
        // Dispatch event
        self.event_dispatcher.dispatch(
            Event::OrderEdited {
                order_id: order.id,
                change_type: "line_item_added".to_string(),
                changes: json!({
                    "product_id": new_item.product_id,
                    "quantity": new_item.quantity,
                    "unit_price": unit_price,
                }),
            }
        ).await?;
        
        Ok(order)
    }
    
    pub async fn remove_line_item(
        &self,
        order_id: Uuid,
        item_id: Uuid,
    ) -> Result<Order> {
        let mut order = self.order_repo.find_by_id(order_id).await?
            .ok_or_else(|| Error::OrderNotFound(order_id))?;
        
        if !self.can_edit_order(&order) {
            return Err(Error::OrderCannotBeEdited {
                order_id,
                status: order.status,
            });
        }
        
        // Find and remove item
        let item_index = order.line_items.iter()
            .position(|item| item.id == item_id)
            .ok_or_else(|| Error::OrderItemNotFound(item_id))?;
        
        let removed_item = order.line_items.remove(item_index);
        
        // Release inventory
        self.inventory_service.release(
            removed_item.product_id,
            removed_item.variant_id,
            removed_item.quantity - removed_item.quantity_fulfilled
        ).await?;
        
        // Recalculate totals
        self.recalculate_order_totals(&mut order).await?;
        
        // Add note
        order.notes.push(OrderNote {
            id: Uuid::new_v4(),
            order_id,
            author_id: None,
            author_name: "System".to_string(),
            content: format!("Removed item: {} (Qty: {})", removed_item.name, removed_item.quantity),
            is_customer_visible: false,
            created_at: Utc::now(),
        });
        
        order = self.order_repo.update(order).await?;
        
        self.event_dispatcher.dispatch(
            Event::OrderEdited {
                order_id: order.id,
                change_type: "line_item_removed".to_string(),
                changes: json!({
                    "product_id": removed_item.product_id,
                    "quantity": removed_item.quantity,
                    "refund_amount": removed_item.total,
                }),
            }
        ).await?;
        
        Ok(order)
    }
    
    fn can_edit_order(&self, order: &Order) -> bool {
        matches!(order.status, OrderStatus::Pending | 
                               OrderStatus::Confirmed | 
                               OrderStatus::OnHold)
    }
    
    async fn recalculate_order_totals(&self, order: &mut Order) -> Result<()> {
        // Recalculate subtotal
        order.subtotal = order.line_items.iter()
            .map(|item| item.unit_price * Decimal::from(item.quantity))
            .sum();
        
        // Recalculate discounts if applicable
        if !order.discounts.is_empty() {
            order.discount_amount = self.discount_service
                .recalculate_discount(&order.discounts, order.subtotal)
                .await?;
        }
        
        let subtotal_after_discount = order.subtotal - order.discount_amount;
        
        // Recalculate tax
        order.tax_amount = self.tax_service.calculate_tax(
            &order.shipping_address,
            &order.line_items,
            subtotal_after_discount,
            order.requires_shipping()
        ).await?;
        
        // Recalculate total
        order.total = subtotal_after_discount + order.tax_amount + order.shipping_amount;
        
        // Update line item totals
        for item in &mut order.line_items {
            item.total = item.unit_price * Decimal::from(item.quantity);
            item.discount_amount = Decimal::ZERO; // Individual discounts can be added later
        }
        
        Ok(())
    }
}
```

## Order Splitting & Combining

```rust
impl OrderServiceImpl {
    pub async fn split_order(
        &self,
        order_id: Uuid,
        split_items: Vec<SplitItem>,
    ) -> Result<Vec<Order>> {
        let original_order = self.order_repo.find_by_id(order_id).await?
            .ok_or_else(|| Error::OrderNotFound(order_id))?;
        
        if !self.can_edit_order(&original_order) {
            return Err(Error::OrderCannotBeEdited {
                order_id,
                status: original_order.status,
            });
        }
        
        let mut orders = Vec::new();
        
        // Group by fulfillment location or other criteria
        let grouped_items = self.group_items_for_splitting(&original_order, split_items).await?;
        
        for (group_id, items_to_move) in grouped_items {
            // Create new order for this group
            let mut new_order = original_order.clone();
            new_order.id = Uuid::new_v4();
            new_order.order_number = self.generate_order_number().await?;
            new_order.line_items = items_to_move.clone();
            new_order.fulfillments = vec![];
            new_order.payments = vec![];
            new_order.returns = vec![];
            new_order.refunds = vec![];
            
            // Recalculate totals for new order
            self.recalculate_order_totals(&mut new_order).await?;
            
            // Save new order
            new_order = self.order_repo.create(new_order).await?;
            orders.push(new_order);
        }
        
        // Remove moved items from original order
        let moved_item_ids: Vec<Uuid> = orders.iter()
            .flat_map(|o| o.line_items.iter().map(|i| i.id))
            .collect();
        
        original_order.line_items.retain(|item| !moved_item_ids.contains(&item.id));
        
        // Update original order
        self.recalculate_order_totals(&mut original_order).await?;
        let original_order = self.order_repo.update(original_order).await?;
        orders.push(original_order);
        
        // Add notes
        for order in &orders {
            self.add_note(order.id, CreateNoteInput {
                content: format!("Order split: {} items moved", order.line_items.len()),
                is_customer_visible: false,
            }).await?;
        }
        
        self.event_dispatcher.dispatch(
            Event::OrderSplit {
                original_order_id: order_id,
                new_order_ids: orders.iter().map(|o| o.id).collect(),
            }
        ).await?;
        
        Ok(orders)
    }
    
    pub async fn combine_orders(
        &self,
        order_ids: [Uuid; 2],
    ) -> Result<Order> {
        let order1 = self.order_repo.find_by_id(order_ids[0]).await?.
        ok_or_else(|| Error::OrderNotFound(order_ids[0]))?;
        let order2 = self.order_repo.find_by_id(order_ids[1]).await?.
        ok_or_else(|| Error::OrderNotFound(order_ids[1]))?;
        
        // Verify orders can be combined
        if order1.customer_id != order2.customer_id {
            return Err(Error::CannotCombineOrders {
                reason: "Orders belong to different customers".to_string(),
            });
        }
        
        if order1.shipping_address != order2.shipping_address {
            return Err(Error::CannotCombineOrders {
                reason: "Different shipping addresses".to_string(),
            });
        }
        
        if !self.can_edit_order(&order1) || !self.can_edit_order(&order2) {
            return Err(Error::CannotCombineOrders {
                reason: "One or both orders cannot be edited".to_string(),
            });
        }
        
        // Create new combined order
        let mut combined_order = order1.clone();
        combined_order.id = Uuid::new_v4();
        combined_order.order_number = self.generate_order_number().await?;
        
        // Combine line items
        combined_order.line_items.extend(order2.line_items.clone());
        
        // Remove duplicates (same product/variant)
        let mut unique_items = HashMap::new();
        for item in combined_order.line_items {
            let key = (item.product_id, item.variant_id);
            if let Some(existing) = unique_items.get_mut(&key) {
                existing.quantity += item.quantity;
                existing.total = existing.unit_price * Decimal::from(existing.quantity);
            } else {
                unique_items.insert(key, item);
            }
        }
        
        combined_order.line_items = unique_items.into_values().collect();
        
        // Recalculate totals
        self.recalculate_order_totals(&mut combined_order).await?;
        
        // Save combined order
        combined_order = self.order_repo.create(combined_order).await?;
        
        // Cancel original orders
        self.cancel_order(order1.id, "Combined with order ".to_string()).await?;
        self.cancel_order(order2.id, "Combined with order ".to_string()).await?;
        
        // Add notes
        self.add_note(combined_order.id, CreateNoteInput {
            content: format!("Combined from orders {} and {}", order1.order_number, order2.order_number),
            is_customer_visible: false,
        }).await?;
        
        self.event_dispatcher.dispatch(
            Event::OrdersCombined {
                original_order_ids: vec![order1.id, order2.id],
                combined_order_id: combined_order.id,
            }
        ).await?;
        
        Ok(combined_order)
    }
}
```

## Fraud Detection

```rust
pub struct FraudDetectionService {
    rules: Vec<Box<dyn FraudRule>>,
    provider: Option<Arc<dyn FraudProvider>>,
}

#[async_trait]
pubit trait FraudRule: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn evaluate(&self, order: &Order) -> Result<FraudRuleResult>;
}

pub struct HighValueRule {
    threshold: Decimal,
}

#[async_trait]
impl FraudRule for HighValueRule {
    fn name(&self) -> &str { "high_value_order" }
    fn description(&self) -> &str { "Flags orders above threshold value" }
    
    async fn evaluate(&self, order: &Order) -> Result<FraudRuleResult> {
        if order.total >= self.threshold {
            Ok(FraudRuleResult {
                triggered: true,
                score: 30, // Medium risk
                reason: format!("Order value ${} exceeds threshold ${}", order.total, self.threshold),
            })
        } else {
            Ok(FraudRuleResult::safe())
        }
    }
}

pub struct NewCustomerHighValueRule {
    threshold: Decimal,
    customer_repo: Arc<dyn CustomerRepository>,
}

#[async_trait]
impl FraudRule for NewCustomerHighValueRule {
    fn name(&self) -> &str { "new_customer_high_value" }
    fn description(&self) -> &str { "New customers placing high-value orders" }
    
    async fn evaluate(&self, order: &Order) -> Result<FraudRuleResult> {
        let customer = self.customer_repo.find_by_id(order.customer_id).await?.
        ok_or_else(|| Error::CustomerNotFound(order.customer_id))?;
        
        // Check if this is a new customer (no previous orders)
        if customer.orders_count == 0 && order.total >= self.threshold {
            Ok(FraudRuleResult {
                triggered: true,
                score: 50, // High risk
                reason: format!(
                    "New customer placing high-value order: ${}",
                    order.total
                ),
            })
        } else {
            Ok(FraudRuleResult::safe())
        }
    }
}

pub struct AddressMismatchRule;

#[async_trait]
impl FraudRule for AddressMismatchRule {
    fn name(&self) -> &str { "address_mismatch" }
    fn description(&self) -> &str { "Shipping and billing addresses differ significantly" }
    
    async fn evaluate(&self, order: &Order) -> Result<FraudRuleResult> {
        let bill = &order.billing_address;
        let ship = &order.shipping_address;
        
        let mut triggers = Vec::new();
        
        if bill.country != ship.country {
            triggers.push("Delivery to different country");
        }
        
        if bill.city != ship.city {
            triggers.push("Different cities");
        }
        
        if bill.postal_code != ship.postal_code {
            triggers.push("Different postal codes");
        }
        
        if !triggers.is_empty() {
            Ok(FraudRuleResult {
                triggered: true,
                score: 20 * triggers.len() as i32, // 20 points per mismatch
                reason: triggers.join(", "),
            })
        } else {
            Ok(FraudRuleResult::safe())
        }
    }
}

impl FraudDetectionService {
    pub fn new() -> Self {
        let mut rules: Vec<Box<dyn FraudRule>> = Vec::new();
        
        // Built-in rules
        rules.push(Box::new(HighValueRule {
            threshold: Decimal::from(1000),
        }));
        
        rules.push(Box::new(AddressMismatchRule));
        
        // Add more rules as needed
        
        Self {
            rules,
            provider: None,
        }
    }
    
    pub async fn check_order(&self, order: &Order) -> Result<FraudCheckResult> {
        let mut total_score = 0;
        let mut triggered_rules = Vec::new();
        let mut all_reasons = Vec::new();
        
        // Evaluate all rules
        for rule in &self.rules {
            match rule.evaluate(order).await {
                Ok(result) => {
                    if result.triggered {
                        total_score += result.score;
                        triggered_rules.push(rule.name().to_string());
                        all_reasons.push(result.reason);
                    }
                }
                Err(e) => {
                    // Log error but continue with other rules
                    tracing::error!("Fraud rule '{}' failed: {}", rule.name(), e);
                }
            }
        }
        
        // Check with external provider if configured
        if let Some(provider) = &self.provider {
            let provider_result = provider.check_order(order).await?;
            total_score = (total_score + provider_result.score) / 2; // Average scores
            triggered_rules.extend(provider_result.triggered_rules);
            all_reasons.extend(provider_result.reasons);
        }
        
        // Determine recommendation
        let recommendation = if total_score >= 75 {
            FraudRecommendation::Block
        } else if total_score >= 50 {
            FraudRecommendation::Review
        } else if total_score >= 25 {
            FraudRecommendation::Monitor
        } else {
            FraudRecommendation::Approve
        };
        
        Ok(FraudCheckResult {
            order_id: order.id,
            score: total_score,
            recommendation,
            triggered_rules,
            reasons: all_reasons,
            timestamp: Utc::now(),
        })
    }
}

pub struct FraudCheckResult {
    pub order_id: Uuid,
    pub score: i32, // 0-100, higher = riskier
    pub recommendation: FraudRecommendation,
    pub triggered_rules: Vec<String>,
    pub reasons: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FraudRecommendation {
    Approve,  // Low risk, process normally
    Monitor,  // Medium-low risk, flag for monitoring
    Review,   // Medium-high risk, manual review required
    Block,    // High risk, block order
}
```

## Order Repository

```rust
#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn create(&self, order: Order) -> Result<Order>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Order>>;
    async fn find_by_number(&self, order_number: &str) -> Result<Option<Order>>;
    async fn find_by_customer(&self, customer_id: Uuid, limit: i64) -> Result<Vec<Order>>;
    async fn find_by_status(&self, status: OrderStatus) -> Result<Vec<Order>>;
    async fn find_by_payment_status(&self, status: PaymentStatus) -> Result<Vec<Order>>;
    async fn find_by_fulfillment_status(&self, status: FulfillmentStatus) -> Result<Vec<Order>>;
    
    async fn list(&self, filter: OrderFilter, pagination: Pagination) -> Result<Vec<Order>>;
    async fn count(&self, filter: OrderFilter) -> Result<i64>;
    
    async fn update(&self, order: Order) -> Result<Order>;
    async fn update_status(&self, id: Uuid, status: OrderStatus) -> Result<Order>;
    async fn update_addresses(&self, order_id: Uuid, billing: Option<Address>, shipping: Option<Address>) -> Result<Order>;
    
    async fn add_line_item(&self, order_id: Uuid, item: OrderLineItem) -> Result<Order>;
    async fn remove_line_item(&self, order_id: Uuid, item_id: Uuid) -> Result<Order>;
    
    async fn add_note(&self, order_id: Uuid, note: OrderNote) -> Result<OrderNote>;
    async fn get_notes(&self, order_id: Uuid) -> Result<Vec<OrderNote>>;
    async fn delete_note(&self, note_id: Uuid) -> Result<bool>;
    
    async fn add_fulfillment(&self, order_id: Uuid, fulfillment: Fulfillment) -> Result<Order>;
    async fn update_fulfillment(&self, order_id: Uuid, fulfillment_id: Uuid, update: UpdateFulfillment) -> Result<Order>;
    
    async fn add_return(&self, order_id: Uuid, return: Return) -> Result<Order>;
    async fn add_refund(&self, order_id: Uuid, refund: Refund) -> Result<Order>;
    
    // Analytics
    async fn get_statistics(&self, filter: OrderFilter) -> Result<OrderStatistics>;
    async fn get_sales_report(&self, period: DateRange, group_by: GroupBy) -> Result<Vec<SalesDataPoint>>;
}

#[derive(Debug, Clone)]
pub struct OrderFilter {
    pub customer_id: Option<Uuid>,
    pub status: Option<OrderStatus>,
    pub payment_status: Option<PaymentStatus>,
    pub fulfillment_status: Option<FulfillmentStatus>,
    pub min_total: Option<Decimal>,
    pub max_total: Option<Decimal>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub tags: Option<Vec<String>>,
    pub fraud_score_min: Option<i32>,
    pub search: Option<String>,
}

pub struct OrderStatistics {
    pub total_orders: i64,
    pub total_revenue: Decimal,
    pub average_order_value: Decimal,
    pub orders_by_status: HashMap<OrderStatus, i64>,
    pub orders_by_payment_status: HashMap<PaymentStatus, i64>,
    pub orders_by_fulfillment_status: HashMap<FulfillmentStatus, i64>,
}
```

## Returns & Refunds

```rust
#[async_trait]
pub trait OrderService {
    // ... methods from above
    
    async fn create_return(&self, order_id: Uuid, input: CreateReturnInput) -> Result<Return>;
    async fn process_refund(&self, order_id: Uuid, refund_input: CreateRefundInput) -> Result<Refund>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReturnInput {
    pub order_id: Uuid,
    pub items: Vec<ReturnItemInput>,
    pub return_reason: String,
    pub return_reason_note: Option<String>,
    pub note: Option<String>,
    pub receive_items: bool, // Whether to expect items back
}

#[derive(Debug, Clone)]
pub struct ReturnItemInput {
    pub line_item_id: Uuid,
    pub quantity: i32,
    return_reason: String,
    note: Option<String>,
}

impl OrderServiceImpl {
    pub async fn create_return(
        &self,
        order_id: Uuid,
        input: CreateReturnInput,
    ) -> Result<Return> {
        let order = self.order_repo.find_by_id(order_id).await?.
            ok_or_else(|| Error::OrderNotFound(order_id))?;
        
        // Verify all items are returnable
        let mut return_items = Vec::new();
        let mut total_refund_amount = Decimal::ZERO;
        
        for item_input in input.items {
            let line_item = order.line_items.iter()
                .find(|li| li.id == item_input.line_item_id)
                .ok_or_else(|| Error::OrderItemNotFound(item_input.line_item_id))?;
            
            // Verify quantity
            if item_input.quantity > (line_item.quantity - line_item.quantity_returned) {
                return Err(Error::InvalidReturnQuantity {
                    item_id: item_input.line_item_id,
                    requested: item_input.quantity,
                    available: line_item.quantity - line_item.quantity_returned,
                });
            }
            
            let refund_amount = line_item.unit_price * Decimal::from(item_input.quantity);
            total_refund_amount += refund_amount;
            
            return_items.push(ReturnItem {
                id: Uuid::new_v4(),
                return_id: Uuid::new_v4(), // Will be set later
                order_line_item_id: line_item.id,
                quantity: item_input.quantity,
                return_reason: item_input.return_reason.clone(),
                note: item_input.note,
                refund_amount,
                metadata: json!({}),
            });
        }
        
        // Create return
        let return = Return {
            id: Uuid::new_v4(),
            order_id,
            return_number: self.generate_return_number().await?,
            items: return_items,
            received_at: None,
            refund_processed_at: None,
            total_refund_amount,
            status: ReturnStatus::Requested,
            return_reason: input.return_reason,
            return_reason_note: input.return_reason_note,
            note: input.note,
            receive_items: input.receive_items,
            metadata: json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        // Save return
        let return = self.order_repo.add_return(order_id, return).await?;
        
        // Update order fulfillment status if all items returned
        if self.all_items_returned(&order) {
            let mut updated_order = order;
            updated_order.fulfillment_status = FulfillmentStatus::Returned;
            self.order_repo.update(updated_order).await?;
        }
        
        // Dispatch events
        self.event_dispatcher.dispatch(
            Event::ReturnRequested {
                return_id: return.id,
                order_id: return.order_id,
                total_refund_amount: return.total_refund_amount,
            }
        ).await?;
        
        Ok(return)
    }
    
    pub async fn process_refund(
        &self,
        order_id: Uuid,
        refund_input: CreateRefundInput,
    ) -> Result<Refund> {
        let order = self.order_repo.find_by_id(order_id).await?.
            ok_or_else(|| Error::OrderNotFound(order_id))?;
        
        let refund_amount = refund_input.amount;
        
        // Check if refund exceeds total paid
        let total_paid = order.total;
        let total_refunded = order.refunds.iter()
            .map(|r| r.amount)
            .sum::<Decimal>();
        
        if total_refunded + refund_amount > total_paid {
            return Err(Error::RefundExceedsPayment {
                requested: refund_amount,
                remaining: total_paid - total_refunded,
            });
        }
        
        // Process payment refund
        if let Some(payment) = order.payments.iter()
            .find(|p| p.status == PaymentStatus::Paid)
        {
            let payment_gateway = self.payment_service
                .get_gateway(&payment.gateway)
                .await?;
            
            // Refund via payment gateway
            payment_gateway.refund_payment(
                payment,
                refund_amount,
                refund_input.reason.clone()
            ).await?;
        }
        
        // Create refund record
        let refund = Refund {
            id: Uuid::new_v4(),
            order_id,
            refund_number: self.generate_refund_number().await?,
            amount: refund_amount,
            currency: order.currency.clone(),
            reason: refund_input.reason,
            processed_at: Some(Utc::now()),
            refunded_by: refund_input.refunded_by,
            metadata: refund_input.metadata.unwrap_or_else(|| json!({})),
            created_at: Utc::now(),
            refund_items: refund_input.items.map(|items| {
                items.into_iter().map(|item| RefundItem {
                    line_item_id: item.line_item_id,
                    quantity: item.quantity,
                    amount: item.amount,
                    reason: item.reason,
                }).collect()
            }).unwrap_or_default(),
        };
        
        // Save refund
        let refund = self.order_repo.add_refund(order_id, refund).await?;
        
        // Update order payment status
        let total_after_refund = total_refunded + refund_amount;
        order.payment_status = if total_after_refund == total_paid {
            PaymentStatus::FullyRefunded
        } else {
            PaymentStatus::PartiallyRefunded
        };
        
        let order = self.order_repo.update(order).await?;
        
        // Dispatch events
        self.event_dispatcher.dispatch(
            Event::OrderRefunded {
                order_id,
                refund_id: refund.id,
                amount: refund.amount,
                payment_status: order.payment_status,
            }
        ).await?;
        
        Ok(refund)
    }
}
```

---

This completes the comprehensive order management system documentation. Next: Cross-platform deployment guide.
