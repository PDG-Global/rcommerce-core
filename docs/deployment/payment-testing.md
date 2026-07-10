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
cd /Users/jeremy/Development/gokart
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

### 1. Initiate Payment (Success)

Tests that a payment can be initiated successfully through the agnostic API.

**Request:**
```http
POST /api/v2/payments
Content-Type: application/json
Authorization: Bearer <token>

{
  "gateway_id": "stripe",
  "amount": "99.99",
  "currency": "USD",
  "payment_method": {
    "type": "card",
    "card": {
      "number": "4242424242424242",
      "exp_month": 12,
      "exp_year": 2025,
      "cvc": "123",
      "name": "Test User"
    }
  },
  "order_id": "order_123",
  "customer_email": "test@example.com"
}
```

**Expected Result:**
- `type: "success"` response
- Payment ID returned
- Transaction ID from provider
- Receipt URL (if available)

### 2. Initiate Payment with 3D Secure

Tests 3D Secure handling for cards that require authentication.

**Request:**
Same as above, but with card number `4000002500003155`

**Expected Result:**
- `type: "requires_action"` response
- `action_type: "three_d_secure"`
- `action_data` containing redirect URL or 3DS parameters
- `expires_at` timestamp

### 3. Complete Payment Action

Tests completing a payment after 3D Secure or redirect.

**Request:**
```http
POST /api/v2/payments/pay_xxx/complete
Content-Type: application/json
Authorization: Bearer <token>

{
  "action_type": "three_d_secure",
  "action_data": {
    "payment_intent_client_secret": "pi_xxx_secret_xxx"
  }
}
```

**Expected Result:**
- `type: "success"` response
- Payment status updated to `succeeded`

### 4. Get Payment Status

Tests retrieving payment status.

**Request:**
```http
GET /api/v2/payments/pay_xxx
Authorization: Bearer <token>
```

**Expected Result:**
- Payment details including status
- Transaction ID
- Amount and currency
- Payment method info (last 4 digits, brand)

### 5. Refund Payment

Tests refunding a captured payment.

**Request:**
```http
POST /api/v2/payments/pay_xxx/refund
Content-Type: application/json
Authorization: Bearer <token>

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
POST /api/v2/payments/methods
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
curl -X POST http://localhost:8080/api/v2/payments/methods \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "currency": "USD",
    "amount": "99.99"
  }'
```

### Initiate Payment (Success)

```bash
curl -X POST http://localhost:8080/api/v2/payments \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "gateway_id": "stripe",
    "amount": "99.99",
    "currency": "USD",
    "payment_method": {
      "type": "card",
      "card": {
        "number": "4242424242424242",
        "exp_month": 12,
        "exp_year": 2025,
        "cvc": "123",
        "name": "Test User"
      }
    },
    "order_id": "order_123",
    "customer_email": "test@example.com",
    "return_url": "https://yoursite.com/checkout/complete"
  }'
```

### Initiate Payment (3D Secure)

```bash
curl -X POST http://localhost:8080/api/v2/payments \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "gateway_id": "stripe",
    "amount": "99.99",
    "currency": "USD",
    "payment_method": {
      "type": "card",
      "card": {
        "number": "4000002500003155",
        "exp_month": 12,
        "exp_year": 2025,
        "cvc": "123",
        "name": "Test User"
      }
    },
    "order_id": "order_123",
    "return_url": "https://yoursite.com/checkout/complete"
  }'
```

Expected response:
```json
{
  "type": "requires_action",
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
curl -X POST http://localhost:8080/api/v2/payments/pay_xxx/complete \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "action_type": "three_d_secure",
    "action_data": {
      "payment_intent": "pi_xxx"
    }
  }'
```

### Get Payment Status

```bash
curl http://localhost:8080/api/v2/payments/pay_xxx \
  -H "Authorization: Bearer YOUR_API_KEY"
```

### Refund Payment

```bash
curl -X POST http://localhost:8080/api/v2/payments/pay_xxx/refund \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "amount": "99.99",
    "reason": "requested_by_customer"
  }'
```

## Configuration

Add to your `config.toml`:

```toml
[payment]
default_gateway = "stripe"

[payment.stripe]
enabled = true
api_key = "${STRIPE_SECRET_KEY}"
webhook_secret = "${STRIPE_WEBHOOK_SECRET}"

[payment.airwallex]
enabled = true
client_id = "${AIRWALLEX_CLIENT_ID}"
api_key = "${AIRWALLEX_API_KEY}"
webhook_secret = "${AIRWALLEX_WEBHOOK_SECRET}"
```

## Webhook Testing

### Local Webhook Testing with Stripe CLI

```bash
# Install Stripe CLI
brew install stripe/stripe-cli/stripe

# Login
stripe login

# Forward webhooks to local server
stripe listen --forward-to localhost:8080/api/v2/webhooks/stripe
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

```javascript
// test-payment.js

async function testPayment() {
  // Test 1: Simple payment
  console.log('Test 1: Simple payment');
  const result1 = await initiatePayment({
    gateway_id: 'stripe',
    amount: '49.99',
    card: { number: '4242424242424242', exp_month: 12, exp_year: 2025, cvc: '123' }
  });
  console.assert(result1.type === 'success', 'Simple payment should succeed');
  
  // Test 2: 3D Secure payment
  console.log('Test 2: 3D Secure payment');
  const result2 = await initiatePayment({
    gateway_id: 'stripe',
    amount: '99.99',
    card: { number: '4000002500003155', exp_month: 12, exp_year: 2025, cvc: '123' }
  });
  console.assert(result2.type === 'requires_action', '3DS card should require action');
  console.assert(result2.action_type === 'three_d_secure', 'Action type should be 3DS');
  
  // Test 3: Declined card
  console.log('Test 3: Declined card');
  const result3 = await initiatePayment({
    gateway_id: 'stripe',
    amount: '99.99',
    card: { number: '4000000000000002', exp_month: 12, exp_year: 2025, cvc: '123' }
  });
  console.assert(result3.type === 'failed', 'Declined card should fail');
  
  console.log('All tests passed!');
}

async function initiatePayment({ gateway_id, amount, card }) {
  const response = await fetch('http://localhost:8080/api/v2/payments', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer YOUR_TEST_API_KEY'
    },
    body: JSON.stringify({
      gateway_id,
      amount,
      currency: 'USD',
      payment_method: { type: 'card', card },
      order_id: `test_order_${Date.now()}`,
      customer_email: 'test@example.com'
    })
  });
  return await response.json();
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

- [ ] Switch to production API keys
- [ ] Configure production webhook endpoints
- [ ] Enable 3D Secure for card payments
- [ ] Set up webhook signature verification
- [ ] Configure fraud rules
- [ ] Test all payment flows end-to-end
- [ ] Test 3D Secure flow with real cards
- [ ] Test refund flow
- [ ] Set up monitoring and alerts
- [ ] Configure backup payment gateway
- [ ] Test webhook handling in production
- [ ] Verify PCI compliance requirements

## Security Notes

1. **Never commit API keys** - Use environment variables
2. **Verify webhooks** - Always verify signatures
3. **Use HTTPS** - Never send payment data over HTTP
4. **PCI Compliance** - Don't store raw card data (R Commerce handles this)
5. **Rate limiting** - Implement rate limits on payment endpoints
6. **Idempotency** - Use idempotency keys for retries

## Additional Resources

- [Stripe Testing Documentation](https://stripe.com/docs/testing)
- [Stripe Test Card Numbers](https://stripe.com/docs/testing#cards)
- [Airwallex API Documentation](https://www.airwallex.com/docs/api)
- [WeChat Pay Documentation](https://pay.weixin.qq.com/wiki/doc/apiv3/index.shtml)
- [Alipay Documentation](https://opendocs.alipay.com/)
