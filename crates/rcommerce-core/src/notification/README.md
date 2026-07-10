# Notification Module Documentation

## Overview

The notification module provides a comprehensive system for sending multi-channel notifications (email, SMS, push, webhook) to customers and administrators. It supports both plain text and HTML email templates with dynamic variable substitution.

## Architecture

### Core Components

```
notification/
├── mod.rs                      # Core types (Notification, Recipient, DeliveryStatus)
├── templates.rs                # Template loading and rendering
├── service.rs                  # NotificationService and factory methods
├── channels/                   # Channel implementations
│   ├── email.rs               # SMTP email channel with HTML support
│   ├── sms.rs                 # SMS channel (Twilio)
│   └── webhook.rs             # Webhook notifications
└── templates/
    └── invoice.html           # HTML invoice email template
```

### Key Types

#### `Notification`
Represents a notification to be sent to a recipient.

```rust
pub struct Notification {
    pub id: Uuid,                              // Unique notification ID
    pub channel: NotificationChannel,          // Email, SMS, Push, etc.
    pub recipient: Recipient,                  // Who to send to
    pub subject: String,                       // Email subject / notification title
    pub body: String,                          // Plain text body
    pub html_body: Option<String>,            // HTML body (for email)
    pub priority: NotificationPriority,        // Low, Normal, High, Urgent
    pub metadata: serde_json::Value,          // Additional data
    pub scheduled_at: Option<DateTime<Utc>>,  // For delayed sending
    pub created_at: DateTime<Utc>,            // Creation timestamp
}
```

#### `NotificationTemplate`
Template for generating notification content with placeholders.

```rust
pub struct NotificationTemplate {
    pub id: String,                            // Template identifier
    pub name: String,                          // Human-readable name
    pub subject: String,                       // Subject template
    pub body: String,                          // Plain text template
    pub html_body: Option<String>,            // HTML template
    pub channel: NotificationChannel,          // Target channel
    pub variables: Vec<String>,               // Required variables
}
```

#### `TemplateVariables`
Container for template variable substitution.

```rust
pub struct TemplateVariables {
    inner: HashMap<String, String>,
}

impl TemplateVariables {
    pub fn add(&mut self, key: String, value: String);
    pub fn add_order(&mut self, order: &Order);
    pub fn add_customer(&mut self, customer: &Customer);
    pub fn add_addresses(&mut self, shipping: &Address, billing: &Address);
    pub fn add_order_items(&mut self, items: &[OrderItem]);
    pub fn add_totals(&mut self, order: &Order);
    pub fn add_company_info(&mut self, company_name: &str, support_email: &str);
}
```

#### `Recipient`
Represents a notification recipient.

```rust
pub struct Recipient {
    pub id: Option<Uuid>,                      // Optional customer ID
    pub channel: NotificationChannel,          // Delivery channel
    pub address: String,                       // Email, phone, webhook URL
    pub name: Option<String>,                  // Display name
}

impl Recipient {
    pub fn email(address: String, name: Option<String>) -> Self;
    pub fn sms(address: String) -> Self;
    pub fn webhook(url: String) -> Self;
}
```

### Notification Channels

#### Email Channel
- Supports both plain text and HTML emails
- MIME multipart/alternative for client compatibility
- SMTP configuration for production sending
- Template-based content generation

```rust
pub struct EmailChannel {
    smtp_host: String,
    smtp_port: u16,
    username: String,
    password: String,
    from_address: String,
    from_name: String,
}
```

#### SMS Channel
- Twilio integration
- Short message formatting
- Delivery status tracking

#### Webhook Channel
- HTTP POST notifications
- JSON payload delivery
- Retry logic with exponential backoff

### Template System

#### Built-in Templates

1. **Order Confirmation HTML** (`order_confirmation_html`)
   - Professional invoice layout
   - Customer and order details
   - Shipping and billing addresses
   - Itemized product list
   - Company branding

2. **Order Confirmation** (`order_confirmation`)
   - Plain text version
   - Concise order information
   
3. **Order Shipped** (`order_shipped`)
   - Shipping notification
   - Tracking information
   - Delivery estimates
   
4. **Low Stock Alert** (`low_stock_alert`)
   - Inventory warnings
   - Critical stock alerts
   - Reorder recommendations

#### Template Variables

Templates support dynamic variable substitution using `{{ variable_name }}` syntax:

| Variable | Description | Example |
|----------|-------------|---------|
| `order_number` | Order identifier | ORD-12345 |
| `order_date` | Formatted date | Jan 24, 2026 |
| `order_total` | Total amount | $99.99 |
| `customer_name` | Customer full name | John Doe |
| `company_name` | Company name | R Commerce |
| `support_email` | Support contact | support@example.com |

#### HTML Template Features

The HTML invoice template (`invoice.html`) includes:

- **Responsive Design**: Mobile-friendly layout
- **Professional Branding**: R Commerce logo and colors
- **Order Metadata**: Number, date, total amount
- **Itemized List**: Products with SKUs and quantities
- **Address Sections**: Separate shipping and billing
- **Totals Breakdown**: Subtotal, shipping, tax, total
- **Company Footer**: Contact information and links

## Usage Examples

### Sending a Basic Notification

```rust
use rcommerce_core::notification::{
    Notification, Recipient, NotificationChannel, NotificationPriority
};
use uuid::Uuid;
use chrono::Utc;

let notification = Notification {
    id: Uuid::new_v4(),
    channel: NotificationChannel::Email,
    recipient: Recipient::email(
        "customer@example.com".to_string(),
        Some("John Doe".to_string())
    ),
    subject: "Order Confirmed: ORD-12345".to_string(),
    body: "Your order has been confirmed.".to_string(),
    html_body: Some("<h1>Order Confirmed</h1>".to_string()),
    priority: NotificationPriority::High,
    metadata: serde_json::json!({}),
    scheduled_at: None,
    created_at: Utc::now(),
};

let service = create_notification_service();
service.send(&notification).await?;
```

### Using Templates

```rust
use rcommerce_core::notification::{
    NotificationTemplate, TemplateVariables, NotificationFactory
};

// Load template
let template = NotificationTemplate::load("order_confirmation_html")?;

// Prepare variables
let mut variables = TemplateVariables::new();
variables.add("order_number".to_string(), "ORD-12345".to_string());
variables.add("customer_name".to_string(), "John Doe".to_string());
variables.add("order_total".to_string(), "99.99".to_string());

// Render templates
let plain_text = template.render(&variables)?;
let html_content = template.render_html(&variables)?;
```

### Factory Pattern

```rust
use rcommerce_core::notification::NotificationFactory;

// Create order confirmation with HTML invoice
let notification = NotificationFactory::order_confirmation_html(
    &order,
    recipient,
    &customer,
    &shipping_address,
    &billing_address,
    &order_items,
)?;

// Send notification
service.send(&notification).await?;
```

## Email Format

### MIME Structure

HTML emails use `multipart/alternative` MIME type:

```
Content-Type: multipart/alternative; boundary="boundary123"

--boundary123
Content-Type: text/plain; charset=utf-8

Plain text version for email clients that don't support HTML

--boundary123
Content-Type: text/html; charset=utf-8

<!DOCTYPE html>
<html>
<!-- HTML version with styling and layout -->
</html>

--boundary123--
```

### Email Headers

```
From: R Commerce <noreply@rcommerce.app>
To: Customer Name <customer@example.com>
Subject: Order Confirmed: ORD-12345
Content-Type: multipart/alternative; boundary="..."
```

## Testing

Run notification module tests:

```bash
# All notification tests
cargo test --lib notification

# Template tests specifically
cargo test --lib notification::templates::tests

# Integration tests
cargo test --lib notification::tests
```

Test coverage includes:
- Template loading and rendering
- Variable substitution
- HTML generation
- Email message structure
- Factory method creation
- Placeholder validation

## Implementation Notes

### Thread Safety
- All types are `Send` and `Sync` where appropriate
- `TemplateVariables` uses `HashMap` for thread-safe access
- `Notification` is cloneable for multi-channel sending

### Error Handling
- Comprehensive error types for template loading, rendering, and sending
- Graceful fallback for missing variables
- Retry logic with exponential backoff

### Performance
- Template compilation at load time
- Efficient string replacement
- Optional HTML generation (only when needed)

### Extensibility
- Easy to add new templates
- Channel-based architecture for new notification types
- Variable system supports any data source

## Future Enhancements

- [ ] Template caching for production performance
- [ ] Database-stored templates
- [ ] Template versioning
- [ ] A/B testing support
- [ ] Analytics integration (open tracking, click tracking)
- [ ] Template editor UI
- [ ] More email providers (SendGrid, Mailgun, AWS SES)
- [ ] Push notification providers (FCM, APNs)
- [ ] SMS providers (Twilio, Vonage)

## Related Documentation

- [Invoice Template Integration](../../INVOICE_TEMPLATE_INTEGRATION.md)
- [HTML Email Preview](../../EMAIL_PREVIEW.md)
- [Security Documentation](../../docs/deployment/04-security.md)