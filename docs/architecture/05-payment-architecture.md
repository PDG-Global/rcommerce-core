# Payment Integration Architecture

## Overview

The payment system provides a unified API for processing payments across multiple gateways while each gateway handles its own secure payment collection. This enables merchants to accept payments through various methods (credit cards, bank transfers, digital wallets, etc.) while maintaining a consistent integration experience.

**Key Design Principle:** Sensitive payment data never touches your server. The frontend uses gateway-provided secure payment UIs (Stripe Elements, Airwallex Hosted Pages) to collect and tokenize payment details. R Commerce creates payment sessions and handles webhooks for payment confirmation.

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

1. **Security**: Sensitive payment data (card numbers) never touches your server
2. **PCI Compliance**: Using gateway's secure UIs achieves SAQ A compliance
3. **No API Keys in Frontend**: Secret keys remain server-side only
4. **Unified Backend API**: Same API regardless of which gateway is used
5. **Simplified Frontend**: Integration handled by gateway SDKs

## Security

- **Card Data**: Never touches your server - goes directly to payment gateway
- **PCI Compliance**: Using Stripe Elements or Airwallex Hosted Pages achieves SAQ A compliance
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

### Create Payment Session

```http
POST /api/v1/payments
{
  "gateway_id": "stripe",
  "amount": "99.99",
  "currency": "USD",
  "order_id": "order_123",
  "customer_email": "customer@example.com",
  "return_url": "https://yoursite.com/checkout/complete"
}
```

**Response (Stripe Elements):**
```json
{
  "client_secret": "pi_xxx_secret_xxx",
  "payment_id": "pay_xxx",
  "gateway": "stripe"
}
```

**Response (Airwallex Hosted):**
```json
{
  "redirect_url": "https://airwallex.com/checkout/xxx",
  "payment_id": "pay_xxx",
  "gateway": "airwallex"
}
```

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

### Stripe Configuration

```toml
[payment.stripe]
enabled = true
secret_key = "${STRIPE_SECRET_KEY}"
publishable_key = "${STRIPE_PUBLISHABLE_KEY}"
webhook_secret = "${STRIPE_WEBHOOK_SECRET}"
```

### Airwallex Configuration

```toml
[payment.airwallex]
enabled = true
api_key = "${AIRWALLEX_API_KEY}"
client_id = "${AIRWALLEX_CLIENT_ID}"
webhook_secret = "${AIRWALLEX_WEBHOOK_SECRET}"
demo = true  # For testing
```

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
