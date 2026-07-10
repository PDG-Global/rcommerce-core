# API Error Codes Reference

This document provides a comprehensive reference for all error codes returned by the R commerce API.

## Error Response Format

All errors follow a consistent JSON format:

```json
{
  "error": {
    "code": "error_code_string",
    "message": "Human-readable error description",
    "details": {
      "additional": "context",
      "request_id": "req_abc123xyz"
    },
    "documentation": "https://docs.rcommerce.app/errors/error_code_string"
  },
  "meta": {
    "request_id": "req_abc123xyz",
    "timestamp": "2024-01-23T14:13:35Z"
  }
}
```

## HTTP Status Codes

| Status | Meaning | Retryable |
|--------|---------|-----------|
| 400 | Bad Request | No |
| 401 | Unauthorized | No |
| 403 | Forbidden | No |
| 404 | Not Found | No |
| 409 | Conflict | Yes (with backoff) |
| 422 | Unprocessable Entity | No |
| 429 | Too Many Requests | Yes (with backoff) |
| 500 | Internal Server Error | Yes |
| 502 | Bad Gateway | Yes |
| 503 | Service Unavailable | Yes |

## Error Code Categories

### 1xx - General Errors

#### `bad_request`
- **Status**: 400
- **Message**: "The request could not be understood"
- **Common Causes**: Invalid JSON, malformed request body
- **Solution**: Check request syntax and JSON validity

#### `unauthorized`
- **Status**: 401
- **Message**: "Authentication is required"
- **Common Causes**: Missing or invalid API key
- **Solution**: Include valid `Authorization: Bearer <api_key>` header

#### `forbidden`
- **Status**: 403
- **Message**: "Access to this resource is forbidden"
- **Common Causes**: 
  - API key doesn't have required permissions
  - Restricted key accessing restricted resource
  - IP not in whitelist
- **Solution**: Check API key permissions and IP restrictions

#### `not_found`
- **Status**: 404
- **Message**: "The requested resource was not found"
- **Common Causes**: Invalid ID, resource deleted
- **Solution**: Verify resource ID exists

#### `method_not_allowed`
- **Status**: 405
- **Message**: "HTTP method not allowed for this endpoint"
- **Common Causes**: Using POST instead of PUT, etc.
- **Solution**: Check allowed methods for endpoint

#### `conflict`
- **Status**: 409
- **Message**: "Request conflicts with current state"
- **Common Causes**: Concurrent modification, duplicate unique fields
- **Solution**: Retry with backoff, check for race conditions

#### `unprocessable_entity`
- **Status**: 422
- **Message**: "Request was well-formed but invalid"
- **Common Causes**: Business rule violations, validation errors
- **Solution**: Check validation errors in response

#### `too_many_requests`
- **Status**: 429
- **Message**: "Rate limit exceeded"
- **Common Causes**: Too many requests in time window
- **Solution**: Wait and retry with exponential backoff

#### `internal_error`
- **Status**: 500
- **Message**: "Internal server error occurred"
- **Common Causes**: Unexpected server error
- **Solution**: Retry with backoff, contact support if persists

#### `not_implemented`
- **Status**: 501
- **Message**: "Feature not implemented"
- **Common Causes**: Using deprecated/unavailable feature
- **Solution**: Check documentation for alternatives

#### `service_unavailable`
- **Status**: 503
- **Message**: "Service temporarily unavailable"
- **Common Causes**: Server maintenance, overload
- **Solution**: Retry with exponential backoff

---

### 2xx - Payment Errors

#### `payment_failed`
- **Status**: 402
- **Message**: "Payment processing failed"
- **Details**: 
  ```json
  {
    "gateway": "stripe",
    "gateway_error_code": "card_declined",
    "gateway_message": "Your card was declined",
    "decline_code": "insufficient_funds"
  }
  ```
- **Common Causes**: 
  - Insufficient funds
  - Card declined
  - Expired card
  - Invalid card details
- **Solution**: 
  - Retry with different payment method
  - Check card details
  - Contact card issuer

#### `payment_blocked_by_fraud`
- **Status**: 402
- **Message**: "Payment blocked by fraud detection"
- **Details**:
  ```json
  {
    "fraud_score": 85,
    "fraud_reasons": ["Transaction from high-risk country", "Billing address mismatch"],
    "recommendation": "Review"
  }
  ```
- **Common Causes**: Fraud rules triggered
- **Solution**: Manual review required

#### `payment_gateway_error`
- **Status**: 502
- **Message**: "Payment gateway error"
- **Details**:
  ```json
  {
    "gateway": "stripe",
    "gateway_error": "Gateway timeout",
    "retryable": true
  }
  ```
- **Common Causes**: Gateway downtime, network issues
- **Solution**: Retry with backoff

#### `payment_method_invalid`
- **Status**: 422
- **Message**: "Payment method is invalid"
- **Details**:
  ```json
  {
    "field": "payment_method",
    "reason": "Expired card"
  }
  ```
- **Common Causes**: Expired card, invalid number, wrong CVV
- **Solution**: Use valid payment method

#### `payment_amount_invalid`
- **Status**: 422
- **Message**: "Payment amount is invalid"
- **Details**:
  ```json
  {
    "amount": 5.00,
    "currency": "USD",
    "min_amount": 10.00,
    "max_amount": 10000.00
  }
  ```
- **Common Causes**: Amount below minimum or above maximum
- **Solution**: Adjust payment amount

#### `payment_currency_unsupported`
- **Status**: 422
- **Message**: "Currency not supported"
- **Details**:
  ```json
  {
    "currency": "XYZ",
    "supported_currencies": ["USD", "EUR", "GBP", "JPY"]
  }
  ```
- **Common Causes**: Unsupported currency
- **Solution**: Use supported currency

---

### 3xx - Order Errors

#### `order_not_found`
- **Status**: 404
- **Message**: "Order not found"
- **Details**:
  ```json
  {
    "order_id": "ord_123456"
  }
  ```

#### `order_not_editable`
- **Status**: 422
- **Message**: "Order cannot be edited in current status"
- **Details**:
  ```json
  {
    "order_id": "ord_123456",
    "current_status": "completed",
    "editable_statuses": ["pending", "confirmed", "on_hold"]
  }
  ```
- **Common Causes**: Trying to edit shipped/completed order
- **Solution**: Cancel and recreate or contact support

#### `order_status_invalid`
- **Status**: 422
- **Message**: "Invalid order status transition"
- **Details**:
  ```json
  {
    "order_id": "ord_123456",
    "from_status": "pending",
    "to_status": "completed",
    "valid_transitions": {
      "pending": ["confirmed", "cancelled"],
      "confirmed": ["processing", "on_hold", "cancelled"],
      "processing": ["shipped", "on_hold"],
      "shipped": ["completed"]
    }
  }
  ```
- **Common Causes**: Invalid status transition
- **Solution**: Use valid status transition

#### `order_item_not_found`
- **Status**: 404
- **Message**: "Order line item not found"

#### `insufficient_inventory`
- **Status**: 422
- **Message**: "Insufficient inventory for product"
- **Details**:
  ```json
  {
    "product_id": "prod_123",
    "variant_id": "var_456",
    "requested": 10,
    "available": 5,
    "inventory_policy": "deny"
  }
  ```
- **Common Causes**: 
  - Low stock
  - Concurrent orders
  - Inventory policy set to "deny"
- **Solution**: 
  - Reduce quantity
  - Wait for restock
  - Change inventory policy

#### `order_fulfilled`
- **Status**: 422
- **Message**: "Order already fulfilled"

---

### 4xx - Product Errors

#### `product_not_found`
- **Status**: 404
- **Message**: "Product not found"

#### `variant_not_found`
- **Status**: 404
- **Message**: "Product variant not found"

#### `product_out_of_stock`
- **Status**: 422
- **Message**: "Product is out of stock"
- **Details**:
  ```json
  {
    "product_id": "prod_123",
    "inventory_quantity": 0,
    "inventory_policy": "deny",
    "restock_date": "2024-02-01"
  }
  ```

#### `product_not_published`
- **Status**: 422
- **Message**: "Product not available for purchase"
- **Details**:
  ```json
  {
    "product_id": "prod_123",
    "status": "draft",
    "published_at": null
  }
  ```

---

### 5xx - Customer Errors

#### `customer_not_found`
- **Status**: 404
- **Message**: "Customer not found"

#### `customer_duplicate_email`
- **Status**: 409
- **Message**: "Customer with this email already exists"
- **Details**:
  ```json
  {
    "email": "customer@example.com",
    "existing_customer_id": "cus_123"
  }
  ```

#### `customer_missing_info`
- **Status**: 422
- **Message**: "Missing required customer information"
- **Details**:
  ```json
  {
    "missing_fields": ["email", "first_name"]
  }
  ```

---

### 6xx - Discount Errors

#### `discount_not_found`
- **Status**: 404
- **Message**: "Discount code not found"

#### `discount_expired`
- **Status**: 422
- **Message**: "Discount code has expired"
- **Details**:
  ```json
  {
    "code": "SAVE20",
    "expired_at": "2024-01-01T00:00:00Z"
  }
  ```

#### `discount_usage_limit`
- **Status**: 422
- **Message**: "Discount usage limit reached"
- **Details**:
  ```json
  {
    "code": "SAVE20",
    "usage_limit": 100,
    "usage_count": 100
  }
  ```

#### `discount_min_cart_value`
- **Status**: 422
- **Message**: "Cart value below discount minimum"
- **Details**:
  ```json
  {
    "code": "SAVE20",
    "cart_value": 45.00,
    "min_required": 50.00
  }
  ```

#### `discount_not_applicable`
- **Status**: 422
- **Message**: "Discount not applicable to cart items"
- **Details**:
  ```json
  {
    "code": "SAVE20",
    "reason": "Excludes sale items"
  }
  ```

---

### 7xx - Shipping Errors

#### `shipping_provider_error`
- **Status**: 502
- **Message**: "Shipping provider error"

#### `shipping_unavailable`
- **Status**: 422
- **Message**: "Shipping not available for address"
- **Details**:
  ```json
  {
    "address": {
      "country": "XZ",
      "postal_code": "99999"
    },
    "reason": "No shipping providers available"
  }
  ```

#### `shipping_rate_expired`
- **Status**: 409
- **Message**: "Shipping rates have expired"

#### `invalid_shipping_address`
- **Status**: 422
- **Message**: "Shipping address is invalid"
- **Details**:
  ```json
  {
    "address": { ... },
    "validation_errors": [
      "Postal code required",
      "Invalid country code"
    ]
  }
  ```

---

### 8xx - Checkout Errors

#### `checkout_cart_empty`
- **Status**: 422
- **Message**: "Cannot checkout with empty cart"
- **Common Causes**: Attempting to checkout without items in cart
- **Solution**: Add items to cart before checkout

#### `checkout_cart_not_found`
- **Status**: 404
- **Message**: "Cart not found for checkout"
- **Details**:
  ```json
  {
    "cart_id": "550e8400-e29b-41d4-a716-446655440000"
  }
  ```

#### `checkout_cart_expired`
- **Status**: 410
- **Message**: "Cart has expired"
- **Details**:
  ```json
  {
    "cart_id": "550e8400-e29b-41d4-a716-446655440000",
    "expired_at": "2026-01-01T00:00:00Z"
  }
  ```

#### `checkout_invalid_shipping_method`
- **Status**: 422
- **Message**: "Selected shipping method is invalid"
- **Common Causes**: Shipping rate no longer available, rate expired
- **Solution**: Re-initiate checkout to get fresh shipping rates

#### `checkout_payment_failed`
- **Status**: 402
- **Message**: "Payment processing failed"
- **Details**:
  ```json
  {
    "payment_error": "Card declined",
    "retryable": true
  }
  ```

#### `checkout_invalid_payment_method`
- **Status**: 422
- **Message**: "Invalid payment method"
- **Common Causes**: Unsupported payment type, invalid token
- **Solution**: Use a supported payment method

#### `checkout_already_completed`
- **Status**: 409
- **Message**: "Checkout already completed"
- **Details**:
  ```json
  {
    "cart_id": "550e8400-e29b-41d4-a716-446655440000",
    "order_id": "550e8400-e29b-41d4-a716-446655440001"
  }
  ```

---

### 9xx - Cart Errors

#### `cart_not_found`
- **Status**: 404
- **Message**: "Cart not found"
- **Details**:
  ```json
  {
    "cart_id": "550e8400-e29b-41d4-a716-446655440000"
  }
  ```

#### `cart_expired`
- **Status**: 410
- **Message**: "Cart has expired"
- **Details**:
  ```json
  {
    "cart_id": "550e8400-e29b-41d4-a716-446655440000",
    "expired_at": "2026-01-01T00:00:00Z"
  }
  ```

#### `cart_converted`
- **Status**: 409
- **Message**: "Cart was already converted to an order"
- **Details**:
  ```json
  {
    "cart_id": "550e8400-e29b-41d4-a716-446655440000",
    "order_id": "550e8400-e29b-41d4-a716-446655440001"
  }
  ```

#### `cart_item_not_found`
- **Status**: 404
- **Message**: "Cart item not found"

#### `invalid_quantity`
- **Status**: 400
- **Message**: "Invalid quantity"
- **Details**:
  ```json
  {
    "quantity": 0,
    "min_quantity": 1,
    "max_quantity": 9999
  }
  ```

#### `product_not_available`
- **Status**: 422
- **Message**: "Product is not available for purchase"
- **Details**:
  ```json
  {
    "product_id": "550e8400-e29b-41d4-a716-446655440000",
    "reason": "Out of stock"
  }
  ```

---

### 8xx - Validation Errors

#### `validation_error`
- **Status**: 422
- **Message**: "Request validation failed"
- **Details**:
  ```json
  {
    "field_errors": {
      "email": ["Invalid email format"],
      "price": ["Must be a positive number"],
      "quantity": ["Must be at least 1"]
    }
  }
  ```

#### `invalid_field_value`
- **Status**: 422
- **Message**: "Field has invalid value"
- **Details**:
  ```json
  {
    "field": "status",
    "value": "invalid_status",
    "allowed_values": ["active", "draft", "archived"]
  }
  ```

#### `missing_required_field`
- **Status**: 422
- **Message**: "Required field is missing"
- **Details**:
  ```json
  {
    "field": "email",
    "resource": "Customer"
  }
  ```

---

### 9xx - Configuration Errors

#### `config_error`
- **Status**: 500
- **Message**: "Configuration error"

#### `missing_config`
- **Status**: 500
- **Message**: "Required configuration missing"
- **Details**:
  ```json
  {
    "config_key": "payments.stripe.secret_key"
  }
  ```

---

## Webhook Error Codes

### Webhook Delivery Errors

#### `webhook_delivery_failed`
- **Status**: 500 (internal)
- **Message**: "Webhook delivery failed"
- **Details**:
  ```json
  {
    "webhook_id": "wh_123",
    "url": "https://example.com/webhook",
    "status_code": 500,
    "retry_count": 3
  }
  ```

#### `webhook_signature_invalid`
- **Status**: 401
- **Message**: "Webhook signature verification failed"

#### `webhook_timeout`
- **Status**: 504
- **Message**: "Webhook delivery timed out"

---

## Compatibility Layer Errors

### Platform-Specific Errors

#### `compatibility_unsupported_endpoint`
- **Status**: 501
- **Message**: "Endpoint not supported in compatibility mode"
- **Details**:
  ```json
  {
    "platform": "woocommerce",
    "endpoint": "/wc-api/v3/bookings",
    "reason": "Bookings not supported"
  }
  ```

#### `compatibility_mapping_failed`
- **Status**: 500
- **Message**: "Failed to map request to native format"

---

## Error Handling Best Practices

### Client-Side Error Handling (JavaScript)

```javascript
class RCommerceError extends Error {
  constructor(errorResponse) {
    super(errorResponse.error.message);
    this.code = errorResponse.error.code;
    this.details = errorResponse.error.details;
    this.requestId = errorResponse.meta.request_id;
    this.documentation = errorResponse.error.documentation;
  }
}

async function handleApiCall() {
  try {
    const response = await fetch('/v1/orders', {
      headers: { 'Authorization': 'Bearer sk_xxx' }
    });
    
    if (!response.ok) {
      const errorData = await response.json();
      
      // Check if error is retryable
      if (isRetryableError(errorData.error.code)) {
        await retryWithBackoff(() => handleApiCall());
        return;
      }
      
      throw new RCommerceError(errorData);
    }
    
    return await response.json();
  } catch (error) {
    console.error(`Request ${error.requestId} failed:`, error);
    
    // Show user-friendly message
    showUserNotification(error.message);
  }
}

function isRetryableError(errorCode) {
  const retryableCodes = new Set([
    'conflict',
    'too_many_requests',
    'internal_error',
    'payment_gateway_error',
    'service_unavailable'
  ]);
  
  return retryableCodes.has(errorCode);
}

async function retryWithBackoff(fn, maxRetries = 3) {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await fn();
    } catch (error) {
      if (i === maxRetries - 1) throw error;
      
      // Exponential backoff
      const delay = Math.pow(2, i) * 1000 + Math.random() * 1000;
      await sleep(delay);
    }
  }
}
```

### Server-Side Error Handling (Rust)

```rust
use thiserror::Error;
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use serde_json::json;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Order not found")]
    OrderNotFound(Uuid),
    
    #[error("Payment processing failed: {0}")]
    PaymentFailed(String),
    
    #[error("Insufficient inventory")]
    InsufficientInventory {
        product_id: Uuid,
        requested: i32,
        available: i32,
    },
    
    #[error(transparent)]
    ValidationError(#[from] validator::ValidationErrors),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_code, details) = match self {
            ApiError::OrderNotFound(order_id) => (
                StatusCode::NOT_FOUND,
                "order_not_found",
                json!({ "order_id": order_id })
            ),
            
            ApiError::PaymentFailed(details) => (
                StatusCode::PAYMENT_REQUIRED,
                "payment_failed",
                json!({ "details": details })
            ),
            
            ApiError::InsufficientInventory { product_id, requested, available } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "insufficient_inventory",
                json!({
                    "product_id": product_id,
                    "requested": requested,
                    "available": available
                })
            ),
            
            ApiError::ValidationError(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                json!({ "field_errors": errors })
            ),
        };
        
        let request_id = generate_request_id();
        
        let body = json!({
            "error": {
                "code": error_code,
                "message": self.to_string(),
                "details": details,
                "documentation": format!("https://docs.rcommerce.app/errors/{}", error_code)
            },
            "meta": {
                "request_id": request_id,
                "timestamp": Utc::now()
            }
        });
        
        (status, body).into_response()
    }
}
```

---

## Error Code Index (Alphabetical)

| Code | Category | Description |
|------|----------|-------------|
| `address_mismatch` | Fraud | Billing and shipping addresses differ |
| `bad_request` | General | Malformed request |
| `card_declined` | Payment | Card was declined by issuer |
| `cart_converted` | Cart | Cart already converted to order |
| `cart_expired` | Cart | Cart has passed expiration date |
| `cart_item_not_found` | Cart | Cart item ID does not exist |
| `cart_not_found` | Cart | Cart ID does not exist |
| `checkout_already_completed` | Checkout | Checkout already processed |
| `checkout_cart_empty` | Checkout | Cannot checkout with empty cart |
| `checkout_cart_expired` | Checkout | Cart expired during checkout |
| `checkout_cart_not_found` | Checkout | Cart not found for checkout |
| `checkout_invalid_payment_method` | Checkout | Invalid payment method |
| `checkout_invalid_shipping_method` | Checkout | Shipping method no longer available |
| `checkout_payment_failed` | Checkout | Payment processing failed |
| `conflict` | General | Concurrent modification |
| `customer_duplicate_email` | Customer | Email already exists |
| `customer_not_found` | Customer | Customer ID not found |
| `discount_expired` | Discount | Discount code past expiration |
| `discount_min_cart_value` | Discount | Cart below minimum threshold |
| `discount_not_applicable` | Discount | Discount doesn't apply to items |
| `discount_not_found` | Discount | Discount code not found |
| `discount_usage_limit` | Discount | Usage limit exceeded |
| `expired_card` | Payment | Card is past expiration date |
| `forbidden` | General | Permission denied |
| `fraud_high_risk` | Fraud | Transaction flagged as high risk |
| `gateway_error` | Payment | Payment gateway error |
| `insufficient_funds` | Payment | Card has insufficient funds |
| `insufficient_inventory` | Order | Not enough stock available |
| `insufficient_permissions` | Security | API key lacks required scope |
| `invalid_amount` | Payment | Payment amount invalid |
| `invalid_api_key` | Security | API key is invalid or expired |
| `invalid_card_number` | Payment | Card number format invalid |
| `invalid_currency` | Payment | Currency not supported |
| `invalid_cvv` | Payment | CVV verification failed |
| `invalid_field_value` | Validation | Field value outside allowed range |
| `invalid_parameters` | Validation | Request parameters invalid |
| `invalid_quantity` | Cart | Quantity must be 1-9999 |
| `invalid_request` | General | Request format incorrect |
| `ip_restricted` | Security | IP address not whitelisted |
| `method_not_allowed` | General | HTTP method not allowed |
| `missing_api_key` | Security | No API key provided |
| `missing_required_field` | Validation | Required field not provided |
| `network_error` | Payment | Network communication failed |
| `not_found` | General | Resource not found |
| `not_implemented` | General | Feature not implemented |
| `order_cannot_be_edited` | Order | Order status prevents editing |
| `order_fulfilled` | Order | Order already shipped |
| `order_item_not_found` | Order | Line item not found |
| `order_not_found` | Order | Order ID not found |
| `payment_blocked_by_fraud` | Fraud | Payment blocked by fraud check |
| `payment_expired` | Payment | Payment session expired |
| `payment_failed` | Payment | Payment processing failed |
| `payment_gateway_error` | Payment | Gateway communication error |
| `payment_intent_not_found` | Payment | Payment intent ID not found |
| `payment_method_invalid` | Payment | Payment method format invalid |
| `payment_processing` | Payment | Payment currently processing |
| `payment_unsupported_currency` | Payment | Currency not supported by gateway |
| `product_not_available` | Cart/Product | Product not available for purchase |
| `product_not_found` | Product | Product ID not found |
| `product_not_published` | Product | Product not available for sale |
| `product_out_of_stock` | Product | Inventory quantity is zero |
| `rate_limit_exceeded` | Security | Too many requests |
| `refund_exceeds_payment` | Refund | Refund amount exceeds payment |
| `resource_locked` | General | Resource temporarily locked |
| `return_not_found` | Order | Return ID not found |
| `server_error` | General | Internal server error |
| `service_unavailable` | General | Service temporarily unavailable |
| `shipping_provider_error` | Shipping | Shipping provider error |
| `shipping_rate_expired` | Shipping | Shipping rates no longer valid |
| `shipping_unavailable` | Shipping | Shipping not available to address |
| `too_many_requests` | Rate Limit | Rate limit exceeded |
| `unauthorized` | Security | Authentication required |
| `unprocessable_entity` | General | Request well-formed but invalid |
| `unsupported_currency` | Payment | Currency not supported |
| `validation_error` | Validation | One or more validation errors |
| `variant_not_found` | Product | Variant ID not found |
| `webhook_delivery_failed` | Webhook | Webhook delivery failed |
| `webhook_signature_invalid` | Webhook | Webhook signature invalid |
| `webhook_timeout` | Webhook | Webhook delivery timed out |

---

## Error Recovery Strategies

### Immediate Retry (No delay)
- `conflict` - Concurrent modification, retry immediately

### Short Retry (1-5 seconds)
- `internal_error` - Temporary server issue
- `payment_gateway_error` - Gateway hiccup
- `service_unavailable` - Brief maintenance

### Medium Retry (30-300 seconds)
- `too_many_requests` - Rate limit, wait for reset
- `webhook_delivery_failed` - Temporary network issue

### Long Retry (5-60 minutes)
- `server_error` - Server issues
- `shipping_provider_error` - Provider downtime

### No Retry (Manual intervention)
- `unauthorized` - Fix authentication first
- `forbidden` - Check permissions
- `not_found` - Verify IDs
- `validation_error` - Fix request data
- `payment_failed` - Use different payment method
- `insufficient_inventory` - Check stock levels

---

## Contacting Support

When reporting errors to support, always include:
1. **Request ID** (`meta.request_id`)
2. **Error code** (`error.code`)
3. **Timestamp** (`meta.timestamp`)
4. **Endpoint** you were calling
5. **Request data** (sanitize sensitive info)

This helps us quickly identify and resolve issues.
