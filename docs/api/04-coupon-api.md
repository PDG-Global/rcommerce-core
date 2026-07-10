# Coupon API Documentation

The Coupon API provides a comprehensive discount system supporting percentage off, fixed amount off, free shipping, and Buy X Get Y promotions. Coupons can be restricted to specific products, collections, customers, or date ranges.

## Overview

- **Discount Types**: Percentage, Fixed Amount, Free Shipping, Buy X Get Y
- **Restrictions**: Products, collections, minimum purchase, date ranges
- **Usage Limits**: Global and per-customer limits
- **Combination Rules**: Control whether coupons can stack

## Base URL

```
/api/v1/coupons
```

## Authentication

All coupon management endpoints require admin authentication. Coupon validation (applying to cart) uses cart authentication.

| Endpoint | Authentication |
|----------|---------------|
| POST /coupons | Admin required |
| PUT /coupons/{id} | Admin required |
| DELETE /coupons/{id} | Admin required |
| GET /coupons | Admin required |
| POST /carts/{id}/coupon | Cart auth (session or JWT) |

## Discount Types

### Percentage

Percentage off the subtotal. Can optionally specify a maximum discount cap.

```json
{
  "discount_type": "percentage",
  "discount_value": 20,
  "maximum_discount": 50.00
}
```

**Example:** 20% off, max $50 discount

### Fixed Amount

Fixed amount off the subtotal.

```json
{
  "discount_type": "fixed_amount",
  "discount_value": 25.00
}
```

**Example:** $25 off orders

### Free Shipping

Removes shipping costs from the order.

```json
{
  "discount_type": "free_shipping"
}
```

### Buy X Get Y (BOGO)

Buy a certain quantity, get additional items free or discounted.

```json
{
  "discount_type": "buy_x_get_y",
  "buy_x": 2,
  "get_y": 1,
  "discount_value": 100
}
```

**Example:** Buy 2, get 1 free (100% off)

## Endpoints

### Create Coupon

Creates a new discount coupon.

```http
POST /api/v1/coupons
Content-Type: application/json
Authorization: Bearer <admin_jwt>

{
  "code": "SUMMER2026",
  "description": "Summer Sale - 20% off everything",
  "discount_type": "percentage",
  "discount_value": 20,
  "minimum_purchase": 50.00,
  "maximum_discount": 100.00,
  "starts_at": "2026-06-01T00:00:00Z",
  "expires_at": "2026-08-31T23:59:59Z",
  "usage_limit": 1000,
  "usage_limit_per_customer": 1,
  "applies_to_specific_products": false,
  "applies_to_specific_collections": false,
  "can_combine": false
}
```

**Response (201 Created):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "code": "SUMMER2026",
  "description": "Summer Sale - 20% off everything",
  "discount_type": "percentage",
  "discount_value": "20.00",
  "minimum_purchase": "50.00",
  "maximum_discount": "100.00",
  "is_active": true,
  "starts_at": "2026-06-01T00:00:00Z",
  "expires_at": "2026-08-31T23:59:59Z",
  "usage_limit": 1000,
  "usage_limit_per_customer": 1,
  "usage_count": 0,
  "applies_to_specific_products": false,
  "applies_to_specific_collections": false,
  "can_combine": false,
  "created_at": "2026-01-28T10:00:00Z",
  "updated_at": "2026-01-28T10:00:00Z"
}
```

### Get Coupon

Retrieves a specific coupon by ID.

```http
GET /api/v1/coupons/{coupon_id}
Authorization: Bearer <admin_jwt>
```

**Response (200 OK):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "code": "SUMMER2026",
  "description": "Summer Sale - 20% off everything",
  "discount_type": "percentage",
  "discount_value": "20.00",
  "minimum_purchase": "50.00",
  "maximum_discount": "100.00",
  "is_active": true,
  "starts_at": "2026-06-01T00:00:00Z",
  "expires_at": "2026-08-31T23:59:59Z",
  "usage_limit": 1000,
  "usage_limit_per_customer": 1,
  "usage_count": 150,
  "applies_to_specific_products": false,
  "applies_to_specific_collections": false,
  "can_combine": false,
  "created_at": "2026-01-28T10:00:00Z",
  "updated_at": "2026-01-28T10:00:00Z"
}
```

### Get Coupon by Code

Retrieves a coupon by its code (for validation purposes).

```http
GET /api/v1/coupons/code/{code}
Authorization: Bearer <admin_jwt>
```

### List Coupons

Returns all coupons with optional filtering.

```http
GET /api/v1/coupons?active_only=true
Authorization: Bearer <admin_jwt>
```

**Query Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `active_only` | boolean | Only return active coupons |
| `discount_type` | string | Filter by discount type |

**Response (200 OK):**

```json
{
  "coupons": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "code": "SUMMER2026",
      "discount_type": "percentage",
      "discount_value": "20.00",
      "is_active": true,
      "usage_count": 150,
      "usage_limit": 1000
    }
  ],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 50,
    "total_pages": 3
  }
}
```

### Update Coupon

Updates an existing coupon.

```http
PUT /api/v1/coupons/{coupon_id}
Content-Type: application/json
Authorization: Bearer <admin_jwt>

{
  "description": "Updated description",
  "is_active": false,
  "discount_value": 25.00,
  "expires_at": "2026-09-30T23:59:59Z"
}
```

**Response (200 OK):**

Returns the updated coupon.

### Delete Coupon

Deletes a coupon. Cannot delete if coupon has been used.

```http
DELETE /api/v1/coupons/{coupon_id}
Authorization: Bearer <admin_jwt>
```

**Response (204 No Content)**

**Error Response:**

```json
{
  "error": "COUPON_HAS_USAGE",
  "message": "Cannot delete coupon that has been used"
}
```

### Add Product Restriction

Restricts a coupon to specific products (or excludes products).

```http
POST /api/v1/coupons/{coupon_id}/products
Content-Type: application/json
Authorization: Bearer <admin_jwt>

{
  "product_id": "550e8400-e29b-41d4-a716-446655440001",
  "is_exclusion": false
}
```

**Response (201 Created)**

### Remove Product Restriction

Removes a product restriction from a coupon.

```http
DELETE /api/v1/coupons/{coupon_id}/products/{product_id}
Authorization: Bearer <admin_jwt>
```

**Response (204 No Content)**

### Add Collection Restriction

Restricts a coupon to specific collections.

```http
POST /api/v1/coupons/{coupon_id}/collections
Content-Type: application/json
Authorization: Bearer <admin_jwt>

{
  "collection_id": "550e8400-e29b-41d4-a716-446655440002",
  "is_exclusion": false
}
```

**Response (201 Created)**

### Get Coupon Statistics

Returns usage statistics for a coupon.

```http
GET /api/v1/coupons/{coupon_id}/stats
Authorization: Bearer <admin_jwt>
```

**Response (200 OK):**

```json
{
  "coupon_id": "550e8400-e29b-41d4-a716-446655440000",
  "code": "SUMMER2026",
  "total_usage": 150,
  "total_discount_amount": "3250.00",
  "remaining_uses": 850,
  "usage_by_day": [
    {
      "date": "2026-06-01",
      "usage_count": 25,
      "discount_amount": "500.00"
    }
  ]
}
```

### Validate Coupon

Validates a coupon code without applying it (for preview).

```http
POST /api/v1/coupons/validate
Content-Type: application/json
X-Session-Token: <session_token>

{
  "code": "SUMMER2026",
  "cart_id": "550e8400-e29b-41d4-a716-446655440003"
}
```

**Response (200 OK) - Valid:**

```json
{
  "valid": true,
  "coupon": {
    "code": "SUMMER2026",
    "discount_type": "percentage",
    "discount_value": "20.00"
  },
  "discount_calculation": {
    "original_amount": "150.00",
    "discount_amount": "30.00",
    "final_amount": "120.00"
  }
}
```

**Response (200 OK) - Invalid:**

```json
{
  "valid": false,
  "error_code": "MINIMUM_PURCHASE_NOT_MET",
  "error_message": "Minimum purchase of $50.00 required",
  "minimum_required": "50.00",
  "current_amount": "35.00"
}
```

## Coupon Validation Rules

When applying a coupon to a cart, the following validations are performed:

### 1. Basic Validations

| Check | Error Code |
|-------|------------|
| Coupon exists | `COUPON_NOT_FOUND` |
| Coupon is active | `COUPON_INACTIVE` |
| Current date within validity period | `COUPON_EXPIRED` / `COUPON_NOT_STARTED` |
| Global usage limit not exceeded | `COUPON_USAGE_LIMIT_REACHED` |
| Customer usage limit not exceeded | `COUPON_CUSTOMER_LIMIT_REACHED` |

### 2. Purchase Validations

| Check | Error Code |
|-------|------------|
| Minimum purchase amount met | `MINIMUM_PURCHASE_NOT_MET` |
| Cart contains applicable products | `COUPON_DOES_NOT_APPLY` |
| Can combine with existing discounts | `COUPON_CANNOT_COMBINE` |

### 3. Product Restrictions

If `applies_to_specific_products` is true:
- Only products in the coupon's product list receive the discount
- Excluded products are never discounted

If `applies_to_specific_collections` is true:
- Only products in specified collections receive the discount

## Discount Calculation Examples

### Percentage with Cap

```
Subtotal: $300
Discount: 20%
Maximum: $50

Calculation: min($300 × 0.20, $50) = $50
Final: $300 - $50 = $250
```

### Fixed Amount

```
Subtotal: $75
Discount: $25

Calculation: min($75, $25) = $25
Final: $75 - $25 = $50
```

### Product-Specific Percentage

```
Cart:
- Product A: $100 (in coupon list)
- Product B: $50 (not in list)
- Product C: $75 (in coupon list)

Discount: 10% on applicable items
Calculation: ($100 + $75) × 0.10 = $17.50
Final: $225 - $17.50 = $207.50
```

## Webhooks

The Coupon system emits the following webhook events:

| Event | Description |
|-------|-------------|
| `coupon.created` | New coupon created |
| `coupon.updated` | Coupon details updated |
| `coupon.deleted` | Coupon deleted |
| `coupon.applied` | Coupon applied to cart |
| `coupon.usage_recorded` | Coupon usage tracked for order |

## Best Practices

### Coupon Code Format

- Use uppercase alphanumeric codes
- Avoid ambiguous characters (0, O, 1, I, L)
- Keep codes between 6-12 characters
- Use descriptive prefixes: `SUMMER20`, `WELCOME10`, `FLASH50`

### Discount Strategies

1. **Percentage Discounts**: Best for encouraging larger orders
2. **Fixed Amount**: Best for specific promotions
3. **Free Shipping**: Best for overcoming shipping hesitation
4. **BOGO**: Best for clearing inventory

### Usage Limits

- Set reasonable global limits to prevent abuse
- Per-customer limits prevent repeat abuse
- Consider time-based limits for urgency

### Testing Coupons

Always test coupons with:
- Minimum purchase thresholds
- Product restrictions
- Combination with other discounts
- Edge cases (exact minimum, empty cart)

## Example: Creating a Complex Coupon

```http
POST /api/v1/coupons
Content-Type: application/json
Authorization: Bearer <admin_jwt>

{
  "code": "VIP2026",
  "description": "VIP Members - 25% off electronics",
  "discount_type": "percentage",
  "discount_value": 25,
  "minimum_purchase": 100.00,
  "maximum_discount": 200.00,
  "starts_at": "2026-01-01T00:00:00Z",
  "expires_at": "2026-12-31T23:59:59Z",
  "usage_limit": 500,
  "usage_limit_per_customer": 2,
  "applies_to_specific_collections": true,
  "can_combine": false
}
```

Then add collection restriction:

```http
POST /api/v1/coupons/{coupon_id}/collections
Authorization: Bearer <admin_jwt>

{
  "collection_id": "550e8400-e29b-41d4-a716-446655440004",
  "is_exclusion": false
}
```
