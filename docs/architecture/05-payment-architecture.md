# Payment Integration Architecture

## Overview

The payment system provides a unified API for processing payments across multiple gateways while each gateway handles its own secure payment collection. This enables merchants to accept payments through various methods (credit cards, bank transfers, digital wallets, etc.) while maintaining a consistent integration experience.

**Key Design Principle:** Checkout can use gateway-provided secure payment UIs (Stripe Elements, Airwallex Hosted Pages) so card data bypasses R Commerce. The direct payment API also accepts payment-method data and forwards it to the selected gateway; raw card data is not persisted locally.

**Supported Gateways:**
- Stripe (Global cards and digital wallets)
- WeChat Pay (China's leading mobile payment)
- AliPay (Alibaba's global payment platform)
- Airwallex (Multi-currency/global)
- Braintree (PayPal-owned)
- PayPal
- Manual/Bank Transfer

## Payment Flow Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Frontend (Browser/App)                           │
│  - Initialize payment with R Commerce API                              │
│  - Collect payment details via Gateway UI (Elements/Hosted Page)      │
│  - Handle redirect/completion                                         │
└───────────────────────────┬─────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      R Commerce API (Axum Server)                        │
│  - Create payment session                                             │
│  - Return client_secret or redirect_url                               │
│  - Handle webhooks for payment confirmation                           │
│  - Update order status                                               │
└───────────────────────────┬─────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    Payment Gateway (External)                          │
│  - Stripe Elements / Airwallex Hosted Page                            │
│  - Tokenize and process payment                                       │
│  - Send webhook notifications                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Integration Flow

1. **Frontend** requests payment session from R Commerce API
2. **R Commerce** creates payment intent with gateway, returns `client_secret` (for Stripe Elements) or `redirect_url` (for hosted pages)
3. **Frontend** uses Stripe Elements or redirects to Airwallex Hosted Payment Page
4. **Gateway** processes payment and notifies R Commerce via webhook
5. **R Commerce** updates order status based on webhook events

## Benefits

1. **Hosted UI option**: Gateway Elements/Hosted Pages can keep card data off the R Commerce server
2. **PCI scope**: Gateway-hosted collection can reduce scope; assess the actual integration
3. **No API Keys in Frontend**: Secret keys remain server-side only
4. **Unified Backend API**: Same API regardless of which gateway is used
5. **Simplified Frontend**: Integration handled by gateway SDKs

## Security

- **Card Data**: Raw card data is not persisted locally; direct API payment data passes through R Commerce to the gateway
- **API Keys**: Secret keys stored server-side only
- **Webhook Verification**: All webhook signatures are verified
- **HTTPS**: Required in production

## Gateway Integration Types

### Stripe Elements (Embedded Form)

Stripe Elements provides an embedded, customizable payment UI that handles card tokenization securely. The card data goes directly from the browser to Stripe, never touching your server.

```javascript
// Frontend: Get client secret from R Commerce
const { client_secret } = await fetch('/api/v1/payments', {...}).then(r => r.json());

// Frontend: Use Stripe Elements
const stripe = await loadStripe('pk_test_xxx');
const elements = stripe.elements({ clientSecret: client_secret });
const paymentElement = elements.create('payment');
paymentElement.mount('#payment-element');

await stripe.confirmPayment({ elements, confirmParams: {...} });
```

### Airwallex Hosted Payment Page

For Airwallex, customers are redirected to Airwallex's hosted payment page to complete payment securely.

```javascript
// Frontend: Get redirect URL from R Commerce
const { redirect_url } = await fetch('/api/v1/payments', {...}).then(r => r.json());

// Frontend: Redirect customer
window.location.href = redirect_url;
```

## API Integration

### Direct Payment API

All `/payments` and `/payment-methods` endpoints require JWT. Customers can
operate only on their own orders, payments, and saved methods; foreign records
are treated as not found. Refunds require an admin JWT and use the gateway
stored on the payment.

```http
POST /api/v1/payments
Authorization: Bearer <customer_jwt>
Content-Type: application/json

{
  "gateway_id": "stripe",
  "amount": "99.99",
  "currency": "USD",
  "payment_method_type": "card",
  "order_id": "550e8400-e29b-41d4-a716-446655440000",
  "customer_email": "customer@example.com",
  "payment_method_data": {
    "type": "card",
    "number": "4242424242424242",
    "exp_month": "12",
    "exp_year": "2030",
    "cvc": "123",
    "name": "Test Customer"
  },
  "save_payment_method": false,
  "description": "Order payment"
}
```

The server validates amount and currency against the authoritative order and
persists the provider intent ID and status on initiation, completion, and
status refresh. Completion and refunds use the stored gateway, with no
implicit fallback. Amounts use major currency units; Airwallex payment and
refund payloads also use major units rather than cents.

`save_payment_method = true` is rejected on direct initiation. Use
`POST /api/v1/payment-methods` to save, then list or delete through the saved
method endpoints. The server maintains a provider-customer mapping per local
customer and gateway and stores only a local SHA-256 token hash for ownership
checks. Airwallex saved-card charges require a CVC supplied in
`payment_method_data.cvc`; it is never stored or logged. Refund reconciliation
uses cumulative settled refund amounts so partial refunds do not mark the whole
order refunded or restock every item.

### Webhook Handling

All payment confirmations come through webhooks:

```http
POST /api/v1/webhooks/{gateway}
```

Your server should:
1. Verify the webhook signature
2. Update the order/payment status
3. Send confirmation emails
4. Trigger fulfillment

## Configuration

```toml
[payment]
default_gateway = "stripe"  # checkout: mock (test only), stripe, or airwallex
test_mode = false           # production

[payment.stripe]
enabled = true
secret_key = "${STRIPE_SECRET_KEY}"
publishable_key = "${STRIPE_PUBLISHABLE_KEY}"
webhook_secret = "${STRIPE_WEBHOOK_SECRET}"

[payment.airwallex]
enabled = false
api_key = "${AIRWALLEX_API_KEY}"
client_id = "${AIRWALLEX_CLIENT_ID}"
webhook_secret = "${AIRWALLEX_WEBHOOK_SECRET}"
demo = false
```

Mock is registered only when `test_mode = true`. The selected real gateway must
be enabled with its required credentials. Unsupported, disabled, or incomplete
selections fail at startup without fallback; production uses
`test_mode = false`.

## Error Handling

The API returns appropriate error types:

```json
{
  "error": {
    "category": "payment",
    "code": "invalid_amount",
    "message": "Amount must be greater than 0.50"
  }
}
```

Common error codes:
- `invalid_amount`: Amount is below minimum
- `currency_not_supported`: Currency not supported by gateway
- `payment_failed`: Gateway rejected the payment
- `gateway_unavailable`: Gateway is down
