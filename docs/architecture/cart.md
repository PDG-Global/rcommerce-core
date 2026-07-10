# Cart System Architecture

## Overview

The Cart System in R Commerce provides a complete shopping cart implementation that supports both guest users (via session tokens) and authenticated customers. Carts are fully persisted to the database with a 30-day expiration period.

## Key Features

- **Guest Carts**: Session-based carts identified by unique tokens
- **Customer Carts**: Tied to authenticated customer accounts
- **Cart Merging**: Automatic merging when a guest user logs in
- **Database Persistence**: All carts stored in PostgreSQL
- **Coupon Support**: Apply and remove discount codes
- **Tax Calculation**: Integration with TaxService for accurate totals

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    API Layer (Axum)                          │
│  POST /carts/guest      - Create guest cart                 │
│  GET  /carts/me         - Get customer cart                 │
│  POST /carts/:id/items  - Add item to cart                  │
│  POST /carts/merge      - Merge guest to customer cart      │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                    Cart Service                              │
│  - get_or_create_cart()   - Find or create cart             │
│  - add_item()             - Add product to cart             │
│  - update_item()          - Modify cart item quantity       │
│  - remove_item()          - Remove item from cart           │
│  - clear_cart()           - Remove all items                │
│  - merge_carts()          - Merge guest into customer cart  │
│  - apply_coupon()         - Apply discount code             │
│  - remove_coupon()        - Remove discount code            │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                 Cart Repository (PostgreSQL)                 │
│  - find_by_id()           - Get cart by UUID                │
│  - find_active_by_customer() - Get customer's active cart   │
│  - find_active_by_session()  - Get guest cart by token      │
│  - create()               - Insert new cart                 │
│  - update()               - Update cart totals              │
│  - add_item()             - Insert cart item                │
│  - update_item()          - Update cart item                │
│  - remove_item()          - Delete cart item                │
└─────────────────────────────────────────────────────────────┘
```

## Data Models

### Cart

```rust
pub struct Cart {
    pub id: Uuid,
    pub customer_id: Option<Uuid>,      // None for guest carts
    pub session_token: Option<String>,  // None for customer carts
    pub currency: Currency,
    pub subtotal: Decimal,
    pub discount_total: Decimal,
    pub tax_total: Decimal,
    pub shipping_total: Decimal,
    pub total: Decimal,
    pub coupon_code: Option<String>,
    pub email: Option<String>,
    pub shipping_address_id: Option<Uuid>,
    pub billing_address_id: Option<Uuid>,
    pub shipping_method: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub converted_to_order: bool,
    pub order_id: Option<Uuid>,
}
```

### CartItem

```rust
pub struct CartItem {
    pub id: Uuid,
    pub cart_id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub original_price: Decimal,
    pub subtotal: Decimal,
    pub discount_amount: Decimal,
    pub total: Decimal,
    pub sku: Option<String>,
    pub title: String,
    pub variant_title: Option<String>,
    pub image_url: Option<String>,
    pub requires_shipping: bool,
    pub is_gift_card: bool,
    pub custom_attributes: Option<Value>,
}
```

### CartIdentifier

Used to identify carts for both guest and customer scenarios:

```rust
pub enum CartIdentifier {
    Customer(Uuid),      // Authenticated customer
    Session(String),     // Guest session token
    CartId(Uuid),        // Direct cart ID
}
```

## Cart Lifecycle

### 1. Guest Cart Creation

When a visitor first adds an item to their cart:

1. Frontend calls `POST /api/v1/carts/guest`
2. Server generates a session token: `sess_<uuid>`
3. Creates cart in database with `session_token` set
4. Returns cart with session token to frontend
5. Frontend stores session token in localStorage/cookies

```rust
pub async fn create_guest_cart(
    State(state): State<AppState>,
) -> Result<Json<CartWithItems>, Error> {
    // Generate session token for guest cart
    let session_token = format!("sess_{}", Uuid::new_v4().to_string().replace("-", ""));

    // Get or create cart via service
    let cart = state
        .cart_service
        .get_or_create_cart(CartIdentifier::Session(session_token.clone()), "USD")
        .await?;

    // Return cart with items
    let cart_with_items = state.cart_service.get_cart_with_items(cart.id).await?;
    Ok(Json(cart_with_items))
}
```

### 2. Adding Items

```rust
pub async fn add_item_to_cart(
    State(state): State<AppState>,
    Path(cart_id): Path<Uuid>,
    Json(request): Json<AddItemRequest>,
) -> Result<Json<CartItem>, Error> {
    // Validate quantity
    if request.quantity <= 0 {
        return Err(Error::validation("Quantity must be greater than 0"));
    }

    // Get product details from product service
    let product_detail = state
        .product_service
        .get_product(request.product_id)
        .await?
        .ok_or_else(|| Error::not_found("Product not found"))?;

    // Build product details (variant or base product)
    let product_details = if let Some(variant_id) = request.variant_id {
        // Find variant and build details...
    } else {
        // Use product-level details...
    };

    // Add item to cart via service
    let cart_item = state
        .cart_service
        .add_item(cart_id, input, product_details)
        .await?;

    Ok(Json(cart_item))
}
```

### 3. Customer Cart Access

When an authenticated customer accesses their cart:

```rust
pub async fn get_customer_cart(
    State(state): State<AppState>,
    Extension(jwt_auth): Extension<JwtAuth>,  // Extracted by middleware
) -> Result<Json<CartWithItems>, Error> {
    // Get or create cart for authenticated customer
    let cart = state
        .cart_service
        .get_or_create_cart(CartIdentifier::Customer(jwt_auth.customer_id), "USD")
        .await?;

    // Return cart with items
    let cart_with_items = state.cart_service.get_cart_with_items(cart.id).await?;
    Ok(Json(cart_with_items))
}
```

### 4. Cart Merging on Login

When a guest user logs in, their guest cart is merged into their customer cart:

```rust
pub async fn merge_carts(
    State(state): State<AppState>,
    Extension(jwt_auth): Extension<JwtAuth>,
    Json(request): Json<MergeCartRequest>,
) -> Result<Json<CartWithItems>, Error> {
    // Merge carts via service
    let cart = state
        .cart_service
        .merge_carts(&request.session_token, jwt_auth.customer_id)
        .await?;

    // Return merged cart with items
    let cart_with_items = state.cart_service.get_cart_with_items(cart.id).await?;
    Ok(Json(cart_with_items))
}
```

**Merge Behavior:**
- If both carts have the same item (same product + variant), quantities are summed
- If both carts have coupons, the customer's coupon is kept (guest coupon discarded)
- The guest cart is marked as converted
- Cart totals are recalculated

## Database Schema

### carts table

```sql
CREATE TABLE carts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID REFERENCES customers(id),
    session_token VARCHAR(100) UNIQUE,
    currency currency NOT NULL DEFAULT 'USD',
    subtotal DECIMAL(10,2) NOT NULL DEFAULT 0,
    discount_total DECIMAL(10,2) NOT NULL DEFAULT 0,
    tax_total DECIMAL(10,2) NOT NULL DEFAULT 0,
    shipping_total DECIMAL(10,2) NOT NULL DEFAULT 0,
    total DECIMAL(10,2) NOT NULL DEFAULT 0,
    coupon_code VARCHAR(100),
    email VARCHAR(255),
    shipping_address_id UUID REFERENCES addresses(id),
    billing_address_id UUID REFERENCES addresses(id),
    shipping_method VARCHAR(100),
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    converted_to_order BOOLEAN NOT NULL DEFAULT false,
    order_id UUID REFERENCES orders(id)
);

CREATE INDEX idx_carts_customer_id ON carts(customer_id) WHERE customer_id IS NOT NULL;
CREATE INDEX idx_carts_session_token ON carts(session_token) WHERE session_token IS NOT NULL;
CREATE INDEX idx_carts_expires_at ON carts(expires_at);
```

### cart_items table

```sql
CREATE TABLE cart_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cart_id UUID NOT NULL REFERENCES carts(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id),
    variant_id UUID REFERENCES product_variants(id),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price DECIMAL(10,2) NOT NULL,
    original_price DECIMAL(10,2) NOT NULL,
    subtotal DECIMAL(10,2) NOT NULL,
    discount_amount DECIMAL(10,2) NOT NULL DEFAULT 0,
    total DECIMAL(10,2) NOT NULL,
    sku VARCHAR(100),
    title VARCHAR(255) NOT NULL,
    variant_title VARCHAR(255),
    image_url TEXT,
    requires_shipping BOOLEAN NOT NULL DEFAULT true,
    is_gift_card BOOLEAN NOT NULL DEFAULT false,
    custom_attributes JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_cart_items_cart_id ON cart_items(cart_id);
CREATE INDEX idx_cart_items_product_id ON cart_items(product_id);
```

## Cart Calculations

Cart totals are automatically calculated:

```
subtotal = sum(item.quantity * item.unit_price)
discount_total = sum(item.discount_amount) + cart-level discount
tax_total = calculated based on shipping address and tax rules
shipping_total = selected shipping method rate
total = subtotal - discount_total + tax_total + shipping_total
```

## Integration with Other Services

### Tax Calculation

The cart service integrates with the TaxService for accurate tax calculation:

```rust
pub async fn get_cart_with_totals(
    &self,
    cart_id: Uuid,
    shipping_address: Option<&Address>,
    vat_id: Option<&str>,
) -> Result<CartWithTotals> {
    let cart_with_items = self.get_cart_with_items(cart_id).await?;
    
    // Calculate tax if address provided and tax service available
    let (tax_total, tax_breakdown) = if let (Some(address), Some(tax_service)) = 
        (shipping_address, &self.tax_service) {
        self.calculate_cart_tax(&cart, &items, address, vat_id).await?
    } else {
        (Decimal::ZERO, None)
    };
    
    // Calculate totals
    let total = cart.subtotal - cart.discount_total + tax_total + cart.shipping_total;
    
    Ok(CartWithTotals { cart, items, tax_total, calculated_total: total, tax_breakdown })
}
```

### Coupon Integration

Coupons are applied through the CouponService:

```rust
pub async fn apply_coupon(
    &self,
    cart_id: Uuid,
    input: ApplyCouponInput,
) -> Result<Cart> {
    // Validate coupon code
    let validation = self.coupon_service.validate_code(&input.coupon_code, cart.customer_id).await?;
    
    if !validation.valid {
        return Err(Error::validation(&validation.message));
    }
    
    // Apply coupon and recalculate totals
    let cart = self.coupon_repo.apply_coupon(cart_id, &input.coupon_code).await?;
    self.recalculate_cart_totals(cart_id).await?;
    
    Ok(cart)
}
```

## Authentication & Authorization

Cart endpoints have different authentication requirements:

### Public Endpoints (No Auth)
- `POST /carts/guest` - Create guest cart
- `GET /carts/:id` - Get cart by ID

### Protected Endpoints (JWT Required)
- `GET /carts/me` - Get customer cart
- `POST /carts/:id/items` - Add item
- `PUT /carts/:id/items/:item_id` - Update item
- `DELETE /carts/:id/items/:item_id` - Remove item
- `DELETE /carts/:id/items` - Clear cart
- `POST /carts/merge` - Merge carts
- `POST /carts/:id/coupon` - Apply coupon
- `DELETE /carts/:id/coupon` - Remove coupon

The `Extension<JwtAuth>` extractor is used to get the authenticated customer ID:

```rust
pub struct JwtAuth {
    pub customer_id: Uuid,
    pub email: String,
    pub permissions: Vec<String>,
}
```

## Error Handling

Common cart-related errors:

| Error | Code | Description |
|-------|------|-------------|
| Cart not found | 404 | Cart ID does not exist |
| Cart expired | 410 | Cart has passed expiration date |
| Cart converted | 409 | Cart was already converted to an order |
| Invalid quantity | 400 | Quantity must be 1-9999 |
| Product not available | 422 | Product inactive or out of stock |
| Coupon invalid | 400 | Coupon code invalid or expired |
| Coupon minimum not met | 422 | Cart value below coupon minimum |

## Webhook Events

The cart system emits the following webhook events:

| Event | Description |
|-------|-------------|
| `cart.created` | New cart created |
| `cart.updated` | Cart details updated |
| `cart.item_added` | Item added to cart |
| `cart.item_updated` | Item quantity/attributes updated |
| `cart.item_removed` | Item removed from cart |
| `cart.coupon_applied` | Coupon applied to cart |
| `cart.coupon_removed` | Coupon removed from cart |
| `cart.merged` | Guest cart merged into customer cart |
| `cart.converted` | Cart converted to order |

## Best Practices

1. **Session Token Storage**: Store session tokens securely in localStorage or cookies with appropriate security settings
2. **Cart Persistence**: Always check for existing carts before creating new ones
3. **Quantity Validation**: Validate product availability before adding to cart
4. **Error Handling**: Handle cart expiration gracefully by creating new carts
5. **Merge on Login**: Always call the merge endpoint when a guest user logs in
6. **Token Cleanup**: Clear guest session token from storage after successful merge

## Example Flow: Guest to Customer

```javascript
// 1. Guest creates cart
const guestCart = await fetch('/api/v1/carts/guest', {
  method: 'POST'
});
const { id, session_token } = await guestCart.json();
localStorage.setItem('cart_session', session_token);

// 2. Guest adds items
await fetch(`/api/v1/carts/${id}/items`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ product_id: '...', quantity: 2 })
});

// 3. Guest logs in
const login = await fetch('/api/v1/auth/login', { ... });
const { access_token } = await login.json();

// 4. Merge guest cart into customer cart
await fetch('/api/v1/carts/merge', {
  method: 'POST',
  headers: { 
    'Authorization': `Bearer ${access_token}`,
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({ session_token })
});

// 5. Clear guest session
localStorage.removeItem('cart_session');
```
