use crate::{Error, Result};

/// Represents a notification template with placeholders for dynamic content substitution.
///
/// Templates define the structure and content of notifications, supporting both plain text
/// and HTML formats. They include variable placeholders (e.g., `{{ variable_name }}`) that
/// are replaced with actual data when rendering.
///
/// # Examples
///
/// ```ignore
/// let template = NotificationTemplate::load("order_confirmation_html")?;
/// let mut variables = TemplateVariables::new();
/// variables.add("order_number".to_string(), "ORD-12345".to_string());
/// variables.add("customer_name".to_string(), "John Doe".to_string());
///
/// let plain_text = template.render(&variables)?;
/// let html_content = template.render_html(&variables)?;
/// ```
#[derive(Debug, Clone)]
pub struct NotificationTemplate {
    /// Unique identifier for the template (e.g., "order_confirmation_html")
    pub id: String,
    /// Human-readable name for the template
    pub name: String,
    /// Subject line template with placeholders
    pub subject: String,
    /// Plain text body template with placeholders
    pub body: String,
    /// Optional HTML body template with placeholders
    pub html_body: Option<String>,
    /// Target notification channel
    pub channel: crate::notification::NotificationChannel,
    /// List of required variable names for this template
    pub variables: Vec<String>,
}

impl NotificationTemplate {
    /// Load from file or database
    pub fn load(id: &str) -> Result<Self> {
        match id {
            // Plain text templates
            "order_confirmation" => Ok(Self::order_confirmation()),
            "order_shipped" => Ok(Self::order_shipped()),
            "low_stock_alert" => Ok(Self::low_stock_alert()),
            // HTML templates
            "order_confirmation_html" => Ok(Self::order_confirmation_html()),
            "order_shipped_html" => Ok(Self::order_shipped_html()),
            "order_cancelled_html" => Ok(Self::order_cancelled_html()),
            "payment_successful_html" => Ok(Self::payment_successful_html()),
            "payment_failed_html" => Ok(Self::payment_failed_html()),
            "refund_processed_html" => Ok(Self::refund_processed_html()),
            "subscription_created_html" => Ok(Self::subscription_created_html()),
            "subscription_renewal_html" => Ok(Self::subscription_renewal_html()),
            "subscription_cancelled_html" => Ok(Self::subscription_cancelled_html()),
            "dunning_first_html" => Ok(Self::dunning_first_html()),
            "dunning_retry_html" => Ok(Self::dunning_retry_html()),
            "dunning_final_html" => Ok(Self::dunning_final_html()),
            "welcome_html" => Ok(Self::welcome_html()),
            "password_reset_html" => Ok(Self::password_reset_html()),
            "abandoned_cart_html" => Ok(Self::abandoned_cart_html()),
            _ => Err(Error::not_found("Template not found")),
        }
    }

    /// Render template with variables
    pub fn render(&self, variables: &TemplateVariables) -> Result<String> {
        let mut rendered = self.body.clone();

        for (key, value) in variables.iter() {
            let placeholder = format!("{{{{ {} }}}}", key);
            rendered = rendered.replace(&placeholder, value);
        }

        Ok(rendered)
    }

    /// Render HTML template with variables
    pub fn render_html(&self, variables: &TemplateVariables) -> Result<Option<String>> {
        if let Some(ref html_template) = self.html_body {
            let mut rendered = html_template.clone();

            for (key, value) in variables.iter() {
                let placeholder = format!("{{{{ {} }}}}", key);
                rendered = rendered.replace(&placeholder, value);
            }

            Ok(Some(rendered))
        } else {
            Ok(None)
        }
    }

    /// Render subject line with variables
    pub fn render_subject(&self, variables: &TemplateVariables) -> Result<String> {
        let mut rendered = self.subject.clone();

        for (key, value) in variables.iter() {
            let placeholder = format!("{{{{ {} }}}}", key);
            rendered = rendered.replace(&placeholder, value);
        }

        Ok(rendered)
    }

    /// Load HTML template from embedded file
    fn load_html_template(path: &str) -> Result<String> {
        match path {
            "invoice.html" => Ok(include_str!("templates/invoice.html").to_string()),
            "order_shipped.html" => Ok(include_str!("templates/order_shipped.html").to_string()),
            "payment_successful.html" => {
                Ok(include_str!("templates/payment_successful.html").to_string())
            }
            "payment_failed.html" => Ok(include_str!("templates/payment_failed.html").to_string()),
            "subscription_created.html" => {
                Ok(include_str!("templates/subscription_created.html").to_string())
            }
            "subscription_renewal.html" => {
                Ok(include_str!("templates/subscription_renewal.html").to_string())
            }
            "subscription_cancelled.html" => {
                Ok(include_str!("templates/subscription_cancelled.html").to_string())
            }
            "dunning_first.html" => Ok(include_str!("templates/dunning_first.html").to_string()),
            "dunning_retry.html" => Ok(include_str!("templates/dunning_retry.html").to_string()),
            "dunning_final.html" => Ok(include_str!("templates/dunning_final.html").to_string()),
            "welcome.html" => Ok(include_str!("templates/welcome.html").to_string()),
            "password_reset.html" => Ok(include_str!("templates/password_reset.html").to_string()),
            "abandoned_cart.html" => Ok(include_str!("templates/abandoned_cart.html").to_string()),
            _ => Err(Error::not_found("HTML template not found")),
        }
    }

    /// Order confirmation template (plain text)
    fn order_confirmation() -> Self {
        Self {
            id: "order_confirmation".to_string(),
            name: "Order Confirmation".to_string(),
            subject: "Order Confirmed: {{ order_number }}".to_string(),
            body: r#"
Hello {{ customer_name }},

Thank you for your order! Your order #{{ order_number }} has been confirmed.

Order Details:
----------------
Total: ${{ order_total }}
Items: {{ item_count }}

We'll send you another email when your order ships.

Best regards,
The R Commerce Team
        "#
            .to_string(),
            html_body: None,
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "order_number".to_string(),
                "order_total".to_string(),
                "item_count".to_string(),
            ],
        }
    }

    /// Order confirmation HTML template with invoice layout
    fn order_confirmation_html() -> Self {
        Self {
            id: "order_confirmation_html".to_string(),
            name: "Order Confirmation HTML".to_string(),
            subject: "Order Confirmed: {{ order_number }}".to_string(),
            body: r#"
Hello {{ customer_name }},

Thank you for your order! Your order #{{ order_number }} has been confirmed.

Order Details:
----------------
Total: ${{ order_total }}
Items: {{ item_count }}

We'll send you another email when your order ships.

Best regards,
The R Commerce Team
        "#
            .to_string(),
            html_body: Some(
                Self::load_html_template("invoice.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "order_number".to_string(),
                "order_date".to_string(),
                "order_total".to_string(),
                "customer_name".to_string(),
                "customer_email".to_string(),
                "shipping_address".to_string(),
                "billing_address".to_string(),
                "subtotal".to_string(),
                "shipping_cost".to_string(),
                "tax".to_string(),
                "items".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn get_default_html_template() -> String {
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Order Confirmation - R Commerce</title>
    <style>
        body { font-family: Arial, sans-serif; background-color: #f9fafb; margin: 0; padding: 20px; }
        .container { max-width: 600px; margin: 0 auto; background-color: #ffffff; border: 1px solid #e5e7eb; }
        .header { padding: 40px; text-align: center; border-bottom: 1px solid #f3f4f6; }
        .content { padding: 40px; }
        .footer { background-color: #0F0F0F; color: #9ca3af; padding: 40px; text-align: center; font-size: 12px; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1 style="color: #EB4F27;">R COMMERCE</h1>
        </div>
        <div class="content">
            <h1>Order Confirmed: {{ order_number }}</h1>
            <p>Thank you for your order, {{ customer_name }}!</p>
            <p><strong>Total:</strong> ${{ order_total }}</p>
        </div>
        <div class="footer">
            <p>Questions? Contact us at <a href="mailto:{{ support_email }}">{{ support_email }}</a></p>
            <p>© 2026 {{ company_name }}</p>
        </div>
    </div>
</body>
</html>"#.to_string()
    }

    /// Order shipped template
    fn order_shipped() -> Self {
        Self {
            id: "order_shipped".to_string(),
            name: "Order Shipped".to_string(),
            subject: "Your Order Has Shipped!".to_string(),
            body: r#"
Great news! Your order #{{ order_number }} has been shipped.

Tracking Information:
{{#if tracking_number}}
Tracking Number: {{ tracking_number }}
Carrier: {{ carrier }}
{{/if}}

Estimated Delivery: {{ estimated_delivery }}

Thank you for your business!
        "#
            .to_string(),
            html_body: None,
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "order_number".to_string(),
                "tracking_number".to_string(),
                "carrier".to_string(),
                "estimated_delivery".to_string(),
            ],
        }
    }

    /// Low stock alert template
    fn low_stock_alert() -> Self {
        Self {
            id: "low_stock_alert".to_string(),
            name: "Low Stock Alert".to_string(),
            subject: "Low Stock Alert: {{ product_name }}".to_string(),
            body: r#"
⚠️ LOW STOCK ALERT

Product: {{ product_name }}
Current Stock: {{ current_stock }}
Threshold: {{ threshold }}

Recommended Reorder Quantity: {{ reorder_quantity }}

{{#if is_critical}}
🚨 CRITICAL: This product is at critically low stock levels!
{{/if}}
        "#
            .to_string(),
            html_body: None,
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "product_name".to_string(),
                "current_stock".to_string(),
                "threshold".to_string(),
                "reorder_quantity".to_string(),
                "is_critical".to_string(),
            ],
        }
    }

    // HTML Templates

    fn order_shipped_html() -> Self {
        Self {
            id: "order_shipped_html".to_string(),
            name: "Order Shipped HTML".to_string(),
            subject: "Your Order Has Shipped: {{ order_number }}".to_string(),
            body: "Your order has shipped.".to_string(),
            html_body: Some(
                Self::load_html_template("order_shipped.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "order_number".to_string(),
                "order_date".to_string(),
                "customer_name".to_string(),
                "tracking_number".to_string(),
                "tracking_url".to_string(),
                "shipping_carrier".to_string(),
                "estimated_delivery".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn order_cancelled_html() -> Self {
        Self {
            id: "order_cancelled_html".to_string(),
            name: "Order Cancelled HTML".to_string(),
            subject: "Order Cancelled: {{ order_number }}".to_string(),
            body: "Your order has been cancelled.".to_string(),
            html_body: Some(
                Self::load_html_template("order_cancelled.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "order_number".to_string(),
                "order_date".to_string(),
                "customer_name".to_string(),
                "cancellation_reason".to_string(),
                "refund_amount".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn payment_successful_html() -> Self {
        Self {
            id: "payment_successful_html".to_string(),
            name: "Payment Successful HTML".to_string(),
            subject: "Payment Confirmed: {{ order_number }}".to_string(),
            body: "Your payment was successful.".to_string(),
            html_body: Some(
                Self::load_html_template("payment_successful.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "order_number".to_string(),
                "customer_name".to_string(),
                "amount".to_string(),
                "payment_method".to_string(),
                "payment_date".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn payment_failed_html() -> Self {
        Self {
            id: "payment_failed_html".to_string(),
            name: "Payment Failed HTML".to_string(),
            subject: "Payment Failed: {{ order_number }}".to_string(),
            body: "Your payment failed.".to_string(),
            html_body: Some(
                Self::load_html_template("payment_failed.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "order_number".to_string(),
                "customer_name".to_string(),
                "amount".to_string(),
                "error_message".to_string(),
                "retry_url".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn refund_processed_html() -> Self {
        Self {
            id: "refund_processed_html".to_string(),
            name: "Refund Processed HTML".to_string(),
            subject: "Refund Processed: {{ order_number }}".to_string(),
            body: "Your refund has been processed.".to_string(),
            html_body: Some(
                Self::load_html_template("refund_processed.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "order_number".to_string(),
                "customer_name".to_string(),
                "refund_amount".to_string(),
                "refund_method".to_string(),
                "processing_time".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn subscription_created_html() -> Self {
        Self {
            id: "subscription_created_html".to_string(),
            name: "Subscription Created HTML".to_string(),
            subject: "Welcome to {{ plan_name }}".to_string(),
            body: "Your subscription has been created.".to_string(),
            html_body: Some(
                Self::load_html_template("subscription_created.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "subscription_id".to_string(),
                "plan_name".to_string(),
                "amount".to_string(),
                "interval".to_string(),
                "next_billing_date".to_string(),
                "trial_end_date".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn subscription_renewal_html() -> Self {
        Self {
            id: "subscription_renewal_html".to_string(),
            name: "Subscription Renewal HTML".to_string(),
            subject: "Subscription Renewed: {{ plan_name }}".to_string(),
            body: "Your subscription has been renewed.".to_string(),
            html_body: Some(
                Self::load_html_template("subscription_renewal.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "plan_name".to_string(),
                "amount".to_string(),
                "billing_date".to_string(),
                "next_billing_date".to_string(),
                "invoice_url".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn subscription_cancelled_html() -> Self {
        Self {
            id: "subscription_cancelled_html".to_string(),
            name: "Subscription Cancelled HTML".to_string(),
            subject: "Subscription Cancelled: {{ plan_name }}".to_string(),
            body: "Your subscription has been cancelled.".to_string(),
            html_body: Some(
                Self::load_html_template("subscription_cancelled.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "plan_name".to_string(),
                "cancellation_date".to_string(),
                "end_date".to_string(),
                "access_until".to_string(),
                "reason".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn dunning_first_html() -> Self {
        Self {
            id: "dunning_first_html".to_string(),
            name: "Dunning First HTML".to_string(),
            subject: "Payment Update Required: {{ order_number }}".to_string(),
            body: "Please update your payment method.".to_string(),
            html_body: Some(
                Self::load_html_template("dunning_first.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "order_number".to_string(),
                "amount".to_string(),
                "payment_method".to_string(),
                "error_message".to_string(),
                "retry_url".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn dunning_retry_html() -> Self {
        Self {
            id: "dunning_retry_html".to_string(),
            name: "Dunning Retry HTML".to_string(),
            subject: "Action Required: Payment Failed for {{ order_number }}".to_string(),
            body: "Your payment failed again.".to_string(),
            html_body: Some(
                Self::load_html_template("dunning_retry.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "order_number".to_string(),
                "amount".to_string(),
                "attempt_number".to_string(),
                "max_attempts".to_string(),
                "next_retry_date".to_string(),
                "update_payment_url".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn dunning_final_html() -> Self {
        Self {
            id: "dunning_final_html".to_string(),
            name: "Dunning Final HTML".to_string(),
            subject: "Final Notice: Payment Required for {{ order_number }}".to_string(),
            body: "Final notice for payment.".to_string(),
            html_body: Some(
                Self::load_html_template("dunning_final.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "order_number".to_string(),
                "amount".to_string(),
                "final_date".to_string(),
                "cancellation_date".to_string(),
                "update_payment_url".to_string(),
                "contact_support_url".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn welcome_html() -> Self {
        Self {
            id: "welcome_html".to_string(),
            name: "Welcome HTML".to_string(),
            subject: "Welcome to {{ company_name }}!".to_string(),
            body: "Welcome to our platform!".to_string(),
            html_body: Some(
                Self::load_html_template("welcome.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "company_name".to_string(),
                "login_url".to_string(),
                "shop_url".to_string(),
                "support_email".to_string(),
                "help_center_url".to_string(),
            ],
        }
    }

    fn password_reset_html() -> Self {
        Self {
            id: "password_reset_html".to_string(),
            name: "Password Reset HTML".to_string(),
            subject: "Password Reset Request".to_string(),
            body: "Reset your password.".to_string(),
            html_body: Some(
                Self::load_html_template("password_reset.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "reset_url".to_string(),
                "reset_token".to_string(),
                "expires_in".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }

    fn abandoned_cart_html() -> Self {
        Self {
            id: "abandoned_cart_html".to_string(),
            name: "Abandoned Cart HTML".to_string(),
            subject: "You left something in your cart!".to_string(),
            body: "Complete your purchase.".to_string(),
            html_body: Some(
                Self::load_html_template("abandoned_cart.html")
                    .unwrap_or_else(|_| Self::get_default_html_template()),
            ),
            channel: crate::notification::NotificationChannel::Email,
            variables: vec![
                "customer_name".to_string(),
                "cart_items".to_string(),
                "cart_total".to_string(),
                "cart_url".to_string(),
                "discount_code".to_string(),
                "company_name".to_string(),
                "support_email".to_string(),
            ],
        }
    }
}

// TemplateVariables is defined in types.rs and re-exported from there
pub use crate::notification::types::TemplateVariables;

impl TemplateVariables {
    /// Add a key-value pair (convenience method for templates module)
    pub fn add(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.insert(key, value);
    }

    pub fn add_order(&mut self, order: &crate::order::Order) {
        self.add("order_id".to_string(), order.id.to_string());
        self.add("order_number".to_string(), order.order_number.clone());
        self.add("order_total".to_string(), order.total.to_string());
        self.add("order_currency".to_string(), order.currency.clone());
        // Add formatted date

        let date_str = order.created_at.format("%b %d, %Y").to_string();
        self.add("order_date".to_string(), date_str);
    }

    pub fn add_customer(&mut self, customer: &crate::models::customer::Customer) {
        self.add("customer_id".to_string(), customer.id.to_string());
        self.add(
            "customer_name".to_string(),
            format!("{} {}", customer.first_name, customer.last_name),
        );
        self.add("customer_email".to_string(), customer.email.clone());
    }

    pub fn add_product(&mut self, product: &crate::models::product::Product) {
        self.add("product_id".to_string(), product.id.to_string());
        self.add("product_name".to_string(), product.title.clone());
        self.add("product_price".to_string(), product.price.to_string());
    }

    pub fn add_inventory_alert(&mut self, alert: &crate::inventory::LowStockAlert) {
        self.add("product_id".to_string(), alert.product_id.to_string());
        self.add("product_name".to_string(), alert.product_name.clone());
        self.add("current_stock".to_string(), alert.current_stock.to_string());
        self.add("threshold".to_string(), alert.threshold.to_string());
        self.add(
            "reorder_quantity".to_string(),
            alert.recommended_reorder_quantity.to_string(),
        );
        self.add("is_critical".to_string(), alert.is_critical().to_string());
    }

    pub fn add_order_items(&mut self, items: &[crate::order::OrderItem]) {
        // Create a simple items list for plain text emails
        let items_text = items
            .iter()
            .map(|item| format!("{} x {} - ${}", item.quantity, item.name, item.price))
            .collect::<Vec<_>>()
            .join("\n");
        self.add("items".to_string(), items_text);

        // Calculate subtotal
        let subtotal: rust_decimal::Decimal = items
            .iter()
            .map(|item| item.price * rust_decimal::Decimal::from(item.quantity))
            .sum();
        self.add("subtotal".to_string(), format!("{:.2}", subtotal));
    }

    pub fn add_addresses(
        &mut self,
        shipping: &crate::common::Address,
        billing: &crate::common::Address,
    ) {
        let recipient_name = format!("{} {}", shipping.first_name, shipping.last_name);
        let street_address = format!(
            "{}{}",
            shipping.address1,
            shipping
                .address2
                .as_ref()
                .map(|a| format!(", {}", a))
                .unwrap_or_default()
        );

        let shipping_str = format!(
            "{}<br>{}<br>{}, {} {}<br>{}",
            recipient_name,
            street_address,
            shipping.city,
            shipping.state.as_deref().unwrap_or(""),
            shipping.zip,
            shipping.country
        );
        self.add("shipping_address".to_string(), shipping_str);

        let billing_recipient = format!("{} {}", billing.first_name, billing.last_name);
        let billing_street = format!(
            "{}{}",
            billing.address1,
            billing
                .address2
                .as_ref()
                .map(|a| format!(", {}", a))
                .unwrap_or_default()
        );

        let billing_str = format!(
            "{}<br>{}<br>{}, {} {}<br>{}",
            billing_recipient,
            billing_street,
            billing.city,
            billing.state.as_deref().unwrap_or(""),
            billing.zip,
            billing.country
        );
        self.add("billing_address".to_string(), billing_str);

        // Add individual address fields for HTML template
        self.add("customer_name".to_string(), recipient_name);
        self.add("shipping_street".to_string(), street_address);
        self.add(
            "shipping_city_state_zip".to_string(),
            format!(
                "{}, {} {}",
                shipping.city,
                shipping.state.as_deref().unwrap_or(""),
                shipping.zip
            ),
        );
        self.add("shipping_country".to_string(), shipping.country.clone());

        self.add("billing_company".to_string(), billing_recipient);
        self.add("billing_street".to_string(), billing_street);
        self.add("billing_city".to_string(), billing.city.clone());
        self.add("billing_country".to_string(), billing.country.clone());
    }

    pub fn add_company_info(&mut self, company_name: &str, support_email: &str) {
        self.add("company_name".to_string(), company_name.to_string());
        self.add("support_email".to_string(), support_email.to_string());
    }

    pub fn add_shipping(&mut self, cost: f64, method: &str) {
        self.add("shipping_cost".to_string(), format!("{:.2}", cost));
        self.add("shipping_method".to_string(), method.to_string());
    }

    pub fn add_tax(&mut self, tax_amount: f64) {
        self.add("tax".to_string(), format!("{:.2}", tax_amount));
        self.add("tax_percent".to_string(), "0".to_string()); // For displaying "Tax (0%)"
    }

    pub fn add_totals(&mut self, order: &crate::order::Order) {
        self.add("subtotal".to_string(), order.subtotal.to_string());
        self.add("tax".to_string(), order.tax_total.to_string());
        self.add(
            "shipping_cost".to_string(),
            order.shipping_total.to_string(),
        );
        self.add("order_total".to_string(), order.total.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_rendering() {
        let template = NotificationTemplate::load("order_confirmation").unwrap();
        let mut vars = TemplateVariables::new();
        vars.add("customer_name".to_string(), "John Doe".to_string());
        vars.add("order_number".to_string(), "ORD-12345".to_string());
        vars.add("order_total".to_string(), "99.99".to_string());
        vars.add("item_count".to_string(), "3".to_string());

        let rendered = template.render(&vars).unwrap();
        assert!(rendered.contains("John Doe"));
        assert!(rendered.contains("ORD-12345"));
        assert!(rendered.contains("$99.99"));
    }

    #[test]
    fn test_html_template_loading() {
        let template = NotificationTemplate::load("order_confirmation_html").unwrap();
        assert!(template.html_body.is_some());

        let html = template.html_body.unwrap();
        assert!(html.contains("R COMMERCE"));
        assert!(html.contains("{{ order_number }}"));
    }
}
