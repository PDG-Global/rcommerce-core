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
    AdminNewOrder,
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
            EmailTemplateType::AdminNewOrder => "admin_new_order_html",
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
    /// Subtotal before shipping/tax
    pub subtotal: &'a str,
    /// Discount total (formatted like "$12.75"); "0.00"/"$0.00" = no discount
    pub discount_total: &'a str,
    /// Coupon code that produced the discount, if any
    pub discount_code: Option<&'a str>,
    /// Shipping cost
    pub shipping_total: &'a str,
    /// Shipping method label (may be empty)
    pub shipping_method: &'a str,
    /// Tax total
    pub tax_total: &'a str,
    /// Order items
    pub items: &'a [OrderItem],
    /// Shipping address
    pub shipping_address: &'a Address,
    /// Billing address
    pub billing_address: &'a Address,
    /// Store branding for the email shell (logo header / contact footer)
    pub branding: &'a crate::config::BrandingConfig,
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
        vars.insert("subtotal", params.subtotal);
        vars.insert("shipping_cost", params.shipping_total);
        vars.insert("shipping_method", params.shipping_method);
        vars.insert("tax", params.tax_total);
        vars.insert("company_name", "rCommerce");
        vars.insert("support_email", "support@example.com");

        // Tax row is omitted entirely when no tax was charged
        if params.tax_total != "$0.00" && params.tax_total != "0.00" {
            let tax_row = format!(
                "<tr><td style=\"padding:4px 0;font-size:13px;color:#666666;\">Tax</td><td align=\"right\" style=\"padding:4px 0;font-size:13px;color:#1a1a1a;\">{}</td></tr>",
                params.tax_total
            );
            vars.insert("tax_row", tax_row);
        } else {
            vars.insert("tax_row", "");
        }

        // Discount row follows the same conditional pattern: shown as a
        // negative amount, labelled with the coupon code when known.
        let discount_amount = params
            .discount_total
            .trim_start_matches('$')
            .parse::<rust_decimal::Decimal>()
            .unwrap_or_default();
        if discount_amount > rust_decimal::Decimal::ZERO {
            let label = match params.discount_code {
                Some(code) if !code.is_empty() => format!("Discount ({code})"),
                _ => "Discount".to_string(),
            };
            let discount_row = format!(
                "<tr><td style=\"padding:4px 0;font-size:13px;color:#666666;\">{label}</td><td align=\"right\" style=\"padding:4px 0;font-size:13px;color:#059669;\">-${}</td></tr>",
                discount_amount
            );
            vars.insert("discount_row", discount_row);
            vars.insert("discount_line", format!("{label}: -${}\n", discount_amount));
        } else {
            vars.insert("discount_row", "");
            vars.insert("discount_line", "");
        }

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

        Self::create_notification(params.recipient_email, &template, vars, params.branding)
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
        branding: &crate::config::BrandingConfig,
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
        vars.insert("company_name", &branding.name);
        vars.insert("support_email", &branding.support_email);

        if let Some(date) = next_retry_date {
            vars.insert("next_retry_date", date);
        }
        let site = branding.site_url.trim_end_matches('/');
        vars.insert("retry_url", format!("{site}/account/orders"));
        vars.insert("update_payment_url", format!("{site}/account/orders"));
        vars.insert("contact_support_url", format!("{site}/contact-us"));

        Self::create_notification(recipient_email, &template, vars, branding)
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
        branding: &crate::config::BrandingConfig,
    ) -> Result<Notification> {
        let template = NotificationTemplate::load("subscription_created_html")?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", customer_name);
        vars.insert("subscription_id", subscription_id);
        vars.insert("plan_name", plan_name);
        vars.insert("amount", amount);
        vars.insert("interval", interval);
        vars.insert("next_billing_date", next_billing_date);
        vars.insert("company_name", &branding.name);
        vars.insert("support_email", &branding.support_email);

        Self::create_notification(recipient_email, &template, vars, branding)
    }

    /// Create a welcome email for new customers
    pub fn welcome(
        recipient_email: &str,
        customer_name: &str,
        branding: &crate::config::BrandingConfig,
    ) -> Result<Notification> {
        let template = NotificationTemplate::load("welcome_html")?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", customer_name);
        vars.insert("company_name", &branding.name);
        vars.insert("support_email", &branding.support_email);
        let site = branding.site_url.trim_end_matches('/');
        vars.insert("login_url", format!("{site}/account/login"));
        vars.insert("shop_url", format!("{site}/products"));

        Self::create_notification(recipient_email, &template, vars, branding)
    }

    /// Create a verification email (welcome + email verification)
    pub fn verification(
        recipient_email: &str,
        customer_name: &str,
        verification_url: &str,
        expires_in: &str,
        branding: &crate::config::BrandingConfig,
    ) -> Result<Notification> {
        let template = NotificationTemplate::load("welcome_html")?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", customer_name);
        vars.insert("company_name", &branding.name);
        vars.insert("support_email", &branding.support_email);
        vars.insert("login_url", verification_url);
        vars.insert("verification_url", verification_url);
        vars.insert("expires_in", expires_in);

        Self::create_notification(recipient_email, &template, vars, branding)
    }

    /// Create a password reset email
    pub fn password_reset(
        recipient_email: &str,
        customer_name: &str,
        reset_token: &str,
        reset_url: &str,
        expires_in: &str,
        branding: &crate::config::BrandingConfig,
    ) -> Result<Notification> {
        let template = NotificationTemplate::load("password_reset_html")?;

        let mut vars = TemplateVariables::new();
        vars.insert("customer_name", customer_name);
        vars.insert("reset_url", reset_url);
        vars.insert("reset_token", reset_token);
        vars.insert("expires_in", expires_in);
        vars.insert("company_name", &branding.name);
        vars.insert("support_email", &branding.support_email);
        vars.insert("accent_color", &branding.accent_color);

        Self::create_notification(recipient_email, &template, vars, branding)
    }

    /// Create notification from template and variables
    fn create_notification(
        recipient_email: &str,
        template: &NotificationTemplate,
        variables: TemplateVariables,
        branding: &crate::config::BrandingConfig,
    ) -> Result<Notification> {
        let subject = template.render_subject(&variables)?;
        let body = template.render(&variables)?;
        let html_body = template.render_html_with_brand(&variables, branding)?;

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
            attachments: Vec::new(),
        })
    }

    /// Format order items as HTML table rows
    pub fn format_order_items(items: &[OrderItem]) -> String {
        let mut html = String::new();

        for item in items {
            // Bundle component lines are $0 stock-reservation rows; only the
            // parent bundle line is shown to customers/admins.
            if item.is_bundle_component {
                continue;
            }
            let variant_line = item
                .variant_title
                .as_deref()
                .filter(|v| !v.is_empty())
                .map(|v| {
                    format!(
                        "<span style=\"display:block;font-size:12px;color:#666666;padding-top:2px;\">{}</span>",
                        v
                    )
                })
                .unwrap_or_default();
            let sku_line = if item.sku.is_empty() {
                String::new()
            } else {
                format!(
                    "<span style=\"display:block;font-size:12px;color:#999999;padding-top:2px;\">SKU: {}</span>",
                    item.sku
                )
            };
            html.push_str(&format!(
                "<tr><td style=\"padding:12px 0;border-bottom:1px solid #f0f0f0;font-size:14px;color:#1a1a1a;\"><span style=\"display:block;font-weight:bold;\">{}</span>{}{}</td><td align=\"center\" width=\"60\" style=\"padding:12px 0;border-bottom:1px solid #f0f0f0;font-size:14px;color:#555555;\">{}</td><td align=\"right\" width=\"100\" style=\"padding:12px 0;border-bottom:1px solid #f0f0f0;font-size:14px;color:#1a1a1a;\">${}</td></tr>",
                item.name, variant_line, sku_line, item.quantity, item.price
            ));
        }

        html
    }

    /// Format address as HTML
    pub fn format_address(address: &Address) -> String {
        format!(
            "<p style=\"margin:0;font-size:13px;line-height:1.7;color:#333333;\"><strong>{}</strong><br>{}<br>{}, {} {}<br>{}</p>",
            address.name, address.street, address.city, address.state, address.zip, address.country
        )
    }
}

/// Order item for email templates
#[derive(Debug, Clone)]
pub struct OrderItem {
    pub name: String,
    /// Variant/option selection, e.g. "Degree: 50" (None = no variant line)
    pub variant_title: Option<String>,
    pub sku: String,
    pub quantity: i32,
    pub price: String,
    /// $0 bundle-component fulfillment line; excluded from rendered lists
    #[allow(dead_code)]
    pub is_bundle_component: bool,
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
            variant_title: None,
            sku: "TEST-001".to_string(),
            quantity: 2,
            price: "29.99".to_string(),
            is_bundle_component: false,
        }];

        let html = EmailNotificationFactory::format_order_items(&items);
        assert!(html.contains("Test Product"));
        assert!(html.contains("TEST-001"));
        assert!(html.contains("2"));
        assert!(html.contains("$29.99"));
        assert!(!html.contains("item-variant"), "plain struct: no marker");
    }

    #[test]
    fn test_order_item_variant_line_renders_under_name() {
        let items = vec![OrderItem {
            name: "Caliber 3 Precision Baseplate".to_string(),
            variant_title: Some("Degree: 50".to_string()),
            sku: "CALIBER3-50".to_string(),
            quantity: 1,
            price: "75.00".to_string(),
            is_bundle_component: false,
        }];

        let html = EmailNotificationFactory::format_order_items(&items);
        assert!(html.contains("Caliber 3 Precision Baseplate"));
        assert!(html.contains("Degree: 50"), "variant line must render");
        // Variant sits inside the same name cell, before the SKU sub-line.
        let name_pos = html.find("Caliber 3 Precision Baseplate").unwrap();
        let variant_pos = html.find("Degree: 50").unwrap();
        let sku_pos = html.find("CALIBER3-50").unwrap();
        assert!(name_pos < variant_pos && variant_pos < sku_pos);
    }

    #[test]
    fn test_order_item_empty_variant_renders_no_line() {
        let items = vec![OrderItem {
            name: "Skate Tool".to_string(),
            variant_title: Some(String::new()),
            sku: "TOOL-01".to_string(),
            quantity: 1,
            price: "25.00".to_string(),
            is_bundle_component: false,
        }];

        let html = EmailNotificationFactory::format_order_items(&items);
        assert!(html.contains("Skate Tool"));
        assert_eq!(
            html.matches("padding-top:2px").count(),
            1,
            "only the SKU line"
        );
    }

    #[test]
    fn test_order_confirmation_discount_row() {
        let addr = Address {
            name: "John Doe".to_string(),
            street: "123 Main St".to_string(),
            city: "New York".to_string(),
            state: "NY".to_string(),
            zip: "10001".to_string(),
            country: "USA".to_string(),
        };
        let items = vec![OrderItem {
            name: "Caliber 3 Precision Baseplate".to_string(),
            variant_title: Some("Degree: 50".to_string()),
            sku: "CALIBER3-50".to_string(),
            quantity: 1,
            price: "75.00".to_string(),
            is_bundle_component: false,
        }];

        let notification = EmailNotificationFactory::order_confirmation(OrderConfirmationParams {
            recipient_email: "customer@example.com",
            customer_name: "John",
            order_number: "ORD-1",
            order_date: "Oct 17, 2026",
            order_total: "USD 64.75",
            subtotal: "$75.00",
            discount_total: "$12.75",
            discount_code: Some("1V97NIBP40"),
            shipping_total: "$2.50",
            shipping_method: "",
            tax_total: "$0.00",
            items: &items,
            shipping_address: &addr,
            billing_address: &addr,
            branding: &crate::config::BrandingConfig::default(),
        })
        .expect("order confirmation email must build");

        let html = notification
            .html_body
            .as_deref()
            .expect("html body must be present");
        assert!(
            html.contains("Discount (1V97NIBP40)"),
            "discount row must be labelled with the coupon code"
        );
        assert!(
            html.contains(r#">-$12.75</td>"#),
            "discount amount must be negative"
        );
        assert!(
            html.contains("color:#059669"),
            "discount amount must render green"
        );
        // Plain-text body carries the matching conditional line.
        assert!(notification.body.contains("Discount (1V97NIBP40): -$12.75"));
    }

    #[test]
    fn test_order_confirmation_no_discount_no_row() {
        let addr = Address {
            name: "John Doe".to_string(),
            street: "123 Main St".to_string(),
            city: "New York".to_string(),
            state: "NY".to_string(),
            zip: "10001".to_string(),
            country: "USA".to_string(),
        };
        let items = vec![];

        let notification = EmailNotificationFactory::order_confirmation(OrderConfirmationParams {
            recipient_email: "customer@example.com",
            customer_name: "John",
            order_number: "ORD-2",
            order_date: "Oct 17, 2026",
            order_total: "USD 77.50",
            subtotal: "$75.00",
            discount_total: "$0.00",
            discount_code: None,
            shipping_total: "$2.50",
            shipping_method: "",
            tax_total: "$0.00",
            items: &items,
            shipping_address: &addr,
            billing_address: &addr,
            branding: &crate::config::BrandingConfig::default(),
        })
        .expect("order confirmation email must build");

        let html = notification
            .html_body
            .as_deref()
            .expect("html body must be present");
        assert!(
            !html.contains("Discount"),
            "no discount row must render when there is no discount"
        );
        assert!(!html.contains("{{ discount_row }}"));
        assert!(!notification.body.contains("Discount"));
    }

    #[test]
    fn test_format_order_items_skips_bundle_components() {
        let items = vec![
            OrderItem {
                name: "Race Pack".to_string(),
                variant_title: None,
                sku: "RACE-PACK".to_string(),
                quantity: 1,
                price: "120.00".to_string(),
                is_bundle_component: false,
            },
            // $0 stock-reservation component line: must not render
            OrderItem {
                name: "80mm Maverick Wheels".to_string(),
                variant_title: Some("Degree: 50".to_string()),
                sku: "MAV-80".to_string(),
                quantity: 2,
                price: "0.00".to_string(),
                is_bundle_component: true,
            },
        ];

        let html = EmailNotificationFactory::format_order_items(&items);
        assert!(html.contains("Race Pack"));
        assert!(html.contains("$120.00"));
        assert!(!html.contains("MAV-80"), "component SKU leaked");
        assert!(!html.contains("80mm Maverick"), "component line leaked");
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
