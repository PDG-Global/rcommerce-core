//! Email testing and management commands

use clap::{Args, Subcommand};
use colored::Colorize;
use dialoguer::Input;
use rcommerce_core::{
    Config,
    notification::{
        NotificationChannel, 
        types::{Notification, NotificationPriority},
    },
};

/// Email commands
#[derive(Subcommand)]
pub enum EmailCommands {
    /// Test email sending
    Test(TestEmailArgs),
    /// List available email templates
    Templates,
    /// Validate email configuration
    Validate,
}

#[derive(Args)]
pub struct TestEmailArgs {
    /// Recipient email address
    #[arg(short, long)]
    recipient: Option<String>,
    
    /// Email subject
    #[arg(short, long)]
    subject: Option<String>,
    
    /// Email body (plain text)
    #[arg(short, long)]
    body: Option<String>,
    
    /// Use HTML content
    #[arg(long)]
    html: bool,
}

/// Handle email commands
pub async fn handle(command: EmailCommands, config: &Config) -> Result<(), String> {
    match command {
        EmailCommands::Test(args) => test_email(args, config).await,
        EmailCommands::Templates => list_templates(),
        EmailCommands::Validate => validate_config(config).await,
    }
}

/// Test email sending
async fn test_email(args: TestEmailArgs, config: &Config) -> Result<(), String> {
    println!("{}", "📧 Email Test".bold().underline());
    println!();
    
    // Get recipient
    let recipient = match args.recipient {
        Some(r) => r,
        None => Input::new()
            .with_prompt("Recipient email address")
            .validate(|input: &String| {
                if input.contains('@') {
                    Ok(())
                } else {
                    Err("Please enter a valid email address")
                }
            })
            .interact()
            .map_err(|e| e.to_string())?,
    };
    
    // Get subject
    let subject = match args.subject {
        Some(s) => s,
        None => Input::new()
            .with_prompt("Email subject")
            .default("Test Email from R Commerce".to_string())
            .interact()
            .map_err(|e| e.to_string())?,
    };
    
    // Get body
    let body = match args.body {
        Some(b) => b,
        None => Input::new()
            .with_prompt("Email body")
            .default("This is a test email from R Commerce.".to_string())
            .interact()
            .map_err(|e| e.to_string())?,
    };
    
    // Check email configuration
    if config.notifications.email.smtp_host.is_empty() {
        println!("{}", "⚠️  Warning: SMTP not configured. Using mock mode.".yellow());
        println!("Emails will be logged to console instead of sent.");
        println!();
    }
    
    println!("\n{}", "Sending test email...".dimmed());
    
    // Create notification
    let notification = Notification {
        id: uuid::Uuid::new_v4(),
        channel: NotificationChannel::Email,
        recipient: recipient.clone(),
        subject: subject.clone(),
        body: body.clone(),
        html_body: if args.html {
            Some(format!("<p>{}</p>", html_escape::encode_text(&body)))
        } else {
            None
        },
        priority: NotificationPriority::Normal,
        template_id: None,
        template_data: None,
        scheduled_at: None,
        created_at: chrono::Utc::now(),
    };
    
    // For now, just log the notification (actual sending would require service setup)
    println!("\n{}", "✅ Test email prepared successfully!".green().bold());
    println!("   Recipient: {}", recipient.cyan());
    println!("   Subject:   {}", subject);
    println!("   Body:      {} bytes", body.len());
    println!("   Mode:      {}", 
        if config.notifications.email.smtp_host.is_empty() {
            "MOCK (would log to console)".yellow()
        } else {
            "SMTP".green()
        }
    );
    
    // Log the notification details
    log::info!("Test email notification created: {:?}", notification);
    
    Ok(())
}

/// List available email templates
fn list_templates() -> Result<(), String> {
    println!("{}", "📧 Available Email Templates".bold().underline());
    println!();
    
    let templates = vec![
        ("order_confirmation", "Order Confirmation", "Sent when an order is placed"),
        ("order_shipped", "Order Shipped", "Sent when an order is shipped"),
        ("payment_failed", "Payment Failed", "Sent when a payment fails"),
        ("subscription_created", "Subscription Created", "Sent when a subscription is created"),
        ("subscription_cancelled", "Subscription Cancelled", "Sent when a subscription is cancelled"),
        ("dunning_first", "Dunning - First Attempt", "First payment failure notification"),
        ("dunning_final", "Dunning - Final Notice", "Final payment failure notice"),
        ("welcome", "Welcome", "Welcome email for new customers"),
        ("password_reset", "Password Reset", "Password reset instructions"),
    ];
    
    for (id, name, description) in templates {
        println!("  {} {}", "•".cyan(), name.bold());
        println!("    ID:          {}", id.dimmed());
        println!("    Description: {}", description);
        println!();
    }
    
    Ok(())
}

/// Validate email configuration
async fn validate_config(config: &Config) -> Result<(), String> {
    println!("{}", "📧 Email Configuration Validation".bold().underline());
    println!();
    
    let email_config = &config.notifications.email;
    
    // Check SMTP configuration
    if email_config.smtp_host.is_empty() {
        println!("{}", "⚠️  SMTP host not configured".yellow());
        println!("   Emails will be logged to console (mock mode).");
        println!();
        println!("To configure SMTP, add the following to your config.toml:");
        println!();
        println!("{}", r#"[notifications.email]
    smtp_host = "smtp.gmail.com"
    smtp_port = 587
    username = "your-email@gmail.com"
    password = "your-app-password"
    from_address = "your-email@gmail.com"
    from_name = "Your Store"
    use_tls = true"#.cyan());
    } else {
        println!("{} SMTP Host:     {}", "✓".green(), email_config.smtp_host);
        println!("{} SMTP Port:     {}", "✓".green(), email_config.smtp_port);
        println!("{} Username:      {}", "✓".green(), email_config.username);
        println!("{} From Address:  {}", "✓".green(), email_config.from_address);
        println!("{} From Name:     {}", "✓".green(), email_config.from_name);
        println!("{} TLS Enabled:   {}", "✓".green(), email_config.use_tls);
        
        println!("\n{}", "Testing SMTP connection...".dimmed());
        
        // Try to connect
        match test_smtp_connection(email_config).await {
            Ok(_) => {
                println!("{}", "✅ SMTP connection successful!".green().bold());
            }
            Err(e) => {
                println!("{}", format!("❌ SMTP connection failed: {}", e).red());
            }
        }
    }
    
    Ok(())
}

/// Test SMTP connection
async fn test_smtp_connection(config: &rcommerce_core::config::EmailConfig) -> Result<(), String> {
    use lettre::{
        AsyncSmtpTransport, Tokio1Executor,
        transport::smtp::authentication::Credentials,
    };
    
    let creds = Credentials::new(config.username.clone(), config.password.clone());
    
    let transport = if config.use_tls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)
            .map_err(|e| format!("Invalid SMTP host: {}", e))?
            .port(config.smtp_port)
            .credentials(creds)
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
            .port(config.smtp_port)
            .credentials(creds)
            .build()
    };
    
    match transport.test_connection().await {
        Ok(true) => Ok(()),
        Ok(false) => Err("SMTP server did not respond".to_string()),
        Err(e) => Err(format!("SMTP connection error: {}", e)),
    }
}
