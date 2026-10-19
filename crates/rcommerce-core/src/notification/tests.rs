/// Integration tests for notification module
#[cfg(test)]
mod notification_integration_tests {

    use crate::notification::{
        Notification, NotificationChannel, NotificationPriority, NotificationTemplate,
        TemplateVariables,
    };

    #[test]
    fn test_template_loading_and_rendering() {
        // Load HTML template
        let template_result = NotificationTemplate::load("order_confirmation_html");
        assert!(
            template_result.is_ok(),
            "Failed to load template: {:?}",
            template_result.err()
        );

        let template = template_result.unwrap();
        assert_eq!(template.id, "order_confirmation_html");
        assert_eq!(template.name, "Order Confirmation HTML");
        assert!(template.html_body.is_some(), "HTML body should be present");
        assert_eq!(template.channel, NotificationChannel::Email);

        // Verify template has expected content
        let html_content = template.html_body.as_ref().unwrap();
        assert!(
            html_content.contains("{{ order_number }}"),
            "Template should have order_number placeholder"
        );
        assert!(
            html_content.contains("{{ customer_name }}"),
            "Template should have customer_name placeholder"
        );
        assert!(
            html_content.contains("{{ order_total }}"),
            "Template should have order_total placeholder"
        );

        // Rendering wraps the body in the shared brand shell; pass an explicit
        // brand (code defaults are deliberately neutral)
        let brand = crate::config::BrandingConfig {
            name: "Example Store".to_string(),
            support_email: "support@example.com".to_string(),
            site_url: "https://example.com".to_string(),
            logo_url: "https://example.com/images/logo.png".to_string(),
            accent_color: "#ff4d00".to_string(),
        };
        let rendered = template
            .render_html_with_brand(&TemplateVariables::new(), &brand)
            .unwrap()
            .unwrap();
        assert!(
            rendered.contains("logo.png"),
            "Rendered template should include the brand logo"
        );
        assert!(
            rendered.contains("Thank you for your order"),
            "Rendered template should include the greeting"
        );
    }

    #[test]
    fn test_template_variable_population() {
        let mut variables = TemplateVariables::new();

        // Add test data
        variables.insert("order_number", "ORD-12345");
        variables.insert("customer_name", "John Doe");
        variables.insert("order_total", "99.99");
        variables.insert("order_date", "Jan 25, 2026");
        variables.insert("company_name", "R Commerce");
        variables.insert("support_email", "support@example.com");
        variables.insert(
            "shipping_address",
            "<strong>John Doe</strong><br>123 Main St<br>San Francisco, CA 94102<br>US",
        );
        variables.insert(
            "billing_address",
            "<strong>John Doe</strong><br>123 Main St<br>San Francisco, CA 94102<br>US",
        );

        // Test plain text rendering
        let template = NotificationTemplate::load("order_confirmation").unwrap();
        let plain_text = template.render(&variables).unwrap();

        assert!(plain_text.contains("John Doe"));
        assert!(plain_text.contains("ORD-12345"));
        assert!(plain_text.contains("$99.99"));

        // Test HTML rendering
        let html_template = NotificationTemplate::load("order_confirmation_html").unwrap();
        let html_result = html_template.render_html(&variables);
        assert!(html_result.is_ok());

        let html_content = html_result.unwrap();
        assert!(html_content.is_some(), "HTML content should be generated");

        let html = html_content.unwrap();
        assert!(
            html.contains("ORD-12345"),
            "HTML should contain order number"
        );
        assert!(
            html.contains("John Doe"),
            "HTML should contain customer name"
        );
        assert!(html.contains("99.99"), "HTML should contain order total");
        assert!(
            html.contains("123 Main St"),
            "HTML should contain shipping address"
        );
    }

    #[test]
    fn test_notification_creation_with_html() {
        use crate::common::Address;
        use crate::models::customer::Customer;
        use crate::models::Currency;

        use chrono::Utc;
        use uuid::Uuid;

        // Create mock customer
        let customer = Customer {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            first_name: "Jane".to_string(),
            last_name: "Smith".to_string(),
            phone: Some("+1234567890".to_string()),
            accepts_marketing: false,
            tax_exempt: false,
            currency: Currency::USD,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            confirmed_at: None,
            timezone: None,
            marketing_opt_in: false,
            email_notifications: true,
            sms_notifications: false,
            push_notifications: false,
            password_hash: None,
            is_verified: true,
            is_imported: false,
            email_verified: true,
            last_login_at: None,
            role: crate::models::CustomerRole::Customer,
        };

        // Create mock addresses
        let _shipping = Address {
            id: Uuid::new_v4(),
            customer_id: customer.id,
            first_name: "Jane".to_string(),
            last_name: "Smith".to_string(),
            company: None,
            phone: None,
            address1: "123 Main St".to_string(),
            address2: None,
            city: "San Francisco".to_string(),
            state: Some("CA".to_string()),
            zip: "94102".to_string(),
            country: "US".to_string(),
            is_default_shipping: true,
            is_default_billing: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let _billing = Address {
            id: Uuid::new_v4(),
            customer_id: customer.id,
            first_name: "Jane".to_string(),
            last_name: "Smith".to_string(),
            company: Some("Acme Corp".to_string()),
            phone: None,
            address1: "456 Business Ave".to_string(),
            address2: None,
            city: "New York".to_string(),
            state: Some("NY".to_string()),
            zip: "10001".to_string(),
            country: "US".to_string(),
            is_default_shipping: false,
            is_default_billing: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Test notification creation using the builder pattern
        let notification = Notification::new(
            NotificationChannel::Email,
            "test@example.com".to_string(),
            "Order Confirmed: ORD-TEST-001".to_string(),
            "Your order has been confirmed".to_string(),
        )
        .with_priority(NotificationPriority::High)
        .with_html_body("<h1>Order Confirmed</h1><p>Your order has been confirmed</p>".to_string())
        .with_metadata(serde_json::json!({
            "order_id": Uuid::new_v4(),
            "customer_id": customer.id,
        }));

        assert_eq!(notification.channel, NotificationChannel::Email);
        assert!(notification.html_body.is_some());
        assert_eq!(notification.priority, NotificationPriority::High);
    }

    #[test]
    fn test_all_placeholders_in_template() {
        let template = NotificationTemplate::load("order_confirmation_html").unwrap();
        let html = template.html_body.unwrap();

        // List of all expected placeholders — must match the variables that
        // EmailNotificationFactory::order_confirmation actually populates.
        let expected_placeholders = vec![
            "order_number",
            "order_date",
            "order_total",
            "customer_name",
            "items",
            "shipping_address",
            "billing_address",
        ];

        for placeholder in expected_placeholders {
            let placeholder_str = format!("{{{{ {} }}}}", placeholder);
            assert!(
                html.contains(&placeholder_str),
                "Template missing placeholder: {}",
                placeholder
            );
        }
    }
}
