# Payment Gateway Testing Guide

This guide covers testing the payment gateway integrations for the provider-agnostic payment system.

## Overview

R Commerce supports multiple payment gateways through a unified, server-side processing API:
- **Stripe** - Global card payments and digital wallets
- **Airwallex** - Multi-currency global payments
- **WeChat Pay** - China's leading mobile payment
- **AliPay** - Alibaba's global payment platform
- **Mock Gateway** - For development and testing

## Architecture Overview

The new agnostic payment system processes all payments server-side:

```
Frontend → R Commerce API → Payment Gateway → Provider API
                ↑
         Unified Interface
    (AgnosticPaymentGateway trait)
```

This provides:
- **Better security**: No API keys in JavaScript
- **Unified interface**: Same code works for all gateways
- **Simpler frontend**: No provider SDKs required

## Prerequisites

### Test API Keys

#### Stripe Test Credentials
1. Create a Stripe account at https://stripe.com
2. Go to Developers → API Keys
3. Copy your **Test Secret Key** (starts with `sk_test_`)
4. Set up a **Webhook Endpoint** and copy the signing secret (starts with `whsec_`)

```bash
export STRIPE_TEST_SECRET_KEY="sk_test_..."
export STRIPE_TEST_WEBHOOK_SECRET="whsec_..."
```

**Note:** You only need the **Secret Key**, not the Publishable Key, since all processing happens server-side.

#### Airwallex Test Credentials
1. Create an Airwallex account at https://www.airwallex.com
2. Go to Developer → API Keys
3. Create a new API key pair
4. Copy your **Client ID** and **API Key**
5. Set up a **Webhook Endpoint** and copy the webhook secret

```bash
export AIRWALLEX_TEST_CLIENT_ID="your_client_id"
export AIRWALLEX_TEST_API_KEY="your_api_key"
export AIRWALLEX_TEST_WEBHOOK_SECRET="your_webhook_secret"
```

## Running Tests

### Quick Test (Mock Gateway)

```bash
cd /path/to/rcommerce
cargo test -p rcommerce-core --lib payment::gateways::tests
```

### Stripe Integration Tests

```bash
export STRIPE_TEST_SECRET_KEY="sk_test_..."
export STRIPE_TEST_WEBHOOK_SECRET="whsec_..."

cargo test -p rcommerce-core --lib payment::gateways::integration_tests::stripe_tests -- --test-threads=1
```

### Airwallex Integration Tests

```bash
export AIRWALLEX_TEST_CLIENT_ID="..."
export AIRWALLEX_TEST_API_KEY="..."
export AIRWALLEX_TEST_WEBHOOK_SECRET="..."

cargo test -p rcommerce-core --lib payment::gateways::integration_tests::airwallex_tests -- --test-threads=1
```

### All Payment Tests

```bash
cargo test -p rcommerce-core --lib payment::gateways -- --test-threads=1
```

## Test Scenarios

For direct-payment examples, replace the order UUID with an order owned by the
JWT customer whose authoritative total and currency match the request. Amounts
are major-unit strings. Direct initiation requires `save_payment_method = false`;
use the dedicated payment-methods endpoint to save a card.

### 1. Initiate Payment (Success)

Tests that a payment can be initiated successfully through the agnostic API.

**Request:**
```http
POST /api/v1/payments
Content-Type: application/json
Authorization: Bearer <customer_jwt>

{
  "gateway_id": "stripe",
  "amount": "99.99",
  "currency": "USD",
  "payment_method_type": "card",
  "order_id": "550e8400-e29b-41d4-a716-446655440000",
  "customer_email": "test@example.com",
  "payment_method_data": {
    "type": "card",
    "number": "4242424242424242",
    "exp_month": "12",
    "exp_year": "2030",
    "cvc": "123",
    "name": "Test User"
  },
  "save_payment_method": false,
  "description": "Test order payment"
}
```

**Expected Result:**
- `result: "success"` response
- Payment ID returned
- Transaction ID from provider
- Receipt URL (if available)

### 2. Initiate Payment with 3D Secure

Tests 3D Secure handling for cards that require authentication.

**Request:**
Same as above, but with card number `4000002500003155`

**Expected Result:**
- `result: "requires_action"` response
- `action_type: "three_d_secure"`
- `action_data` containing redirect URL or 3DS parameters
- `expires_at` timestamp

### 3. Complete Payment Action

Tests completing a payment after 3D Secure or redirect.

**Request:**
```http
POST /api/v1/payments/pay_xxx/complete
Content-Type: application/json
Authorization: Bearer <customer_jwt>

{
  "action_type": "three_d_secure",
  "action_data": {
    "payment_intent_client_secret": "pi_xxx_secret_xxx"
  }
}
```

**Expected Result:**
- `result: "success"` response
- Payment status updated to `succeeded`

### 4. Get Payment Status

Tests retrieving payment status.

**Request:**
```http
GET /api/v1/payments/pay_xxx
Authorization: Bearer <token>
```

**Expected Result:**
- Payment details including status
- Transaction ID
- Amount and currency
- Payment method info (last 4 digits, brand)

### 5. Refund Payment

Tests refunding a captured payment through the gateway stored on the payment.
Refunds require an admin JWT.

**Request:**
```http
POST /api/v1/payments/pay_xxx/refund
Content-Type: application/json
Authorization: Bearer <admin_jwt>

{
  "amount": "99.99",
  "reason": "requested_by_customer"
}
```

**Expected Result:**
- Refund ID returned
- Refund status
- Original payment updated with refund info

### 6. Get Available Payment Methods

Tests retrieving available payment methods for a checkout.

**Request:**
```http
POST /api/v1/payments/methods
Content-Type: application/json
Authorization: Bearer <token>

{
  "currency": "USD",
  "amount": "99.99"
}
```

**Expected Result:**
- List of gateways with their supported methods
- Required fields for each method
- 3DS support flags

## Manual Testing with cURL

### Get Payment Methods

```bash
curl -X POST http://localhost:8080/api/v1/payments/methods \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_CUSTOMER_JWT" \
  -d '{
    "currency": "USD",
    "amount": "99.99"
  }'
```

### Initiate Payment (Success)

```bash
curl -X POST http://localhost:8080/api/v1/payments \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_CUSTOMER_JWT" \
  -d '{
    "gateway_id": "stripe",
    "amount": "99.99",
    "currency": "USD",
    "payment_method_type": "card",
    "order_id": "550e8400-e29b-41d4-a716-446655440000",
    "customer_email": "test@example.com",
    "payment_method_data": {
      "type": "card",
      "number": "4242424242424242",
      "exp_month": "12",
      "exp_year": "2030",
      "cvc": "123",
      "name": "Test User"
    },
    "save_payment_method": false,
    "description": "Test order payment"
  }'
```

### Initiate Payment (3D Secure)

```bash
curl -X POST http://localhost:8080/api/v1/payments \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_CUSTOMER_JWT" \
  -d '{
    "gateway_id": "stripe",
    "amount": "99.99",
    "currency": "USD",
    "payment_method_type": "card",
    "order_id": "550e8400-e29b-41d4-a716-446655440000",
    "customer_email": "test@example.com",
    "payment_method_data": {
      "type": "card",
      "number": "4000002500003155",
      "exp_month": "12",
      "exp_year": "2030",
      "cvc": "123",
      "name": "Test User"
    },
    "save_payment_method": false,
    "description": "Test order payment with 3DS"
  }'
```

Expected response:
```json
{
  "result": "requires_action",
  "payment_id": "pay_xxx",
  "action_type": "three_d_secure",
  "action_data": {
    "redirect_url": "https://hooks.stripe.com/3d_secure/..."
  },
  "expires_at": "2026-01-28T11:00:00Z"
}
```

### Complete Payment Action

```bash
curl -X POST http://localhost:8080/api/v1/payments/pay_xxx/complete \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_CUSTOMER_JWT" \
  -d '{
    "action_type": "three_d_secure",
    "action_data": {
      "payment_intent": "pi_xxx"
    }
  }'
```

### Get Payment Status

```bash
curl http://localhost:8080/api/v1/payments/pay_xxx \
  -H "Authorization: Bearer YOUR_CUSTOMER_JWT"
```

### Refund Payment

Refunds require an admin JWT and use the gateway stored on the payment.

```bash
curl -X POST http://localhost:8080/api/v1/payments/pay_xxx/refund \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_ADMIN_JWT" \
  -d '{
    "amount": "99.99",
    "reason": "requested_by_customer"
  }'
```

## Configuration

Add to your `config.toml`:

```toml
[payment]
default_gateway = "mock"  # use "stripe" or "airwallex" to test those checkout flows
test_mode = true          # required to register mock; use false in production

[payment.stripe]
enabled = true
secret_key = "${STRIPE_SECRET_KEY}"
webhook_secret = "${STRIPE_WEBHOOK_SECRET}"

[payment.airwallex]
enabled = true
client_id = "${AIRWALLEX_CLIENT_ID}"
api_key = "${AIRWALLEX_API_KEY}"
webhook_secret = "${AIRWALLEX_WEBHOOK_SECRET}"
```

Mock is not registered when `test_mode = false`. Checkout accepts only `mock`,
`stripe`, or `airwallex` as its default. A selected real gateway must be enabled
with credentials; startup fails rather than falling back to another gateway.
Payment and refund amounts are major-unit decimals, including for Airwallex.

## Webhook Testing

### Local Webhook Testing with Stripe CLI

```bash
# Install Stripe CLI
brew install stripe/stripe-cli/stripe

# Login
stripe login

# Forward webhooks to local server
stripe listen --forward-to localhost:8080/api/v1/webhooks/stripe
```

### Trigger Test Events

```bash
# Trigger payment success
stripe trigger payment_intent.succeeded

# Trigger payment failure
stripe trigger payment_intent.payment_failed
```

## Test Cards

### Stripe Test Cards

| Card Number | Scenario |
|-------------|----------|
| `4242424242424242` | Success |
| `4000000000000002` | Card declined |
| `4000000000009995` | Insufficient funds |
| `4000002500003155` | Requires 3D Secure |
| `4000000000003220` | 3D Secure 2 frictionless |
| `4000008400001629` | 3D Secure 2 challenge |
| `4000000000000127` | Incorrect CVC |
| `4000000000000069` | Expired card |

## Frontend Testing Example

Use an order owned by the JWT customer whose total is exactly `99.99 USD`:

```javascript
// test-payment.js
const ORDER_ID = '550e8400-e29b-41d4-a716-446655440000';

async function initiatePayment(number) {
  const response = await fetch('http://localhost:8080/api/v1/payments', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer YOUR_CUSTOMER_JWT'
    },
    body: JSON.stringify({
      gateway_id: 'stripe',
      amount: '99.99',
      currency: 'USD',
      payment_method_type: 'card',
      order_id: ORDER_ID,
      customer_email: 'test@example.com',
      payment_method_data: {
        type: 'card',
        number,
        exp_month: '12',
        exp_year: '2030',
        cvc: '123',
        name: 'Test User'
      },
      save_payment_method: false,
      description: 'Test order payment'
    })
  });
  return await response.json();
}

async function testPayment() {
  const success = await initiatePayment('4242424242424242');
  console.assert(success.result === 'success', 'Simple payment should succeed');

  const challenge = await initiatePayment('4000002500003155');
  console.assert(challenge.result === 'requires_action', '3DS should require action');
  console.assert(challenge.action_type === 'three_d_secure', 'Action type should be 3DS');

  const declined = await initiatePayment('4000000000000002');
  console.assert(declined.result === 'failed', 'Declined card should fail');
}

testPayment().catch(console.error);
```

## Troubleshooting

### Common Issues

**Authentication Errors:**
- Verify API keys are correct
- Check that you're using test keys (not production)
- Ensure environment variables are set

**Network Errors:**
- Check internet connectivity
- Verify firewall settings
- Some corporate networks block payment APIs

**Webhook Errors:**
- Verify webhook secret is correct
- Check payload is not modified
- Ensure correct signature algorithm

**3D Secure Issues:**
- Ensure `return_url` is provided in payment request
- Check that redirect URL is properly handled
- Verify completion endpoint is called after redirect

### Debug Logging

Enable debug logging:

```bash
RUST_LOG=debug cargo test -p rcommerce-core --lib payment::gateways
```

For API debugging:

```bash
RUST_LOG=rcommerce_api=debug,sqlx=warn cargo run
```

## Production Checklist

Before going live:

- [ ] Set `payment.test_mode = false`
- [ ] Switch to production API keys and enable the selected real gateway with its required credentials
- [ ] Configure production webhook endpoints
- [ ] Enable 3D Secure for card payments
- [ ] Set up webhook signature verification
- [ ] Configure fraud rules
- [ ] Test all payment flows end-to-end
- [ ] Test 3D Secure flow with real cards
- [ ] Test refund flow with an admin JWT
- [ ] Set up monitoring and alerts
- [ ] Configure any additional gateway explicitly; there is no automatic fallback
- [ ] Test webhook handling in production
- [ ] Verify PCI compliance requirements

## Security Notes

1. **Never commit API keys** - Use environment variables
2. **Verify webhooks** - Always verify signatures
3. **Use HTTPS** - Never send payment data over HTTP
4. **PCI Compliance** - Don't store raw card data (R Commerce handles this)
5. **Rate limiting** - Implement rate limits on payment endpoints
6. **Idempotency** - Use idempotency keys for retries
7. **Authorization** - Payment endpoints require JWT; customers are restricted to their own orders/payments and refunds are admin-only
8. **Saved methods** - Use `POST /payment-methods`, not `save_payment_method=true` during direct initiation; provider-customer mappings and local SHA-256 token hashes enforce ownership

## Additional Resources

- [Stripe Testing Documentation](https://stripe.com/docs/testing)
- [Stripe Test Card Numbers](https://stripe.com/docs/testing#cards)
- [Airwallex API Documentation](https://www.airwallex.com/docs/api)
- [WeChat Pay Documentation](https://pay.weixin.qq.com/wiki/doc/apiv3/index.shtml)
- [Alipay Documentation](https://opendocs.alipay.com/)
