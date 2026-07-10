//! Email Template System
//!
//! This module provides HTML email templates for various notification types.
//! It uses the existing template system with the invoice.html as the base design.

use crate::notification::{
    templates::{NotificationTemplate, TemplateVariables},
    Notification, NotificationChannel,
};
use crate::Result;
use uuid::Uuid;

/// Email template types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailTemplateType {
    OrderConfirmation,
    OrderShipped,
    PaymentFailed,
    PaymentSuccessful,
    SubscriptionCreated,
    SubscriptionRenewal,
    SubscriptionCancelled,
    DunningFirst,
    DunningRetry,
    DunningFinal,
    Welcome,
    PasswordReset,
    AbandonedCart,
}

impl EmailTemplateType {
    pub fn template_id(&self) -> &'static str {
        match self {
            EmailTemplateType::OrderConfirmation => "order_confirmation_html",
            EmailTemplateType::OrderShipped => "order_shipped_html",
            EmailTemplateType::PaymentFailed => "payment_failed_html",
            EmailTemplateType::PaymentSuccessful => "payment_successful_html",
            EmailTemplateType::SubscriptionCreated => "subscription_created_html",
            EmailTemplateType::SubscriptionRenewal => "subscription_renewal_html",
            EmailTemplateType::SubscriptionCancelled => "subscription_cancelled_html",
            EmailTemplateType::DunningFirst => "dunning_first_html",
            EmailTemplateType::DunningRetry => "dunning_retry_html",
            EmailTemplateType::DunningFinal => "dunning_final_html",
            EmailTemplateType::Welcome => "welcome_html",
            EmailTemplateType::PasswordReset => "password_reset_html",
            EmailTemplateType::AbandonedCart => "abandoned_cart_html",
        }
    }
}

/// Order confirmation email parameters
#[derive(Debug, Clone)]
pub struct OrderConfirmationParams<'a> {
    /// Recipient email address
    pub recipient_email: &'a str,
    /// Customer name
    pub customer_name: &'a str,
    /// Order number
    pub order_number: &'a str,
    /// Order date
    pub order_date: &'a str,
    /// Order total
    pub order_total: &'a str,
    /// Order items
    pub items: &'a [OrderItem],
    /// Shipping address
    pub shipping_address: &'a Address,
    /// Billing address
    pub billing_address: &'a Address,
}

/// Factory for creating email notifications from templates
pub struct EmailNotificationFactory;

impl EmailNotificationFactory {
    /// Create an order confirmation email
    pub fn order_confirmation(params: OrderConfirmationParams<'_>) -> Result<Notification> {
        let template = NotificationTemplate::load("order_confirmation_html")?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", params.customer_name);
        vars.insert("order_number", params.order_number);
        vars.insert("order_date", params.order_date);
        vars.insert("order_total", params.order_total);
        vars.insert("company_name", "R Commerce");
        vars.insert("support_email", "support@rcommerce.local");

        // Format items as HTML
        let items_html = Self::format_order_items(params.items);
        vars.insert("items", &items_html);

        // Format addresses
        vars.insert(
            "shipping_address",
            Self::format_address(params.shipping_address),
        );
        vars.insert(
            "billing_address",
            Self::format_address(params.billing_address),
        );

        Self::create_notification(params.recipient_email, &template, vars)
    }

    /// Create a payment failed email (dunning)
    pub fn payment_failed(
        recipient_email: &str,
        customer_name: &str,
        order_number: &str,
        amount: &str,
        attempt_number: i32,
        max_attempts: i32,
        next_retry_date: Option<&str>,
    ) -> Result<Notification> {
        let template_id = if attempt_number == 1 {
            "dunning_first_html"
        } else if attempt_number >= max_attempts - 1 {
            "dunning_final_html"
        } else {
            "dunning_retry_html"
        };

        let template = NotificationTemplate::load(template_id)?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", customer_name);
        vars.insert("order_number", order_number);
        vars.insert("amount", amount);
        vars.insert("attempt_number", attempt_number.to_string());
        vars.insert("max_attempts", max_attempts.to_string());
        vars.insert("company_name", "R Commerce");
        vars.insert("support_email", "support@rcommerce.local");

        if let Some(date) = next_retry_date {
            vars.insert("next_retry_date", date);
        }

        Self::create_notification(recipient_email, &template, vars)
    }

    /// Create a subscription confirmation email
    pub fn subscription_created(
        recipient_email: &str,
        customer_name: &str,
        subscription_id: &str,
        plan_name: &str,
        amount: &str,
        interval: &str,
        next_billing_date: &str,
    ) -> Result<Notification> {
        let template = NotificationTemplate::load("subscription_created_html")?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", customer_name);
        vars.insert("subscription_id", subscription_id);
        vars.insert("plan_name", plan_name);
        vars.insert("amount", amount);
        vars.insert("interval", interval);
        vars.insert("next_billing_date", next_billing_date);
        vars.insert("company_name", "R Commerce");
        vars.insert("support_email", "support@rcommerce.local");

        Self::create_notification(recipient_email, &template, vars)
    }

    /// Create a welcome email for new customers
    pub fn welcome(recipient_email: &str, customer_name: &str) -> Result<Notification> {
        let template = NotificationTemplate::load("welcome_html")?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", customer_name);
        vars.insert("company_name", "R Commerce");
        vars.insert("support_email", "support@rcommerce.local");
        vars.insert("login_url", "https://rcommerce.local/login");

        Self::create_notification(recipient_email, &template, vars)
    }

    /// Create a password reset email
    pub fn password_reset(
        recipient_email: &str,
        customer_name: &str,
        reset_token: &str,
        reset_url: &str,
        expires_in: &str,
    ) -> Result<Notification> {
        let template = NotificationTemplate::load("password_reset_html")?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", customer_name);
        vars.insert("reset_url", reset_url);
        vars.insert("reset_token", reset_token);
        vars.insert("expires_in", expires_in);
        vars.insert("company_name", "R Commerce");
        vars.insert("support_email", "support@rcommerce.local");

        Self::create_notification(recipient_email, &template, vars)
    }

    /// Create notification from template and variables
    fn create_notification(
        recipient_email: &str,
        template: &NotificationTemplate,
        variables: TemplateVariables,
    ) -> Result<Notification> {
        let subject = template.render_subject(&variables)?;
        let body = template.render(&variables)?;
        let html_body = template.render_html(&variables)?;

        Ok(Notification {
            id: Uuid::new_v4(),
            channel: NotificationChannel::Email,
            recipient: recipient_email.to_string(),
            subject,
            body,
            html_body,
            priority: crate::notification::types::NotificationPriority::Normal,
            status: crate::notification::types::DeliveryStatus::Pending,
            attempt_count: 0,
            max_attempts: 3,
            error_message: None,
            metadata: serde_json::json!({"template_id": template.id}),
            scheduled_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    /// Format order items as HTML
    fn format_order_items(items: &[OrderItem]) -> String {
        let mut html = String::from("<table style='width:100%;border-collapse:collapse;'>");
        html.push_str("<tr style='border-bottom:1px solid #e5e7eb;'>");
        html.push_str("<th style='text-align:left;padding:8px;'>Product</th>");
        html.push_str("<th style='text-align:center;padding:8px;'>Qty</th>");
        html.push_str("<th style='text-align:right;padding:8px;'>Price</th>");
        html.push_str("</tr>");

        for item in items {
            html.push_str("<tr style='border-bottom:1px solid #f3f4f6;'>");
            html.push_str(&format!(
                "<td style='padding:8px;'><strong>{}</strong><br><span style='color:#6b7280;font-size:12px;'>{}</span></td>",
                item.name, item.sku
            ));
            html.push_str(&format!(
                "<td style='text-align:center;padding:8px;'>{}</td>",
                item.quantity
            ));
            html.push_str(&format!(
                "<td style='text-align:right;padding:8px;'>${}</td>",
                item.price
            ));
            html.push_str("</tr>");
        }

        html.push_str("</table>");
        html
    }

    /// Format address as HTML
    fn format_address(address: &Address) -> String {
        format!(
            "<p style='margin:0;'><strong>{}</strong><br>{}<br>{}, {} {}<br>{}</p>",
            address.name, address.street, address.city, address.state, address.zip, address.country
        )
    }
}

/// Order item for email templates
#[derive(Debug, Clone)]
pub struct OrderItem {
    pub name: String,
    pub sku: String,
    pub quantity: i32,
    pub price: String,
}

/// Address for email templates
#[derive(Debug, Clone)]
pub struct Address {
    pub name: String,
    pub street: String,
    pub city: String,
    pub state: String,
    pub zip: String,
    pub country: String,
}

/// Extension trait for NotificationTemplate
#[allow(dead_code)]
trait NotificationTemplateExt {
    fn render_subject(&self, variables: &TemplateVariables) -> Result<String>;
}

impl NotificationTemplateExt for NotificationTemplate {
    fn render_subject(&self, variables: &TemplateVariables) -> Result<String> {
        let mut rendered = self.subject.clone();

        for (key, value) in variables.iter() {
            let placeholder = format!("{{{{ {} }}}}", key);
            rendered = rendered.replace(&placeholder, value);
        }

        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_item_formatting() {
        let items = vec![OrderItem {
            name: "Test Product".to_string(),
            sku: "TEST-001".to_string(),
            quantity: 2,
            price: "29.99".to_string(),
        }];

        let html = EmailNotificationFactory::format_order_items(&items);
        assert!(html.contains("Test Product"));
        assert!(html.contains("TEST-001"));
        assert!(html.contains("2"));
        assert!(html.contains("$29.99"));
    }

    #[test]
    fn test_address_formatting() {
        let address = Address {
            name: "John Doe".to_string(),
            street: "123 Main St".to_string(),
            city: "New York".to_string(),
            state: "NY".to_string(),
            zip: "10001".to_string(),
            country: "USA".to_string(),
        };

        let html = EmailNotificationFactory::format_address(&address);
        assert!(html.contains("John Doe"));
        assert!(html.contains("123 Main St"));
        assert!(html.contains("New York"));
    }
}
