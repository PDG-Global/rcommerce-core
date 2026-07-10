/// Integration test for HTML invoice template
#[cfg(test)]
mod html_template_integration {
    use super::*;
    use crate::order::{Order, OrderItem};
    use crate::models::customer::Customer;
    use crate::models::address::Address;
    use rust_decimal::Decimal;
    use uuid::Uuid;
    use chrono::Utc;
    
    fn create_test_order() -> Order {
        Order {
            id: Uuid::new_v4(),
            order_number: "ORD-092-331".to_string(),
            customer_id: Some(Uuid::new_v4()),
            customer_email: "customer@example.com".to_string(),
            billing_address_id: Some(Uuid::new_v4()),
            shipping_address_id: Some(Uuid::new_v4()),
            status: crate::order::lifecycle::OrderStatus::Confirmed,
            fulfillment_status: crate::order::fulfillment::FulfillmentStatus::Unfulfilled,
            payment_status: crate::payment::PaymentStatus::Paid,
            currency: "USD".to_string(),
            subtotal: Decimal::new(405000, 2), // 4050.00
            tax_total: Decimal::new(0, 2),     // 0.00
            shipping_total: Decimal::new(7000, 2), // 70.00
            discount_total: Decimal::new(0, 2),    // 0.00
            total: Decimal::new(412000, 2),    // 4120.00
            notes: None,
            tags: vec![],
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        }
    }
    
    fn create_test_customer() -> Customer {
        Customer {
            id: Uuid::new_v4(),
            email: "customer@example.com".to_string(),
            first_name: "Alex".to_string(),
            last_name: "Developer".to_string(),
            phone: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            email_verified: true,
            metadata: serde_json::json!({}),
        }
    }
    
    fn create_test_address() -> Address {
        Address {
            id: Uuid::new_v4(),
            customer_id: Some(Uuid::new_v4()),
            recipient_name: "Alex Developer".to_string(),
            company: None,
            street_address: "101 Tech Plaza, Suite 404".to_string(),
            city: "San Francisco".to_string(),
            state: "CA".to_string(),
            zip_code: "94107".to_string(),
            country: "United States".to_string(),
            phone: None,
            is_default: true,
            address_type: crate::models::address::AddressType::Shipping,
        }
    }
    
    #[test]
    fn test_html_template_loading() {
        let template = NotificationTemplate::load("order_confirmation_html").unwrap();
        assert!(template.html_body.is_some());
        
        let html = template.html_body.unwrap();
        assert!(html.contains("R COMMERCE"));
        assert!(html.contains("{{ order_number }}"));
        assert!(html.contains("{{ customer_name }}"));
    }
    
    #[test]
    fn test_template_variable_population() {
        let order = create_test_order();
        let customer = create_test_customer();
        let shipping = create_test_address();
        let billing = create_test_address();
        
        let mut variables = TemplateVariables::new();
        variables.add_order(&order);
        variables.add_customer(&customer);
        variables.add_addresses(&shipping, &billing);
        variables.add_totals(&order);
        variables.add_company_info("PDG Global Limited", "support@rcommerce.app");
        
        // Check that all required variables are present
        let required_vars = vec![
            "order_number",
            "order_date",
            "order_total",
            "customer_name",
            "shipping_street",
            "shipping_city_state_zip",
            "shipping_country",
            "billing_company",
            "billing_street",
            "billing_city",
            "billing_country",
            "company_name",
            "support_email",
        ];
        
        for var in required_vars {
            assert!(
                variables.inner.contains_key(var),
                "Missing required variable: {}", var
            );
        }
        
        // Check specific values
        assert_eq!(variables.inner.get("order_number").unwrap(), "ORD-092-331");
        assert_eq!(variables.inner.get("customer_name").unwrap(), "Alex Developer");
        assert_eq!(variables.inner.get("order_total").unwrap(), "4120.00");
    }
    
    #[tokio::test]
    async fn test_notification_factory_html() {
        use crate::notification::{NotificationFactory, Recipient, NotificationChannel};
        
        let order = create_test_order();
        let customer = create_test_customer();
        let shipping = create_test_address();
        let billing = create_test_address();
        let order_items = vec![];
        
        let recipient = Recipient::email(
            "customer@example.com".to_string(),
            Some("Alex Developer".to_string()),
        );
        
        let notification = NotificationFactory::order_confirmation_html(
            &order,
            recipient,
            &customer,
            &shipping,
            &billing,
            &order_items,
        );
        
        assert!(notification.is_ok());
        let notification = notification.unwrap();
        
        assert_eq!(notification.subject, "Order Confirmed: {{ order_number }}");
        assert!(notification.html_body.is_some());
        
        let html = notification.html_body.unwrap();
        assert!(html.contains("ORD-092-331"));
        assert!(html.contains("Alex Developer"));
        assert!(html.contains("$4120.00"));
    }
}