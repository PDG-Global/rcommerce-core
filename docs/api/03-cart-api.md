# Cart API Documentation

The Cart API provides a complete shopping cart system that supports both guest users (via session tokens) and authenticated customers. Carts persist for 30 days by default and can be seamlessly merged when a guest user logs in.

> **Status:** ✅ Fully Implemented with CartService Integration

## Overview

- **Guest Carts**: Identified by session token, no authentication required
- **Customer Carts**: Tied to authenticated customer accounts via JWT
- **Cart Merging**: Automatic merging when guest logs in
- **Database Persistence**: All carts stored in PostgreSQL via CartService
- **Expiration**: Carts expire after 30 days of inactivity

## Base URL

```
/api/v1/carts
```

## Authentication

| Endpoint | Authentication |
|----------|---------------|
| `POST /carts/guest` | None (creates session token) |
| `GET /carts/:id` | None (cart ID is the identifier) |
| `GET /carts/me` | Required (JWT Bearer token) |
| All other endpoints | Required (JWT Bearer token) |

### Using JWT Authentication

Protected endpoints require a valid JWT token in the Authorization header:

```http
Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...
```

The JWT is extracted by middleware and the `JwtAuth` context is passed to handlers:

```rust
pub struct JwtAuth {
    pub customer_id: Uuid,      // Extracted from JWT 'sub' claim
    pub email: String,          // Extracted from JWT 'email' claim
    pub permissions: Vec<String>, // Extracted from JWT permissions
}
```

## Endpoints

### Create Guest Cart

Creates a new cart for a guest user. Returns a session token that should be stored client-side.

```http
POST /api/v1/carts/guest
Content-Type: application/json
```

**Request Body:**
```json
{
  "currency": "USD"
}
```

**Response (201 Created):**

```json
{
  "cart": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "customer_id": null,
    "session_token": "sess_abc123xyz456",
    "currency": "USD",
    "subtotal": "0.00",
    "discount_total": "0.00",
    "tax_total": "0.00",
    "shipping_total": "0.00",
    "total": "0.00",
    "coupon_code": null,
    "item_count": 0,
    "created_at": "2026-01-15T10:30:00Z",
    "updated_at": "2026-01-15T10:30:00Z",
    "expires_at": "2026-02-14T10:30:00Z"
  },
  "items": []
}
```

**Important:** Store the `session_token` in localStorage or cookies for subsequent requests.

**Implementation Details:**
- Generates a session token: `sess_<uuid>`
- Creates cart in database via `CartService`
- Sets expiration to 30 days from creation
- Returns empty cart with items array

### Get or Create Customer Cart

Returns the current customer's active cart or creates a new one if none exists.

```http
GET /api/v1/carts/me
Authorization: Bearer <jwt_token>
```

**Response (200 OK):**

```json
{
  "cart": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "customer_id": "550e8400-e29b-41d4-a716-446655440002",
    "session_token": null,
    "currency": "USD",
    "subtotal": "150.00",
    "discount_total": "15.00",
    "tax_total": "13.50",
    "shipping_total": "10.00",
    "total": "158.50",
    "coupon_code": "SUMMER10",
    "item_count": 3,
    "created_at": "2026-01-15T10:30:00Z",
    "updated_at": "2026-01-15T11:00:00Z",
    "expires_at": "2026-02-14T11:00:00Z"
  },
  "items": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440003",
      "product_id": "550e8400-e29b-41d4-a716-446655440004",
      "variant_id": "550e8400-e29b-41d4-a716-446655440005",
      "quantity": 2,
      "unit_price": "50.00",
      "original_price": "50.00",
      "subtotal": "100.00",
      "discount_amount": "10.00",
      "total": "90.00",
      "sku": "PROD-001-L",
      "title": "Premium T-Shirt",
      "variant_title": "Large / Blue",
      "image_url": "https://cdn.example.com/products/001.jpg",
      "requires_shipping": true,
      "is_gift_card": false
    }
  ]
}
```

**Implementation Details:**
- Extracts `customer_id` from `Extension<JwtAuth>`
- Calls `cart_service.get_or_create_cart(CartIdentifier::Customer(id), "USD")`
- Updates expiration on each access
- Returns cart with items from database

### Get Cart by ID

Retrieves a specific cart with all items. Works for both guest and customer carts.

```http
GET /api/v1/carts/{cart_id}
```

**Response (200 OK):**

Returns the cart with items (same format as `/carts/me`).

**Error Responses:**
- `404 Not Found` - Cart ID does not exist
- `410 Gone` - Cart has expired

### Add Item to Cart

Adds a product to the cart. If the item already exists, quantities are merged.

```http
POST /api/v1/carts/{cart_id}/items
Content-Type: application/json
Authorization: Bearer <jwt_token>

{
  "product_id": "550e8400-e29b-41d4-a716-446655440004",
  "variant_id": "550e8400-e29b-41d4-a716-446655440005",
  "quantity": 2
}
```

**Request Fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `product_id` | UUID | Yes | Product to add |
| `variant_id` | UUID | No | Product variant (if applicable) |
| `quantity` | Integer | Yes | Quantity to add (must be > 0) |

**Response (201 Created):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440003",
  "product_id": "550e8400-e29b-41d4-a716-446655440004",
  "variant_id": "550e8400-e29b-41d4-a716-446655440005",
  "quantity": 2,
  "unit_price": "50.00",
  "original_price": "50.00",
  "subtotal": "100.00",
  "discount_amount": "10.00",
  "total": "90.00",
  "sku": "PROD-001-L",
  "title": "Premium T-Shirt",
  "variant_title": "Large / Blue",
  "image_url": "https://cdn.example.com/products/001.jpg",
  "requires_shipping": true,
  "is_gift_card": false
}
```

**Implementation Details:**
1. Validates quantity > 0
2. Fetches product details from `ProductService`
3. If variant specified, validates variant exists
4. Builds `ProductDetails` with pricing information
5. Calls `cart_service.add_item(cart_id, input, product_details)`
6. Recalculates cart totals automatically

**Error Responses:**
- `400 Bad Request` - Invalid quantity (must be > 0)
- `404 Not Found` - Product or variant not found
- `422 Unprocessable Entity` - Product out of stock

### Update Cart Item

Updates the quantity of a cart item. Set quantity to 0 to remove the item.

```http
PUT /api/v1/carts/{cart_id}/items/{item_id}
Content-Type: application/json
Authorization: Bearer <jwt_token>

{
  "quantity": 3
}
```

**Response (200 OK):**

Returns the updated cart item.

**Implementation Details:**
- Validates quantity >= 0
- Calls `cart_service.update_item(cart_id, item_id, input)`
- If quantity is 0, item is removed
- Recalculates cart totals

### Remove Item from Cart

Removes an item from the cart.

```http
DELETE /api/v1/carts/{cart_id}/items/{item_id}
Authorization: Bearer <jwt_token>
```

**Response (204 No Content)**

### Clear Cart

Removes all items from the cart.

```http
DELETE /api/v1/carts/{cart_id}/items
Authorization: Bearer <jwt_token>
```

**Response (204 No Content)**

### Apply Coupon

Applies a discount coupon to the cart.

```http
POST /api/v1/carts/{cart_id}/coupon
Content-Type: application/json
Authorization: Bearer <jwt_token>

{
  "coupon_code": "SUMMER20"
}
```

**Response (200 OK):**

Returns the updated `CartWithItems` with applied discount.

**Implementation Details:**
- Normalizes coupon code to uppercase
- Validates coupon via `CouponService`
- Applies discount to cart totals
- Stores coupon code on cart

**Error Responses:**
- `400 Bad Request` - Invalid or empty coupon code
- `422 Unprocessable Entity` - Coupon expired, usage limit reached, or minimum cart value not met

### Remove Coupon

Removes the applied coupon from the cart.

```http
DELETE /api/v1/carts/{cart_id}/coupon
Authorization: Bearer <jwt_token>
```

**Response (200 OK):**

Returns the updated cart without the coupon.

### Merge Guest Cart

Merges a guest cart into the authenticated customer's cart. Call this after user login.

```http
POST /api/v1/carts/merge
Content-Type: application/json
Authorization: Bearer <jwt_token>

{
  "session_token": "sess_abc123xyz456"
}
```

**Response (200 OK):**

Returns the merged `CartWithItems` with all items from both carts combined.

**Merge Behavior:**
- If both carts have the same item (same product + variant), quantities are summed
- If both carts have coupons, the customer's coupon is kept
- The guest cart is marked as converted
- Cart totals are recalculated

**Implementation:**
```rust
pub async fn merge_carts(
    State(state): State<AppState>,
    Extension(jwt_auth): Extension<JwtAuth>,
    Json(request): Json<MergeCartRequest>,
) -> Result<Json<CartWithItems>, Error> {
    let cart = state
        .cart_service
        .merge_carts(&request.session_token, jwt_auth.customer_id)
        .await?;
    
    let cart_with_items = state.cart_service.get_cart_with_items(cart.id).await?;
    Ok(Json(cart_with_items))
}
```

### Delete Cart

Permanently deletes a cart and all its items.

```http
DELETE /api/v1/carts/{cart_id}
Authorization: Bearer <jwt_token>
```

**Response (204 No Content)**

## Cart Calculations

The cart automatically calculates totals:

```
subtotal = sum(item.quantity * item.unit_price)
discount_total = sum(item.discount_amount) + cart-level discount
tax_total = calculated based on shipping address (via TaxService)
shipping_total = calculated based on shipping method (via ShippingService)
total = subtotal - discount_total + tax_total + shipping_total
```

## Error Codes

| Code | Status | Description |
|------|--------|-------------|
| `cart_not_found` | 404 | Cart ID does not exist |
| `cart_expired` | 410 | Cart has passed expiration date |
| `cart_converted` | 409 | Cart was already converted to an order |
| `item_not_found` | 404 | Cart item ID does not exist |
| `invalid_quantity` | 400 | Quantity must be between 1 and 9999 |
| `product_not_available` | 422 | Product is inactive or out of stock |
| `coupon_invalid` | 400 | Coupon code is invalid or expired |
| `coupon_minimum_not_met` | 422 | Cart subtotal below coupon minimum |
| `coupon_usage_limit` | 422 | Coupon usage limit reached |

## Webhook Events

The Cart system emits the following webhook events:

| Event | Description |
|-------|-------------|
| `cart.created` | New cart created |
| `cart.updated` | Cart details updated |
| `cart.item_added` | Item added to cart |
| `cart.item_updated` | Item quantity updated |
| `cart.item_removed` | Item removed from cart |
| `cart.cleared` | All items removed from cart |
| `cart.coupon_applied` | Coupon applied to cart |
| `cart.coupon_removed` | Coupon removed from cart |
| `cart.merged` | Guest cart merged into customer cart |
| `cart.converted` | Cart converted to order |
| `cart.deleted` | Cart permanently deleted |

## Implementation Architecture

### Service Layer

The Cart API uses `CartService` for all business logic:

```rust
pub struct CartService {
    cart_repo: Arc<dyn CartRepository>,
    coupon_repo: Arc<dyn CouponRepository>,
    coupon_service: Arc<CouponService>,
    tax_service: Option<Arc<dyn TaxService>>,
}
```

**Key Methods:**
- `get_or_create_cart(identifier, currency)` - Find or create cart
- `add_item(cart_id, input, product_details)` - Add product to cart
- `update_item(cart_id, item_id, input)` - Update item quantity
- `remove_item(cart_id, item_id)` - Remove item from cart
- `clear_cart(cart_id)` - Remove all items
- `merge_carts(session_token, customer_id)` - Merge guest to customer cart
- `apply_coupon(cart_id, input)` - Apply discount code
- `remove_coupon(cart_id)` - Remove discount code
- `get_cart_with_items(cart_id)` - Get cart with all items

### Repository Layer

PostgreSQL repository provides data access:

```rust
#[async_trait]
pub trait CartRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Cart>>;
    async fn find_active_by_customer(&self, customer_id: Uuid) -> Result<Option<Cart>>;
    async fn find_active_by_session(&self, session_token: &str) -> Result<Option<Cart>>;
    async fn create(&self, cart: &Cart) -> Result<()>;
    async fn update(&self, cart: &Cart) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
    async fn get_items(&self, cart_id: Uuid) -> Result<Vec<CartItem>>;
    async fn add_item(&self, cart_id: Uuid, item: &CartItem) -> Result<()>;
    async fn update_item(&self, item: &CartItem) -> Result<()>;
    async fn remove_item(&self, item_id: Uuid) -> Result<()>;
    async fn clear_items(&self, cart_id: Uuid) -> Result<()>;
    async fn update_expiration(&self, cart_id: Uuid, expires_at: DateTime<Utc>) -> Result<()>;
}
```

## Best Practices

1. **Session Token Storage**: Store session tokens in localStorage or cookies with appropriate security settings:
   ```javascript
   // Using localStorage
   localStorage.setItem('cart_session', cart.session_token);
   
   // Or secure cookie
   document.cookie = `cart_session=${cart.session_token}; Secure; SameSite=Strict`;
   ```

2. **Cart Persistence**: Always check for existing carts before creating new ones:
   ```javascript
   // Check for existing session first
   const sessionToken = localStorage.getItem('cart_session');
   if (sessionToken) {
     // Use existing cart
   } else {
     // Create new guest cart
   }
   ```

3. **Quantity Validation**: Validate product availability before adding to cart:
   ```javascript
   // Check stock before adding
   if (product.inventory_quantity < quantity) {
     showError('Insufficient stock');
     return;
   }
   ```

4. **Error Handling**: Handle cart expiration gracefully:
   ```javascript
   try {
     const cart = await fetchCart(cartId);
   } catch (error) {
     if (error.code === 'cart_expired') {
       // Create new cart
       const newCart = await createGuestCart();
     }
   }
   ```

5. **Merge on Login**: Always call the merge endpoint when a guest user logs in:
   ```javascript
   async function handleLogin(email, password) {
     const login = await fetch('/api/v1/auth/login', { ... });
     const sessionToken = localStorage.getItem('cart_session');
     
     if (sessionToken) {
       await fetch('/api/v1/carts/merge', {
         method: 'POST',
         headers: { 'Authorization': `Bearer ${login.access_token}` },
         body: JSON.stringify({ session_token: sessionToken })
       });
       localStorage.removeItem('cart_session');
     }
   }
   ```

## Example Flow: Guest to Customer

```javascript
// 1. Guest creates cart
const guestCart = await fetch('/api/v1/carts/guest', {
  method: 'POST'
}).then(r => r.json());
localStorage.setItem('cart_session', guestCart.cart.session_token);

// 2. Guest adds item
await fetch(`/api/v1/carts/${guestCart.cart.id}/items`, {
  method: 'POST',
  headers: { 
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({ 
    product_id: '550e8400-e29b-41d4-a716-446655440004', 
    quantity: 2 
  })
});

// 3. Guest logs in
const login = await fetch('/api/v1/auth/login', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ email: 'user@example.com', password: '...' })
}).then(r => r.json());

// 4. Merge guest cart into customer cart
await fetch('/api/v1/carts/merge', {
  method: 'POST',
  headers: { 
    'Authorization': `Bearer ${login.access_token}`,
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({ session_token: guestCart.cart.session_token })
});

// 5. Clear guest session
localStorage.removeItem('cart_session');

// 6. Get customer cart
const customerCart = await fetch('/api/v1/carts/me', {
  headers: { 'Authorization': `Bearer ${login.access_token}` }
}).then(r => r.json());

// Customer cart now contains merged items
console.log(customerCart.items);
```
