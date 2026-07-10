# Checkout System Architecture

## Overview

The Checkout System in R Commerce provides a complete three-step checkout flow that handles tax calculation, shipping rate selection, and payment processing. It integrates with Cart, Tax, Shipping, and Payment services to provide a seamless checkout experience.

## Key Features

- **Three-Step Checkout**: Initiate → Select Shipping → Complete
- **Tax Calculation**: Real-time tax calculation based on shipping address
- **Shipping Rates**: Multi-provider shipping rate calculation
- **Payment Processing**: Provider-agnostic payment handling
- **Order Creation**: Automatic order creation on successful payment
- **Cart Conversion**: Carts are marked as converted after checkout

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    API Layer (Axum)                          │
│  POST /checkout/initiate  - Start checkout, get rates       │
│  POST /checkout/shipping  - Select shipping method          │
│  POST /checkout/complete  - Process payment, create order   │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                  Checkout Service                            │
│  - initiate_checkout()    - Calculate tax & shipping        │
│  - select_shipping()      - Apply shipping selection        │
│  - complete_checkout()    - Process payment & create order  │
└──────────────────────────┬──────────────────────────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
┌───────▼──────┐  ┌────────▼────────┐  ┌──────▼───────┐
│ Cart Service │  │  Tax Service    │  │ Shipping     │
│              │  │                 │  │ Provider     │
└──────────────┘  └─────────────────┘  └──────────────┘
        │                  │                  │
┌───────▼──────┐  ┌────────▼────────┐  ┌──────▼───────┐
│ Order Service│  │ Payment Service │  │ Order        │
│              │  │                 │  │ Repository   │
└──────────────┘  └─────────────────┘  └──────────────┘
```

## Checkout Flow

### Step 1: Initiate Checkout

**Endpoint:** `POST /api/v1/checkout/initiate`

**Purpose:** 
- Validate the cart
- Calculate tax based on shipping address
- Fetch available shipping rates from providers
- Return checkout summary with totals

**Request:**
```json
{
  "cart_id": "550e8400-e29b-41d4-a716-446655440000",
  "shipping_address": {
    "first_name": "John",
    "last_name": "Doe",
    "address_line1": "123 Main St",
    "city": "New York",
    "state": "NY",
    "postal_code": "10001",
    "country": "US",
    "phone": "+1-555-1234"
  },
  "billing_address": {
    "first_name": "John",
    "last_name": "Doe",
    "address_line1": "123 Main St",
    "city": "New York",
    "state": "NY",
    "postal_code": "10001",
    "country": "US"
  },
  "vat_id": null,
  "currency": "USD"
}
```

**Response:**
```json
{
  "cart_id": "550e8400-e29b-41d4-a716-446655440000",
  "items": [
    {
      "id": "...",
      "product_id": "...",
      "variant_id": null,
      "title": "Premium T-Shirt",
      "sku": "TS-001",
      "quantity": 2,
      "unit_price": "49.99",
      "total": "99.98"
    }
  ],
  "subtotal": "99.98",
  "discount_total": "10.00",
  "shipping_total": "0.00",
  "shipping_tax": "0.00",
  "item_tax": "8.75",
  "tax_total": "8.75",
  "total": "98.73",
  "currency": "USD",
  "available_shipping_rates": [
    {
      "provider_id": "fedex",
      "carrier": "FedEx",
      "service_code": "ground",
      "service_name": "Ground",
      "rate": "12.50",
      "currency": "USD",
      "delivery_days": 5,
      "total_cost": "12.50"
    },
    {
      "provider_id": "ups",
      "carrier": "UPS",
      "service_code": "2day",
      "service_name": "2nd Day Air",
      "rate": "24.99",
      "currency": "USD",
      "delivery_days": 2,
      "total_cost": "24.99"
    }
  ],
  "selected_shipping_rate": null,
  "tax_breakdown": [
    {
      "tax_zone_name": "New York State",
      "tax_rate_name": "Sales Tax",
      "rate": "0.0875",
      "taxable_amount": "99.98",
      "tax_amount": "8.75"
    }
  ],
  "vat_id_valid": null
}
```

**Implementation:**
```rust
pub async fn initiate_checkout(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(request): Json<InitiateCheckoutApiRequest>,
) -> Result<Json<CheckoutSummaryResponse>, (StatusCode, Json<serde_json::Value>)> {
    let core_request = InitiateCheckoutRequest {
        cart_id: request.cart_id,
        shipping_address: request.shipping_address,
        billing_address: request.billing_address,
        vat_id: request.vat_id,
        customer_id: Some(auth.customer_id),
        currency: request.currency.and_then(|c| c.parse().ok()),
    };

    match state.checkout_service.initiate_checkout(core_request).await {
        Ok(summary) => {
            let response: CheckoutSummaryResponse = summary.into();
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to initiate checkout: {}", e);
            Err((StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))))
        }
    }
}
```

### Step 2: Select Shipping

**Endpoint:** `POST /api/v1/checkout/shipping`

**Purpose:**
- Update checkout with selected shipping method
- Recalculate totals including shipping costs
- Apply shipping tax if applicable

**Request:**
```json
{
  "cart_id": "550e8400-e29b-41d4-a716-446655440000",
  "shipping_rate": {
    "provider_id": "ups",
    "carrier": "UPS",
    "service_code": "2day",
    "service_name": "2nd Day Air",
    "rate": "24.99",
    "currency": "USD",
    "delivery_days": 2,
    "total_cost": "24.99"
  }
}
```

**Response:**
```json
{
  "cart_id": "550e8400-e29b-41d4-a716-446655440000",
  "items": [...],
  "subtotal": "99.98",
  "discount_total": "10.00",
  "shipping_total": "24.99",
  "shipping_tax": "2.19",
  "item_tax": "8.75",
  "tax_total": "10.94",
  "total": "125.91",
  "currency": "USD",
  "available_shipping_rates": [...],
  "selected_shipping_rate": {
    "provider_id": "ups",
    "carrier": "UPS",
    "service_code": "2day",
    "service_name": "2nd Day Air",
    "rate": "24.99",
    "currency": "USD",
    "delivery_days": 2,
    "total_cost": "24.99"
  },
  "tax_breakdown": [...],
  "vat_id_valid": null
}
```

**Implementation:**
```rust
pub async fn select_shipping(
    State(state): State<AppState>,
    Extension(_auth): Extension<JwtAuth>,
    Json(request): Json<SelectShippingApiRequest>,
) -> Result<Json<CheckoutSummaryResponse>, (StatusCode, Json<serde_json::Value>)> {
    let shipping_rate: ShippingRate = request.shipping_rate.into();
    
    let package = Package {
        weight: dec!(1.0),
        weight_unit: "kg".to_string(),
        length: Some(dec!(30.0)),
        width: Some(dec!(20.0)),
        height: Some(dec!(15.0)),
        dimension_unit: Some("cm".to_string()),
        predefined_package: None,
    };

    let core_request = SelectShippingRequest {
        cart_id: request.cart_id,
        shipping_rate,
        package,
    };

    match state.checkout_service.select_shipping(core_request).await {
        Ok(summary) => {
            let response: CheckoutSummaryResponse = summary.into();
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to select shipping: {}", e);
            Err((StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))))
        }
    }
}
```

### Step 3: Complete Checkout

**Endpoint:** `POST /api/v1/checkout/complete`

**Purpose:**
- Validate all checkout data
- Process payment through selected payment gateway
- Create order from cart
- Mark cart as converted
- Return order details

**Request:**
```json
{
  "cart_id": "550e8400-e29b-41d4-a716-446655440000",
  "shipping_address": { ... },
  "billing_address": { ... },
  "payment_method": {
    "type": "card",
    "token": "tok_visa_4242"
  },
  "customer_email": "john.doe@example.com",
  "vat_id": null,
  "notes": "Please leave at front door",
  "selected_shipping_rate": {
    "provider_id": "ups",
    "carrier": "UPS",
    "service_code": "2day",
    "service_name": "2nd Day Air",
    "rate": "24.99",
    "currency": "USD",
    "delivery_days": 2,
    "total_cost": "24.99"
  }
}
```

**Response:**
```json
{
  "order": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "order_number": "ORD-2026-000001",
    "customer_id": "550e8400-e29b-41d4-a716-446655440002",
    "customer_email": "john.doe@example.com",
    "status": "pending",
    "payment_status": "paid",
    "fulfillment_status": "pending",
    "currency": "USD",
    "subtotal": "99.98",
    "tax_total": "10.94",
    "shipping_total": "24.99",
    "discount_total": "10.00",
    "total": "125.91",
    "items": [...],
    "created_at": "2026-01-15T10:30:00Z",
    "metadata": {}
  },
  "payment_id": "pay_abc123xyz",
  "total_charged": "125.91",
  "currency": "USD"
}
```

**Implementation:**
```rust
pub async fn complete_checkout(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(request): Json<CompleteCheckoutApiRequest>,
) -> Result<(StatusCode, Json<CheckoutResultResponse>), (StatusCode, Json<serde_json::Value>)> {
    // Validate email
    if request.customer_email.is_empty() || !request.customer_email.contains('@') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid email address"})),
        ));
    }

    let core_request = CompleteCheckoutRequest {
        cart_id: request.cart_id,
        shipping_address: request.shipping_address,
        billing_address: request.billing_address,
        payment_method: request.payment_method.into(),
        customer_email: request.customer_email,
        customer_id: Some(auth.customer_id),
        vat_id: request.vat_id,
        notes: request.notes,
        selected_shipping_rate: request.selected_shipping_rate.into(),
    };

    match state.checkout_service.complete_checkout(core_request).await {
        Ok(result) => {
            let response: CheckoutResultResponse = result.into();
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(e) => {
            tracing::error!("Failed to complete checkout: {}", e);
            Err((StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))))
        }
    }
}
```

## Checkout Service

The CheckoutService orchestrates the checkout process:

```rust
pub struct CheckoutService {
    cart_service: Arc<CartService>,
    tax_service: Arc<dyn TaxService>,
    order_service: Arc<OrderService>,
    payment_service: Arc<PaymentService>,
    shipping_factory: Arc<ShippingProviderFactory>,
    config: CheckoutConfig,
}
```

### Initiate Checkout

```rust
pub async fn initiate_checkout(
    &self,
    request: InitiateCheckoutRequest,
) -> Result<CheckoutSummary> {
    // 1. Get cart with items
    let cart_with_items = self.cart_service.get_cart_with_items(request.cart_id).await?;
    
    // 2. Calculate tax
    let tax_calculation = self.calculate_tax(&cart_with_items, &request.shipping_address, request.vat_id.as_deref()).await?;
    
    // 3. Get shipping rates
    let shipping_rates = self.get_shipping_rates(&cart_with_items, &request.shipping_address).await?;
    
    // 4. Build checkout summary
    let summary = CheckoutSummary {
        cart_id: request.cart_id,
        items: cart_with_items.items.into_iter().map(|i| i.into()).collect(),
        subtotal: cart_with_items.cart.subtotal,
        discount_total: cart_with_items.cart.discount_total,
        shipping_total: Decimal::ZERO,
        shipping_tax: Decimal::ZERO,
        item_tax: tax_calculation.total_tax,
        tax_total: tax_calculation.total_tax,
        total: cart_with_items.cart.subtotal - cart_with_items.cart.discount_total + tax_calculation.total_tax,
        currency: cart_with_items.cart.currency,
        available_shipping_rates: shipping_rates,
        selected_shipping_rate: None,
        tax_breakdown: tax_calculation.breakdown,
        vat_id_valid: request.vat_id.map(|_| true), // Simplified - actual validation needed
    };
    
    Ok(summary)
}
```

### Complete Checkout

```rust
pub async fn complete_checkout(
    &self,
    request: CompleteCheckoutRequest,
) -> Result<CheckoutResult> {
    // 1. Validate cart
    let cart_with_items = self.cart_service.get_cart_with_items(request.cart_id).await?;
    
    // 2. Calculate final totals with selected shipping
    let total = self.calculate_total(&cart_with_items, &request.selected_shipping_rate).await?;
    
    // 3. Process payment
    let payment_result = self.process_payment(
        total,
        &request.payment_method,
        &request.customer_email,
    ).await?;
    
    // 4. Create order
    let order = self.create_order_from_cart(
        &cart_with_items,
        &request,
        &payment_result,
    ).await?;
    
    // 5. Mark cart as converted
    self.cart_service.mark_as_converted(request.cart_id, order.id).await?;
    
    Ok(CheckoutResult {
        order,
        payment_id: payment_result.id,
        total_charged: total,
        currency: cart_with_items.cart.currency,
    })
}
```

## Payment Methods

The checkout system supports multiple payment methods:

```rust
pub enum PaymentMethod {
    Card {
        token: String,
        last4: Option<String>,
        brand: Option<String>,
        exp_month: Option<u8>,
        exp_year: Option<u16>,
    },
    BankTransfer {
        account_number: String,
        routing_number: String,
        bank_name: Option<String>,
    },
    DigitalWallet {
        provider: String,  // "apple_pay", "google_pay"
        token: Option<String>,
    },
    BuyNowPayLater {
        provider: String,  // "klarna", "afterpay"
    },
    CashOnDelivery,
}
```

## API Endpoints Summary

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | /api/v1/checkout/initiate | JWT | Start checkout, get tax & shipping rates |
| POST | /api/v1/checkout/shipping | JWT | Select shipping method |
| POST | /api/v1/checkout/complete | JWT | Process payment & create order |

## Authentication

All checkout endpoints require JWT authentication. The `JwtAuth` context is extracted by the auth middleware:

```rust
pub async fn initiate_checkout(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,  // Extracted by middleware
    Json(request): Json<InitiateCheckoutApiRequest>,
) -> Result<...> {
    // auth.customer_id contains the authenticated customer's ID
    // auth.email contains the customer's email
    // auth.permissions contains the customer's permissions
}
```

## Error Handling

| Error | Code | Description |
|-------|------|-------------|
| Cart not found | 404 | Cart ID does not exist |
| Cart empty | 422 | Cannot checkout with empty cart |
| Invalid shipping address | 422 | Address validation failed |
| Invalid payment method | 422 | Payment method not supported |
| Payment failed | 402 | Payment processing failed |
| Shipping unavailable | 422 | No shipping rates available for address |

## Webhook Events

The checkout system emits the following webhook events:

| Event | Description |
|-------|-------------|
| `checkout.initiated` | Checkout started |
| `checkout.shipping_selected` | Shipping method selected |
| `checkout.completed` | Checkout completed successfully |
| `checkout.failed` | Checkout failed (payment or other) |
| `order.created` | Order created from checkout |
| `payment.succeeded` | Payment processed successfully |
| `payment.failed` | Payment processing failed |

## Integration with Other Services

### Tax Service Integration

The checkout service uses the TaxService to calculate taxes:

```rust
let tax_context = TaxContext {
    transaction_type: TransactionType::Sale,
    destination: TaxAddress::from(&shipping_address),
    origin: self.config.tax_origin_address.clone(),
    customer: Some(CustomerTaxInfo {
        customer_id: request.customer_id,
        tax_exempt: false,
        vat_id: request.vat_id.map(|v| VatId::new(v, "EU")),
    }),
};

let tax_calculation = self.tax_service.calculate_tax(&tax_context, &taxable_items).await?;
```

### Shipping Integration

Shipping rates are fetched from configured providers:

```rust
let shipment = Shipment {
    from: self.config.shipping_origin_address.clone(),
    to: ShippingAddress::from(&shipping_address),
    packages: vec![self.estimate_package(&cart_with_items)],
    items: cart_with_items.items.iter().map(|i| i.into()).collect(),
};

let rates = provider.calculate_rates(&shipment).await?;
```

### Payment Integration

Payments are processed through the PaymentService:

```rust
let payment_request = PaymentRequest {
    amount: total,
    currency: cart.currency,
    payment_method: request.payment_method,
    description: format!("Order for cart {}", cart.id),
    customer_email: Some(request.customer_email.clone()),
    customer_id: request.customer_id,
    metadata: json!({"cart_id": cart.id}),
};

let payment_result = self.payment_service.process_payment(payment_request).await?;
```

## Configuration

Checkout behavior can be configured via `config.toml`:

```toml
[checkout]
# Default currency
default_currency = "USD"

# Tax calculation origin address
[tax.origin]
country = "US"
state = "CA"
postal_code = "90210"
city = "Los Angeles"

# Shipping origin address
[shipping.origin]
country = "US"
state = "CA"
postal_code = "90210"
city = "Los Angeles"

# Available payment gateways
[payment]
default_gateway = "stripe"

[payment.stripe]
enabled = true
secret_key = "sk_test_..."
webhook_secret = "whsec_..."
```

## Best Practices

1. **Validate Early**: Validate all input data before processing
2. **Idempotency**: Use idempotency keys for payment processing
3. **Error Messages**: Provide clear error messages for checkout failures
4. **Inventory Check**: Verify inventory availability before completing checkout
5. **Cart Locking**: Consider locking carts during checkout to prevent modifications
6. **Payment Retry**: Allow customers to retry payment without re-entering all data
7. **Address Validation**: Validate and normalize addresses for accurate tax/shipping
