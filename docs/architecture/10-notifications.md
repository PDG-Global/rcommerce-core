# Notification System Architecture

## Overview

The Notification System provides a unified, extensible framework for delivering messages across multiple channels including email, SMS, push notifications, and webhooks. The system supports templating, queue-based delivery, retry logic, rate limiting, and comprehensive audit trails.

**Supported Channels:**
- Email (SMTP, SendGrid, AWS SES, Mailgun)
- SMS (Twilio, AWS SNS, Vonage, custom providers)
- Push Notifications (FCM, APNs, web push)
- Webhooks (HTTP callbacks)
- In-app notifications
- Slack/Discord integrations

**Key Features:**
- Template-based message rendering
- Queue-based asynchronous delivery
- Multi-channel message composition
- Automatic retries with backoff
- Rate limiting
- Message tracking and analytics
- Preference management
- A/B testing support

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Event Triggers (Orders, Users, etc.)         │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                  Notification Orchestrator                      │
│  - Event listener                                               │
│  - Rule engine                                                  │
│  - Template selection                                           │
│  - Recipient resolution                                         │
└────────────┬──────────────────────┬──────────────────────┬───────┘
             │                      │                      │
    ┌────────▼─────────┐   ┌────────▼────────┐   ┌───────▼────────┐
    │   Email Queue    │   │    SMS Queue    │   │  Webhook Queue │
    │                  │   │                 │   │                │
    └────────┬─────────┘   └────────┬────────┘   └────────┬───────┘
             │                      │                     │
    ┌────────▼─────────┐   ┌────────▼────────┐   ┌───────▼────────┐
    │  Email Worker    │   │   SMS Worker    │   │ Webhook Worker │
    │                  │   │                 │   │                │
    └────────┬─────────┘   └────────┬────────┘   └────────┬───────┘
             │                      │                     │
    ┌────────▼─────────┐   ┌────────▼────────┐   ┌───────▼────────┐
    │ Email Providers  │   │  SMS Providers  │   │Webhook Handlers│
    │┌────────────────┐│   │┌────────────────┐│   │┌────────────────┐│
    ││ SMTP          ││   ││ Twilio        ││   ││ HTTP Callback ││
    ││ SendGrid      ││   ││ AWS SNS       ││   ││               ││
    ││ AWS SES       ││   ││ Vonage        ││   ││               ││
    ││ Mailgun       ││   ││ Custom       ││   ││               ││
    │└────────────────┘│   │└────────────────┘│   │└────────────────┘│
    └──────────────────┘   └──────────────────┘   └──────────────────┘
                                                            
                                                          │
            ┌───────────────────────────────────────────┼──────────────┐
            │                                           │              │
    ┌───────▼────────┐                         ┌────────▼──────┐  ┌───▼──────────┐
    │   Analytics    │                         │    Audit      │  │ Preferences  │
    │    & Tracking  │                         │    Trail      │  │  Management  │
    └────────────────┘                         └───────────────┘  └──────────────┘
```

## Core Data Models

### Notification Entity

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub type_: String,                     // "order_created", "password_reset"
    pub channel: NotificationChannel,      // Email, SMS, etc.
    pub priority: NotificationPriority,    // Low, Normal, High, Urgent
    pub status: NotificationStatus,
    pub recipient: Recipient,
    pub template_id: Option<Uuid>,         // Optional template for rendering
    pub subject: Option<String>,           // For email
    pub body: serde_json::Value,           // Template data
    pub rendered_content: Option<RenderedContent>, // After rendering
    pub scheduled_for: Option<DateTime<Utc>>, // For scheduled notifications
    pub attempts: Vec<DeliveryAttempt>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,  // For time-sensitive notifications
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "notification_channel", rename_all = "snake_case")]
pub enum NotificationChannel {
    Email,
    Sms,
    Push,
    Webhook,
    InApp,
    Slack,
    Discord,
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "notification_priority", rename_all = "snake_case")]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "notification_status", rename_all = "snake_case")]
pub enum NotificationStatus {
    Pending,      // Waiting to be sent
    Processing,   // Being processed
    Queued,       // In delivery queue
    Sent,         // Sent successfully
    Delivered,    // Confirmed delivery
    Failed,       // Failed after retries
    Cancelled,    // Manually cancelled
    Bounced,      // Email bounced
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipient {
    pub id: Uuid,                  // Customer/user ID
    pub email: Option<String>,
    pub phone: Option<String>,     // For SMS
    pub device_tokens: Vec<String>,// For push notifications
    pub webhook_url: Option<String>,// For webhooks
    pub preferences: NotificationPreferences,
    pub locale: String,            // For localization
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub email_enabled: bool,
    pub sms_enabled: bool,
    pub push_enabled: bool,
    pub marketing_emails: bool,
    pub order_updates: bool,
    pub frequency: NotifyFrequency,
    pub quiet_hours: Option<(u8, u8)>,  // (start_hour, end_hour)
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "notify_frequency", rename_all = "snake_case")]
pub enum NotifyFrequency {
    Immediate,
    Hourly,
    Daily,
    Weekly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedContent {
    pub subject: Option<String>,        // Email subject
    pub html_body: Option<String>,      // HTML content
    pub text_body: Option<String>,      // Plain text content
    pub sms_body: Option<String>,       // SMS content (160 chars)
    pub push_data: Option<PushData>,
    pub rendered_at: DateTime<Utc>,
    pub template_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushData {
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
    pub badge: Option<String>,
    pub data: serde_json::Value,
    pub actions: Vec<PushAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushAction {
    pub action: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    pub id: Uuid,
    pub attempt_number: i32,
    pub channel: NotificationChannel,
    pub status: DeliveryStatus,
    pub provider: String,              // "smtp", "twilio", etc.
    pub sent_at: DateTime<Utc>,
    pub response_code: Option<String>,
    pub response_message: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub opened_at: Option<DateTime<Utc>>,  // Email tracking
    pub clicked_at: Option<DateTime<Utc>>, // Link tracking
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "delivery_status", rename_all = "snake_case")]
pub enum DeliveryStatus {
    Success,
    TemporaryFailure,   // Will retry
    PermanentFailure,   // Won't retry
    ProviderError,
    RateLimited,
    InvalidRecipient,
}
```

## Template System

### Template Entity

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationTemplate {
    pub id: Uuid,
    pub name: String,                  // "order_confirmation", "password_reset"
    pub description: String,
    pub channel: NotificationChannel,
    pub subject_template: Option<String>, // For email
    pub body_template: String,         // Tera template syntax
    pub text_body_template: Option<String>, // Plain text for email
    pub design_system: Option<DesignSystem>, // For rich emails
    pub locale: String,                // "en", "es", "fr", etc.
    pub is_active: bool,
    pub metadata: serde_json::Value,
    pub variables_schema: serde_json::Value, // For validation
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignSystem {
    pub template_id: String,           // "order-confirmation-1", "minimal", etc.
    pub brand_colors: BrandColors,
    pub logo_url: Option<String>,
    pub header_html: Option<String>,
    pub footer_html: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandColors {
    pub primary: String,
    pub secondary: String,
    pub accent: String,
    pub text: String,
    pub background: String,
}
```

### Template Rendering Example

```rust
pub struct TemplateRenderer {
    tera: Tera,
}

impl TemplateRenderer {
    pub fn new() -> Result<Self> {
        let mut tera = Tera::new("templates/**/*").unwrap();
        
        // Add custom filters
        tera.register_filter("currency", |value, args| {
            match value {
                Value::Number(n) => {
                    let currency = args.get("currency").and_then(|v| v.as_str()).unwrap_or("USD");
                    Ok(Value::String(format!("{} ${:.2}", currency, n)))
                }
                _ => Err(tera::Error::msg("Filter `currency` received invalid input")),
            }
        });
        
        tera.register_filter("date_format", |value, args| {
            match value {
                Value::String(s) => {
                    let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("%Y-%m-%d");
                    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                        Ok(Value::String(dt.format(format).to_string()))
                    } else {
                        Ok(value.clone())
                    }
                }
                _ => Err(tera::Error::msg("Filter `date_format` received invalid input")),
            }
        });
        
        Ok(Self { tera })
    }
    
    pub fn render_template(
        &self,
        template: &NotificationTemplate,
        context: &serde_json::Value,
        recipient: &Recipient,
    ) -> Result<RenderedContent> {
        let mut tera_context = Context::from_serialize(context)?;
        
        // Add global context
        tera_context.insert("recipient", recipient);
        tera_context.insert("store_name", "Your Store");
        tera_context.insert("store_url", "https://yourstore.com");
        tera_context.insert("year", &Utc::now().year().to_string());
        
        // Render subject
        let subject = if let Some(subject_tpl) = &template.subject_template {
            Some(self.tera.render_str(subject_tpl, &tera_context)?)
        } else {
            None
        };
        
        // Render HTML body
        let html_body = Some(self.tera.render_str(&template.body_template, &tera_context)?);
        
        // Render text body
        let text_body = if let Some(text_tpl) = &template.text_body_template {
            Some(self.tera.render_str(text_tpl, &tera_context)?)
        } else {
            // Auto-generate from HTML
            html_body.as_ref().map(|html| {
                html2text::from_read(html.as_bytes(), 80)
            })
        };
        
        // Generate SMS body (160 chars max)
        let sms_body = if template.channel == NotificationChannel::Sms {
            Some(self.generate_sms_body(&template.body_template, context))
        } else {
            None
        };
        
        // Generate push data
        let push_data = if template.channel == NotificationChannel::Push {
            Some(self.generate_push_data(&template.body_template, &tera_context))
        } else {
            None
        };
        
        Ok(RenderedContent {
            subject,
            html_body,
            text_body,
            sms_body,
            push_data,
            rendered_at: Utc::now(),
            template_version: self.get_template_version(),
        })
    }
    
    fn generate_sms_body(&self, template: &str, context: &serde_json::Value) -> String {
        // Very basic SMS template
        let mut msg = format!("Your Store: ",);
        
        // Truncate to 160 chars
        if msg.len() > 160 {
            msg = format!("{}...", &msg[..157]);
        }
        
        msg
    }
    
    fn generate_push_data(&self, template: &str, context: &Context) -> Option<PushData> {
        Some(PushData {
            title: self.tera.render_str("Order Confirmation", context).ok()?,
            body: self.tera.render_str("Your order has been shipped!", context).ok()?,
            icon: Some("/favicon.ico".to_string()),
            data: json!({ "action": "order_shipped", "order_id": "order_123" }),
            actions: vec![],
        })
    }
    
    fn get_template_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

// Template example: order_confirmation.email.html
/*
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>{{subject}}</title>
</head>
<body>
    <h1>Order Confirmation</h1>
    <p>Hi {{recipient.first_name}},</p>
    <p>Thank you for your order #{{order.number}}!</p>
    
    <table>
        <tr>
            <th>Item</th>
            <th>Quantity</th>
            <th>Price</th>
        </tr>
        {% for item in order.items %}
        <tr>
            <td>{{ item.name }}</td>
            <td>{{ item.quantity }}</td>
            <td>{{ item.price | currency(currency="USD") }}</td>
        </tr>
        {% endfor %}
    </table>
    
    <h3>Total: {{ order.total | currency }}</h3>
    
    <p>Your order will be shipped to:</p>
    <address>
        {{ order.shipping_address.first_name }} {{ order.shipping_address.last_name }}<br>
        {{ order.shipping_address.street1 }}<br>
        {{ order.shipping_address.city }}, {{ order.shipping_address.state }} {{ order.shipping_address.postal_code }}
    </address>
    
    <p><a href="{{ order.tracking_url }}">Track your order</a></p>
    
    <footer>
        <p>&copy; {{ year }} {{ store_name }}</p>
        <p>123 Main St, City, State 12345</p>
        <p><a href="{{ unsubscribe_url }}">Unsubscribe</a></p>
    </footer>
</body>
</html>
*/
```

## Email Provider Implementation

```rust
#[async_trait]
pub trait EmailProvider: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    
    async fn send_email(&self, email: EmailMessage) -> Result<EmailDeliveryResult>;
    
    async fn verify_email(&self, email: &str) -> Result<EmailVerificationResult>;
}

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub from: EmailAddress,
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    pub bcc: Vec<EmailAddress>,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
    pub reply_to: Option<EmailAddress>,
    pub headers: HashMap<String, String>,
    pub attachments: Vec<EmailAttachment>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct EmailAddress {
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EmailAttachment {
    pub filename: String,
    pub content: Vec<u8>,
    pub mime_type: String,
    pub content_id: Option<String>,
}

// SMTP Provider Implementation
pub struct SmtpProvider {
    smtp_client: AsyncSmtpTransport<Tokio1Executor>,
    from_address: EmailAddress,
    reply_to: Option<EmailAddress>,
}

#[async_trait]
impl EmailProvider for SmtpProvider {
    fn id(&self) -> &'static str { "smtp" }
    fn name(&self) -> &'static str { "SMTP" }
    
    async fn send_email(&self, email: EmailMessage) -> Result<EmailDeliveryResult> {
        let mut builder = Message::builder();
        
        // Set from
        let from_address = if let Some(name) = &self.from_address.name {
            Mailbox::new(Some(name.clone()), self.from_address.email.clone())
        } else {
            Mailbox::new(None, self.from_address.email.clone())
        };
        builder = builder.from(from_address);
        
        // Set recipients
        for to in &email.to {
            builder = builder.to(Mailbox::new(
                to.name.clone(),
                to.email.clone(),
            ));
        }
        
        // Set subject
        builder = builder.subject(email.subject);
        
        // Build multipart message
        let mut multipart_builder = MultiPart::mixed();
        
        // Add text body
        multipart_builder = multipart_builder.singlepart(
            SinglePart::builder()
                .header(ContentType::TEXT_PLAIN)
                .body(email.text_body.clone())
        );
        
        // Add HTML body
        multipart_builder = multipart_builder.singlepart(
            SinglePart::builder()
                .header(ContentType::TEXT_HTML)
                .body(email.html_body.clone())
        );
        
        // Add attachments
        for attachment in &email.attachments {
            let content_type = ContentType::parse(&attachment.mime_type)?;
            let content_disposition = ContentDisposition::attachment(&attachment.filename);
            
            let part = SinglePart::builder()
                .header(content_type)
                .header(content_disposition)
                .body(attachment.content.clone());
            
            multipart_builder = multipart_builder.singlepart(part);
        }
        
        let message = builder.multipart(multipart_builder.build()?)?;
        
        // Send email
        let start = Instant::now();
        match self.smtp_client.send(message).await {
            Ok(response) => Ok(EmailDeliveryResult {
                message_id: response.message_id,
                response_code: Some(response.response_code.to_string()),
                delivered_in_ms: start.elapsed().as_millis() as i64,
                success: true,
            }),
            Err(e) => Err(e.into()),
        }
    }
    
    async fn verify_email(&self, email: &str) -> Result<EmailVerificationResult> {
        // Simple regex validation
        let email_regex = Regex::new(r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$")
            .unwrap();
        
        if !email_regex.is_match(email) {
            return Ok(EmailVerificationResult {
                is_valid: false,
                reason: Some("Invalid email format".to_string()),
            });
        }
        
        // Check MX records (optional)
        #[cfg(feature = "dns_validation")]
        {
            let domain = email.split('@').nth(1).unwrap_or("");
            if let Err(_) = self.check_mx_records(domain).await {
                return Ok(EmailVerificationResult {
                    is_valid: false,
                    reason: Some("Domain has no MX records".to_string()),
                });
            }
        }
        
        Ok(EmailVerificationResult {
            is_valid: true,
            reason: None,
        })
    }
}
```

### SendGrid Implementation

```rust
pub struct SendGridProvider {
    api_key: String,
    client: reqwest::Client,
    from_address: EmailAddress,
}

#[async_trait]
impl EmailProvider for SendGridProvider {
    fn id(&self) -> &'static str { "sendgrid" }
    fn name(&self) -> &'static str { "SendGrid" }
    
    async fn send_email(&self, email: EmailMessage) -> Result<EmailDeliveryResult> {
        let mut personalization = serde_json::json!({
            "to": email.to.iter().map(|addr| {
                json!({"email": addr.email, "name": addr.name})
            }).collect::<Vec<_>>(),
        });
        
        if !email.cc.is_empty() {
            personalization["cc"] = json!(email.cc.iter().map(|addr| {
                json!({"email": addr.email, "name": addr.name})
            }).collect::<Vec<_>>());
        }
        
        if !email.bcc.is_empty() {
            personalization["bcc"] = json!(email.bcc.iter().map(|addr| {
               json!({"email": addr.email, "name": addr.name})
            }).collect::<Vec<_>>());
        }
        
        let mut message = serde_json::json!({
            "personalizations": [personalization],
            "from": {
                "email": self.from_address.email,
                "name": self.from_address.name,
            },
            "subject": email.subject,
            "content": [
                {
                    "type": "text/plain",
                    "value": email.text_body,
                },
                {
                    "type": "text/html",
                    "value": email.html_body,
                }
            ],
            "tracking_settings": {
                "click_tracking": { "enable": true },
                "open_tracking": { "enable": true }
            }
        });
        
        // Add attachments
        if !email.attachments.is_empty() {
            message["attachments"] = json!(email.attachments.iter().map(|att| {
                json!({
                    "content": base64::encode(&att.content),
                    "filename": att.filename,
                    "type": att.mime_type,
                })
            }).collect::<Vec<_>>());
        }
        
        // Add custom headers
        if !email.headers.is_empty() {
            message["headers"] = json!(email.headers);
        }
        
        let start = Instant::now();
        let response = self.client
            .post("https://api.sendgrid.com/v3/mail/send")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&message)
            .send()
            .await?;
        
        if response.status().is_success() {
            Ok(EmailDeliveryResult {
                message_id: response.headers()
                    .get("x-message-id")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string()),
                response_code: Some(response.status().to_string()),
                delivered_in_ms: start.elapsed().as_millis() as i64,
                success: true,
            })
        } else {
            let error_body = response.text().await?;
            Err(anyhow!("SendGrid error: {}", error_body))
        }
    }
}
```

## SMS Provider Implementation

```rust
#[async_trait]
pub trait SmsProvider: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    
    async fn send_sms(&self, sms: SmsMessage) -> Result<SmsDeliveryResult>;
    
    async fn verify_phone(&self, phone: &str) -> Result<PhoneVerificationResult>;
}

#[derive(Debug, Clone)]
pub struct SmsMessage {
    pub to: String,                    // E.164 format: +1234567890
    pub from: String,                  // Sender ID or phone number
    pub body: String,                  // Max 1600 chars
    pub message_type: SmsType,
    pub callback_url: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "sms_type", rename_all = "snake_case")]
pub enum SmsType {
    Text,
    Unicode,  // For emojis and non-Latin characters
    Binary,
}

// Twilio Implementation
pub struct TwilioProvider {
    account_sid: String,
    auth_token: String,
    from_number: String,
    client: reqwest::Client,
}

#[async_trait]
impl SmsProvider for TwilioProvider {
    fn id(&self) -> &'static str { "twilio" }
    fn name(&self) -> &'static str { "Twilio" }
    
    async fn send_sms(&self, sms: SmsMessage) -> Result<SmsDeliveryResult> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );
        
        let message_type = match sms.message_type {
            SmsType::Text => "SMS",
            SmsType::Unicode => "Unicode",
            SmsType::Binary => "Binary",
        };
        
        let form = vec![
            ("To", sms.to),
            ("From", sms.from),
            ("Body", sms.body),
            ("MessagingServiceSid", self.from_number.clone()),
        ];
        
        let start = Instant::now();
        let response = self.client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&form)
            .send()
            .await?;
        
        if response.status().is_success() {
            let twilio_response: serde_json::Value = response.json().await?;
            
            Ok(SmsDeliveryResult {
                message_id: twilio_response["sid"].as_str().map(|s| s.to_string()),
                provider_response: Some(twilio_response),
                delivered_in_ms: start.elapsed().as_millis() as i64,
                success: true,
                segments_used: twilio_response["num_segments"].as_i64(),
            })
        } else {
            let error = response.text().await?;
      Err(anyhow!("Twilio error: {}", error))
        }
    }
    
    async fn verify_phone(&self, phone: &str) -> Result<PhoneVerificationResult> {
        // Twilio Lookup API
        let url = format!(
            "https://lookups.twilio.com/v1/PhoneNumbers/{}",
            urlencoding::encode(phone)
        );
        
        let response = self.client
            .get(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .send()
            .await?;
        
        if response.status().is_success() {
            let lookup_response: serde_json::Value = response.json().await?;
            let carrier = lookup_response["carrier"]["type"].as_str();
            
            Ok(PhoneVerificationResult {
                is_valid: true,
    is_mobile: carrier == Some("mobile"),
                is_landline: carrier == Some("landline"),
                country: lookup_response["country_code"].as_str().map(|s| s.to_string()),
                carrier: carrier.map(|s| s.to_string()),
            })
        } else {
            Ok(PhoneVerificationResult {
                is_valid: false,
                ..Default::default()
            })
        }
    }
}
```

## Webhook Notifications

```rust
#[async_trait]
pub trait WebhookProvider: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    
    async fn send_webhook(&self, webhook: WebhookMessage) -> Result<WebhookDeliveryResult>;
}

#[derive(Debug, Clone)]
pub struct WebhookMessage {
    pub url: String,
    pub method: String,  // GET, POST, PUT, PATCH, DELETE
    pub headers: HashMap<String, String>,
    pub body: serde_json::Value,
    pub timeout: Duration,
    pub retry_policy: RetryPolicy,
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_type: BackoffType,  // Linear, exponential
    pub initial_delay: Duration,
    pub max_delay: Duration,
}

#[derive(Debug, Clone)]
pub enum BackoffType {
    Linear,
    Exponential,
}
```

## Notification Orchestrator

```rust
pub struct NotificationOrchestrator {
    template_renderer: Arc<TemplateRenderer>,
    channel_providers: HashMap<NotificationChannel, Arc<dyn ChannelProvider>>,
    queue_manager: Arc<dyn QueueManager>,
    event_dispatcher: Arc<dyn EventDispatcher>,
    config: NotificationConfig,
}

impl NotificationOrchestrator {
    pub async fn send_notification(&self, notification: &Notification) -> Result<Notification> {
        let mut updated_notification = notification.clone();
        
        // 1. Check if recipient wants this notification
        if !self.should_send_notification(notification) {
            updated_notification.status = NotificationStatus::Cancelled;
            return Ok(updated_notification);
        }
        
        // 2. Check quiet hours
        if let Some((start, end)) = notification.recipient.preferences.quiet_hours {
            let now = Utc::now().hour() as u8;
            if (start <= end && now >= start && now < end) ||
               (start > end && (now >= start || now < end)) {
                // Reschedule for after quiet hours
                updated_notification.scheduled_for = Some(self.after_quiet_hours(start, end));
                return Ok(updated_notification);
            }
        }
        
        // 3. Queue notification for delivery
        self.queue_manager.enqueue(notification.clone()).await?;
        
        updated_notification.status = NotificationStatus::Queued;
        
        // 4. Emit event
        self.event_dispatcher.dispatch(
            Event::NotificationQueued {
                notification_id: notification.id,
                channel: notification.channel,
            }
        ).await?;
        
        Ok(updated_notification)
    }
    
    pub async fn process_queued_notification(&self, notification: Notification) -> Result<()> {
        let provider = self.channel_providers
            .get(&notification.channel)
            .ok_or_else(|| NotificationError::UnsupportedChannel(notification.channel))?;
        
        // Get template
        let template = if let Some(template_id) = notification.template_id {
            self.template_repository.find_by_id(template_id).await?
                .ok_or_else(|| NotificationError::TemplateNotFound(template_id))?;
        } else {
            // Use default template
            self.template_repository.find_by_name(&notification.type_).await?
                .ok_or_else(|| NotificationError::TemplateNotFoundByName(notification.type_.clone()))?;
        };
        
        // Render template
        let rendered = self.template_renderer
            .render_template(&template, &notification.body, &notification.recipient)?;
        
        // Prepare message for channel
        let message = self.prepare_channel_message(&notification, &rendered)?;
        
        // Send via provider
        let result = provider.send(message).await;
        
        // Track attempt
        self.track_delivery_attempt(&notification, &result).await?;
        
        // Handle result
        match result {
            Ok(delivery) => {
                let mut updated_notification = notification.clone();
                updated_notification.status = NotificationStatus::Sent;
                updated_notification.rendered_content = Some(rendered);
                self.notification_repository.update(updated_notification).await?;
                
                self.event_dispatcher.dispatch(
                    Event::NotificationDelivered {
                        notification_id: notification.id,
                        channel: notification.channel,
                    }
                ).await?;
            }
            Err(e) => {
                // Check if should retry
                if self.should_retry(&notification, &e) {
                    self.queue_manager.retry_later(notification, &e).await?;
                } else {
                    let mut updated_notification = notification.clone();
                    updated_notification.status = NotificationStatus::Failed;
                    self.notification_repository.update(updated_notification).await?;
                }
            }
        }
        
        Ok(())
    }
    
    fn should_retry(&self, notification: &Notification, error: &Error) -> bool {
        let max_attempts = match notification.priority {
            NotificationPriority::Urgent => 10,
            NotificationPriority::High => 7,
            NotificationPriority::Normal => 5,
            NotificationPriority::Low => 3,
        };
        
        notification.attempts.len() < max_attempts as usize
    }
}
```

## Configuration

```toml
[notifications]
# Global settings
enabled = true
default_sender_name = "Your Store"
default_sender_email = "noreply@yourstore.com"
default_sms_sender = "+1234567890"

# Queue settings
queue_provider = "redis"  # "memory", "redis", "database"
worker_count = 3
max_retries = 5

# Template settings
template_cache = true
template_auto_reload = true  # Development

# Email configuration
[notifications.email]
provider = "sendgrid"  # "smtp", "ses", "mailgun"

[notifications.email.sendgrid]
api_key = "${SENDGRID_API_KEY}"
from_address = "noreply@yourstore.com"
from_name = "Your Store"
tracking_enabled = true

# SMTP fallback (used if primary fails)
[notifications.email.smtp_fallback]
enabled = true
host = "smtp.example.com"
port = 587
username = "${SMTP_USER}"
password = "${SMTP_PASS}"
use_tls = true

# SMS configuration
[notifications.sms]
provider = "twilio"  # "twilio", "sns", "vonage"

[notifications.sms.twilio]
account_sid = "${TWILIO_ACCOUNT_SID}"
auth_token = "${TWILIO_AUTH_TOKEN}"
from_number = "${TWILIO_FROM}"
messaging_service_sid = "${TWILIO_MESSAGING_SID}"

# Push notifications
[notifications.push]
provider = "fcm"  # "fcm", "apns", "onesignal"

[notifications.push.fcm]
service_account_key = "${GOOGLE_SERVICE_ACCOUNT_KEY}"
project_id = "${GOOGLE_PROJECT_ID}"

[notifications.push.apns]
team_id = "${APPLE_TEAM_ID}"
key_id = "${APPLE_KEY_ID}"
private_key = "${APPLE_PRIVATE_KEY}"
topic = "com.yourstore.app"

# Webhook configuration
[notifications.webhooks]
timeout_seconds = 30
max_concurrent = 50
retry_policy = "exponential"

# Rate limiting
[[notifications.rate_limits]]
channel = "email"
max_per_minute = 100
burst = 20

[[notifications.rate_limits]]
channel = "sms"
max_per_minute = 10
burst = 2

[[notifications.rate_limits]]
channel = "push"
max_per_minute = 500
burst = 100

# Template examples
[[notification_templates]]
name = "order_confirmation"
channel = "email"
subject = "Order Confirmed - #{{ order.number }}"
locale = "en"
body = """
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Order Confirmation - {{ order.number }}</title>
</head>
<body>
    <h1>Thank you for your order!</h1>
    <p>Order #{{ order.number }}</p>
    <p>Total: {{ order.total | currency }}</p>
</body>
</html>
"""

[[notification_templates]]
name = "order_shipped"
channel = "sms"
locale = "en"
body = "Your order #{{ order.number }} has shipped! Track it: {{ order.tracking_url }}"

[[notification_templates]]
name = "password_reset"
channel = "email"
subject = "Reset your password"
locale = "en"
body = """
Click the link to reset your password: {{ reset_url }}
This link expires in 1 hour.
"""
```

## Usage Examples

### Order Confirmation Notification

```rust
// Create notification
let notification = Notification {
    id: Uuid::new_v4(),
    type_: "order_confirmation".to_string(),
    channel: NotificationChannel::Email,
    priority: NotificationPriority::High,
    status: NotificationStatus::Pending,
    recipient: Recipient {
        id: customer.id,
        email: Some(customer.email),
        phone: None,
        device_tokens: vec![],
        webhook_url: None,
        preferences: NotificationPreferences {
            email_enabled: true,
            sms_enabled: false,
            push_enabled: true,
            marketing_emails: customer.accepts_marketing,
            order_updates: true,
            frequency: NotifyFrequency::Immediate,
            quiet_hours: None,
        },
        locale: customer.locale.unwrap_or_else(|| "en".to_string()),
    },
    template_id: Some(template.id),
    subject: None, // Will be rendered from template
    body: json!({
        "order": order,
        "customer": customer,
        "items": order.items,
        "tracking_url": tracking_url,
    }),
    rendered_content: None,
    scheduled_for: None,
    attempts: vec![],
    metadata: json!({
        "order_number": order.number,
        "order_total": order.total,
    }),
    created_at: Utc::now(),
    updated_at: Utc::now(),
    expires_at: None,
};

// Send notification
notification_orchestrator.send_notification(&notification).await?;
```

### Multi-Channel Notification

```rust
pub async fn notify_order_shipped(
    &self,
    order: &Order,
    customer: &Customer,
    tracking_url: String,
) -> Result<Vec<Notification>> {
    let mut notifications = Vec::new();
    
    // Email notification
    if customer.preferences.email_enabled {
        notifications.push(
            self.create_notification(
                NotificationChannel::Email,
                "order_shipped".to_string(),
                customer,
                json!({
                    "order": order,
                    "tracking_url": tracking_url,
                }),
            ).await?
        );
    }
    
    // SMS notification (if enabled and phone available)
    if customer.preferences.sms_enabled && customer.phone.is_some() {
        notifications.push(
            self.create_notification(
                NotificationChannel::Sms,
                "order_shipped".to_string(),
                customer,
                json!({
                    "order_number": order.number,
                    "tracking_url": self.shorten_url(&tracking_url),
                }),
            ).await?
        );
    }
    
    // Push notification (if app installed)
    if !customer.device_tokens.is_empty() {
        notifications.push(
            self.create_notification(
                NotificationChannel::Push,
                "order_shipped".to_string(),
                customer,
                json!({
                    "order": order,
                    "tracking_url": tracking_url,
                }),
            ).await?
        );
    }
    
    // Send all notifications
    for notification in &notifications {
        self.notification_orchestrator.send_notification(notification).await?;
    }
    
    Ok(notifications)
}
```

## Analytics and Tracking

```rust
#[derive(Debug, Clone)]
pub struct NotificationAnalytics {
    pub sent_count: i64,
    pub delivered_count: i64,
    pub opened_count: i64,
    pub clicked_count: i64,
    pub bounced_count: i64,
    pub failed_count: i64,
    pub avg_delivery_time_ms: i64,
    pub most_popular_templates: Vec<(String, i64)>,
    pub peak_send_times: Vec<(DateTime<Utc>, i64)>,
}

impl NotificationOrchestrator {
    pub async fn get_analytics(&self, period: DateRange) -> Result<NotificationAnalytics> {
        let stats = self.notification_repository.get_stats(period).await?;
        
        Ok(NotificationAnalytics {
            sent_count: stats.sent,
            delivered_count: stats.delivered,
            opened_count: stats.opened,
            clicked_count: stats.clicked,
            bounced_count: stats.bounced,
            failed_count: stats.failed,
            avg_delivery_time_ms: stats.avg_delivery_time_ms,
            most_popular_templates: stats.popular_templates,
            peak_send_times: stats.peak_times,
        })
    }
}
```

## Bounce & Complaint Handling

```rust
pub struct BounceHandler {
    notification_repository: Arc<dyn NotificationRepository>,
    customer_repository: Arc<dyn CustomerRepository>,
}

impl BounceHandler {
    pub async fn handle_hard_bounce(&self, email: &str, reason: String) -> Result<()> {
        // Find customer
        let customer = self.customer_repository.find_by_email(email).await?
            .ok_or_else(|| anyhow!("Customer not found"))?;
        
        // Update customer to suppress emails
        let mut updated_customer = customer.clone();
        updated_customer.email_suppressed = true;
        updated_customer.suppression_reason = Some(reason);
        
        self.customer_repository.update(updated_customer).await?;
        
        // Log bounce
        info!("Hard bounce for {}: {}", email, reason);
        
        Ok(())
    }
    
    pub async fn handle_complaint(&self, email: &str, complaint_type: String) -> Result<()> {
        // Immediately unsubscribe
        let customer = self.customer_repository.find_by_email(email).await?
            .ok_or_else(|| anyhow!("Customer not found"))?;
        
        let mut updated_customer = customer.clone();
        updated_customer.accepts_marketing = false;
        updated_customer.unsubscribed_at = Some(Utc::now());
        updated_customer.unsubscription_source = Some(complaint_type);
        
        self.customer_repository.update(updated_customer).await?;
        
        info!("Complaint received for {}: {}", email, complaint_type);
        
        Ok(())
    }
}
```

This comprehensive notification system provides multi-channel support, template management, queue-based delivery, analytics, and proper bounce/complaint handling suitable for production ecommerce environments.