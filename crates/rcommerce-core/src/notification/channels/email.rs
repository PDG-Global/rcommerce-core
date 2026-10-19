//! Email notification channel implementation
//!
//! This module provides email sending capabilities using SMTP.
//! It supports both real SMTP servers and a mock mode for testing.

use crate::notification::{Notification, NotificationChannel};
use crate::{Error, Result};
use lettre::{
    message::{header, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

/// Email sending mode
#[derive(Debug, Clone)]
pub enum EmailMode {
    /// Send emails via SMTP
    Smtp(SmtpConfig),
    /// Send emails via Postmark API
    Postmark(PostmarkConfig),
    /// Log emails to console (for testing)
    Mock,
    /// Save emails to a directory (for testing)
    FileSystem { output_dir: String },
}

/// Postmark configuration
#[derive(Debug, Clone)]
pub struct PostmarkConfig {
    pub server_token: String,
    pub from_address: String,
    pub from_name: String,
    pub message_stream: String,
}

impl PostmarkConfig {
    pub fn new(
        server_token: impl Into<String>,
        from_address: impl Into<String>,
        from_name: impl Into<String>,
    ) -> Self {
        Self {
            server_token: server_token.into(),
            from_address: from_address.into(),
            from_name: from_name.into(),
            message_stream: "outbound".to_string(),
        }
    }
}

/// SMTP configuration
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub from_name: String,
    pub use_tls: bool,
    pub use_starttls: bool,
}

impl SmtpConfig {
    /// Create a config for implicit TLS (SMTPS, port 465)
    pub fn implicit_tls(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
        from_address: impl Into<String>,
        from_name: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            password: password.into(),
            from_address: from_address.into(),
            from_name: from_name.into(),
            use_tls: true,
            use_starttls: false,
        }
    }

    /// Create a config for STARTTLS (port 587)
    pub fn starttls(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
        from_address: impl Into<String>,
        from_name: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            password: password.into(),
            from_address: from_address.into(),
            from_name: from_name.into(),
            use_tls: false,
            use_starttls: true,
        }
    }
}

/// Email notification channel
#[derive(Clone)]
pub struct EmailChannel {
    mode: EmailMode,
    transport: Option<AsyncSmtpTransport<Tokio1Executor>>,
}

impl EmailChannel {
    /// Create a new email channel with SMTP configuration
    pub async fn new_smtp(config: SmtpConfig) -> Result<Self> {
        let creds = Credentials::new(config.username.clone(), config.password.clone());

        let transport = if config.use_tls {
            // Implicit TLS (SMTPS) on port 465
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                .map_err(|e| Error::config(format!("Invalid SMTP host: {}", e)))?
                .port(config.port)
                .credentials(creds)
                .tls(lettre::transport::smtp::client::Tls::Wrapper(
                    lettre::transport::smtp::client::TlsParameters::new(config.host.clone())
                        .map_err(|e| Error::config(format!("TLS parameters: {}", e)))?,
                ))
                .build()
        } else if config.use_starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .map_err(|e| Error::config(format!("Invalid SMTP host: {}", e)))?
                .port(config.port)
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
                .port(config.port)
                .credentials(creds)
                .build()
        };

        // Test the connection
        match transport.test_connection().await {
            Ok(true) => {
                log::info!(
                    "SMTP connection established successfully to {}",
                    config.host
                );
            }
            Ok(false) => {
                log::warn!(
                    "SMTP server {} did not respond to connection test",
                    config.host
                );
            }
            Err(e) => {
                log::warn!(
                    "SMTP connection test failed for {}: {}. Email sending may fail at runtime.",
                    config.host,
                    e
                );
            }
        }

        Ok(Self {
            mode: EmailMode::Smtp(config),
            transport: Some(transport),
        })
    }

    /// Create a new email channel with Postmark configuration
    pub fn new_postmark(config: PostmarkConfig) -> Self {
        log::info!("Email channel created in POSTMARK mode - emails will be sent via Postmark API");
        Self {
            mode: EmailMode::Postmark(config),
            transport: None,
        }
    }

    /// Create a mock email channel for testing (logs to console)
    pub fn new_mock() -> Self {
        log::info!("Email channel created in MOCK mode - emails will be logged to console");
        Self {
            mode: EmailMode::Mock,
            transport: None,
        }
    }

    /// Create a file-system based email channel for testing (saves emails as files)
    pub fn new_filesystem(output_dir: String) -> Result<Self> {
        use std::fs;
        fs::create_dir_all(&output_dir).map_err(|e| {
            Error::config(format!("Failed to create email output directory: {}", e))
        })?;

        log::info!(
            "Email channel created in FILESYSTEM mode - emails will be saved to {}",
            output_dir
        );
        Ok(Self {
            mode: EmailMode::FileSystem { output_dir },
            transport: None,
        })
    }

    /// Send an email notification
    pub async fn send(&self, notification: &Notification) -> Result<()> {
        if notification.channel != NotificationChannel::Email {
            return Err(Error::notification_error(
                "Invalid channel for email sender",
            ));
        }

        match &self.mode {
            EmailMode::Smtp(config) => self.send_smtp(notification, config).await,
            EmailMode::Postmark(config) => self.send_postmark(notification, config).await,
            EmailMode::Mock => self.send_mock(notification).await,
            EmailMode::FileSystem { output_dir } => {
                self.send_filesystem(notification, output_dir).await
            }
        }
    }

    /// Send email via SMTP
    async fn send_smtp(&self, notification: &Notification, config: &SmtpConfig) -> Result<()> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| Error::notification_error("SMTP transport not initialized"))?;

        let from = format!("{} <{}>", config.from_name, config.from_address);

        let message_builder =
            Message::builder()
                .from(from.parse().map_err(|e| {
                    Error::notification_error(format!("Invalid from address: {}", e))
                })?)
                .to(notification
                    .recipient
                    .parse()
                    .map_err(|e| Error::notification_error(format!("Invalid recipient: {}", e)))?)
                .subject(notification.subject.clone());

        let message = if notification.attachments.is_empty() {
            if let Some(ref html_body) = notification.html_body {
                message_builder.multipart(
                    MultiPart::alternative()
                        .singlepart(
                            SinglePart::builder()
                                .header(header::ContentType::TEXT_PLAIN)
                                .body(notification.body.clone()),
                        )
                        .singlepart(
                            SinglePart::builder()
                                .header(header::ContentType::TEXT_HTML)
                                .body(html_body.clone()),
                        ),
                )
            } else {
                message_builder.body(notification.body.clone())
            }
        } else {
            let mut mixed = match notification.html_body {
                Some(ref html_body) => MultiPart::mixed().multipart(
                    MultiPart::alternative()
                        .singlepart(
                            SinglePart::builder()
                                .header(header::ContentType::TEXT_PLAIN)
                                .body(notification.body.clone()),
                        )
                        .singlepart(
                            SinglePart::builder()
                                .header(header::ContentType::TEXT_HTML)
                                .body(html_body.clone()),
                        ),
                ),
                None => MultiPart::mixed().singlepart(
                    SinglePart::builder()
                        .header(header::ContentType::TEXT_PLAIN)
                        .body(notification.body.clone()),
                ),
            };
            for attachment in &notification.attachments {
                let content_type = header::ContentType::parse(&attachment.content_type)
                    .unwrap_or_else(|_| {
                        header::ContentType::parse("application/octet-stream")
                            .expect("static mime type")
                    });
                mixed = mixed.singlepart(
                    SinglePart::builder()
                        .header(content_type)
                        .header(header::ContentDisposition::attachment(
                            attachment.filename.as_str(),
                        ))
                        .body(attachment.content.clone()),
                );
            }
            message_builder.multipart(mixed)
        }
        .map_err(|e| Error::notification_error(format!("Failed to build email: {}", e)))?;

        match transport.send(message).await {
            Ok(response) => {
                log::info!(
                    "Email sent successfully to {}: response={}",
                    notification.recipient,
                    response.message().collect::<Vec<_>>().join(", ")
                );
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to send email to {}: {}", notification.recipient, e);
                Err(Error::notification_error(format!(
                    "Failed to send email: {}",
                    e
                )))
            }
        }
    }

    /// Send email via Postmark API
    async fn send_postmark(
        &self,
        notification: &Notification,
        config: &PostmarkConfig,
    ) -> Result<()> {
        use serde_json::json;

        let from = format!("{} <{}>", config.from_name, config.from_address);

        let mut payload = json!({
            "From": from,
            "To": notification.recipient,
            "Subject": notification.subject,
            "TextBody": notification.body,
            "MessageStream": config.message_stream,
        });

        if let Some(ref html_body) = notification.html_body {
            payload["HtmlBody"] = json!(html_body);
        }

        if !notification.attachments.is_empty() {
            use base64::{engine::general_purpose::STANDARD, Engine as _};
            let attachments: Vec<serde_json::Value> = notification
                .attachments
                .iter()
                .map(|a| {
                    json!({
                        "Name": a.filename,
                        "Content": STANDARD.encode(&a.content),
                        "ContentType": a.content_type,
                    })
                })
                .collect();
            payload["Attachments"] = json!(attachments);
        }

        let client = reqwest::Client::new();
        let response = client
            .post("https://api.postmarkapp.com/email")
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("X-Postmark-Server-Token", &config.server_token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| Error::notification_error(format!("Postmark API error: {}", e)))?;

        if response.status().is_success() {
            log::info!("Email sent via Postmark to {}", notification.recipient);
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(Error::notification_error(format!(
                "Postmark API error ({}): {}",
                status, body
            )))
        }
    }

    /// Mock send - logs to console
    async fn send_mock(&self, notification: &Notification) -> Result<()> {
        log::info!("╔══════════════════════════════════════════════════════════════╗");
        log::info!("║                     MOCK EMAIL SENT                          ║");
        log::info!("╠══════════════════════════════════════════════════════════════╣");
        log::info!("║ To:      {:<50} ║", notification.recipient);
        log::info!("║ Subject: {:<50} ║", notification.subject);
        log::info!("╠══════════════════════════════════════════════════════════════╣");

        for line in notification.body.lines() {
            if line.len() > 60 {
                log::info!("║ {:<60} ║", &line[..60]);
            } else {
                log::info!("║ {:<60} ║", line);
            }
        }

        if let Some(ref html) = notification.html_body {
            log::info!("╠══════════════════════════════════════════════════════════════╣");
            log::info!("║ HTML Body: {:<48} ║", format!("{} bytes", html.len()));
        }

        for attachment in &notification.attachments {
            log::info!(
                "║ Attachment: {:<47} ║",
                format!(
                    "{} ({} bytes)",
                    attachment.filename,
                    attachment.content.len()
                )
            );
        }

        log::info!("╚══════════════════════════════════════════════════════════════╝");

        Ok(())
    }

    /// FileSystem send - saves to file
    async fn send_filesystem(&self, notification: &Notification, output_dir: &str) -> Result<()> {
        use chrono::Utc;
        use std::fs::File;
        use std::io::Write;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!(
            "{}_{}.eml",
            timestamp,
            notification.recipient.replace(['@', '.'], "_")
        );
        let filepath = format!("{}/{}", output_dir, filename);

        let mut content = format!("To: {}\n", notification.recipient);
        content.push_str(&format!("Subject: {}\n", notification.subject));
        content.push_str(&format!("Date: {}\n\n", Utc::now().to_rfc2822()));
        content.push_str(&notification.body);

        if let Some(ref html) = notification.html_body {
            content.push_str("\n\n--HTML Body--\n");
            content.push_str(html);
        }

        for attachment in &notification.attachments {
            content.push_str(&format!(
                "\n\n--Attachment: {} ({} bytes, {})--\n",
                attachment.filename,
                attachment.content.len(),
                attachment.content_type
            ));
        }

        let mut file = File::create(&filepath).map_err(|e| {
            Error::notification_error(format!("Failed to create email file: {}", e))
        })?;

        file.write_all(content.as_bytes())
            .map_err(|e| Error::notification_error(format!("Failed to write email file: {}", e)))?;

        log::info!("Email saved to file: {}", filepath);

        Ok(())
    }

    /// Build email message with both plain text and HTML parts
    pub fn build_email_message(&self, notification: &Notification) -> EmailMessage {
        let from = match &self.mode {
            EmailMode::Smtp(config) => format!("{} <{}>", config.from_name, config.from_address),
            _ => "R Commerce <notifications@rcommerce.local>".to_string(),
        };

        EmailMessage {
            from,
            to: notification.recipient.clone(),
            subject: notification.subject.clone(),
            text_body: notification.body.clone(),
            html_body: notification.html_body.clone(),
        }
    }

    /// Get the current email mode
    pub fn mode(&self) -> &EmailMode {
        &self.mode
    }
}

/// Represents an email message with both plain text and HTML parts
#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub text_body: String,
    pub html_body: Option<String>,
}

impl EmailMessage {
    /// Create a plain text email
    pub fn plain_text(from: String, to: String, subject: String, body: String) -> Self {
        Self {
            from,
            to,
            subject,
            text_body: body,
            html_body: None,
        }
    }

    /// Create an HTML email with plain text fallback
    pub fn html(
        from: String,
        to: String,
        subject: String,
        text_body: String,
        html_body: String,
    ) -> Self {
        Self {
            from,
            to,
            subject,
            text_body,
            html_body: Some(html_body),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::types::NotificationPriority;

    fn create_test_notification() -> Notification {
        let now = chrono::Utc::now();
        Notification {
            id: uuid::Uuid::new_v4(),
            channel: NotificationChannel::Email,
            recipient: "test@example.com".to_string(),
            subject: "Test Email".to_string(),
            body: "This is a test email".to_string(),
            html_body: Some("<p>This is a test email</p>".to_string()),
            priority: NotificationPriority::Normal,
            status: crate::notification::types::DeliveryStatus::Pending,
            attempt_count: 0,
            max_attempts: 3,
            error_message: None,
            metadata: serde_json::json!({}),
            scheduled_at: None,
            created_at: now,
            updated_at: now,
            attachments: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_mock_email_channel() {
        let channel = EmailChannel::new_mock();
        let notification = create_test_notification();

        let result = channel.send(&notification).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_filesystem_email_channel() {
        let temp_dir = std::env::temp_dir().join("rcommerce_email_test");
        let channel = EmailChannel::new_filesystem(temp_dir.to_string_lossy().to_string()).unwrap();
        let notification = create_test_notification();

        let result = channel.send(&notification).await;
        assert!(result.is_ok());

        // Check that file was created
        let entries = std::fs::read_dir(&temp_dir).unwrap();
        assert!(entries.count() > 0);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_email_message_builder() {
        let msg = EmailMessage::plain_text(
            "from@example.com".to_string(),
            "to@example.com".to_string(),
            "Subject".to_string(),
            "Body".to_string(),
        );

        assert_eq!(msg.from, "from@example.com");
        assert_eq!(msg.to, "to@example.com");
        assert_eq!(msg.subject, "Subject");
        assert_eq!(msg.text_body, "Body");
        assert!(msg.html_body.is_none());
    }

    #[test]
    fn test_email_message_html() {
        let msg = EmailMessage::html(
            "from@example.com".to_string(),
            "to@example.com".to_string(),
            "Subject".to_string(),
            "Plain text".to_string(),
            "<p>HTML</p>".to_string(),
        );

        assert!(msg.html_body.is_some());
        assert_eq!(msg.html_body.unwrap(), "<p>HTML</p>");
    }
}
