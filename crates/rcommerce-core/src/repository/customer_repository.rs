use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    Result,
    models::{Customer, Address, CreateCustomerRequest, UpdateCustomerRequest},
    traits::Repository,
};
use super::Database;

pub struct CustomerRepository {
    db: Database,
}

impl CustomerRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn create_with_request(&self, request: CreateCustomerRequest) -> Result<Customer> {
        let customer = sqlx::query_as::<_, Customer>(
            r#"
            INSERT INTO customers (email, first_name, last_name, phone, accepts_marketing, currency)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#
        )
        .bind(request.email)
        .bind(request.first_name)
        .bind(request.last_name)
        .bind(request.phone)
        .bind(request.accepts_marketing)
        .bind(request.currency)
        .fetch_one(self.db.pool())
        .await?;
        
        Ok(customer)
    }
    
    pub async fn create_with_password(&self, request: CreateCustomerRequest, password_hash: String) -> Result<Customer> {
        let customer = sqlx::query_as::<_, Customer>(
            r#"
            INSERT INTO customers (email, first_name, last_name, phone, accepts_marketing, currency, password_hash)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#
        )
        .bind(request.email)
        .bind(request.first_name)
        .bind(request.last_name)
        .bind(request.phone)
        .bind(request.accepts_marketing)
        .bind(request.currency)
        .bind(password_hash)
        .fetch_one(self.db.pool())
        .await?;
        
        Ok(customer)
    }
    
    pub async fn update_with_request(&self, id: Uuid, request: UpdateCustomerRequest) -> Result<Customer> {
        let mut sets = Vec::new();
        let mut param_count = 0;
        
        // Track which fields are present
        let has_email = request.email.is_some();
        let has_first_name = request.first_name.is_some();
        let has_last_name = request.last_name.is_some();
        let has_accepts_marketing = request.accepts_marketing.is_some();
        let has_tax_exempt = request.tax_exempt.is_some();
        
        if has_email {
            param_count += 1;
            sets.push(format!("email = ${}", param_count));
        }
        if has_first_name {
            param_count += 1;
            sets.push(format!("first_name = ${}", param_count));
        }
        if has_last_name {
            param_count += 1;
            sets.push(format!("last_name = ${}", param_count));
        }
        if has_accepts_marketing {
            param_count += 1;
            sets.push(format!("accepts_marketing = ${}", param_count));
        }
        if has_tax_exempt {
            param_count += 1;
            sets.push(format!("tax_exempt = ${}", param_count));
        }
        
        if sets.is_empty() {
            return Err(crate::Error::Validation("No fields to update".to_string()));
        }
        
        let id_idx = param_count + 1;
        let query = format!(
            "UPDATE customers SET {}, updated_at = NOW() WHERE id = ${} RETURNING *",
            sets.join(", "),
            id_idx
        );
        
        // Build query with explicit binds
        let mut query_builder = sqlx::query_as::<_, Customer>(&query);
        
        if let Some(email) = request.email {
            query_builder = query_builder.bind(email);
        }
        if let Some(first_name) = request.first_name {
            query_builder = query_builder.bind(first_name);
        }
        if let Some(last_name) = request.last_name {
            query_builder = query_builder.bind(last_name);
        }
        if let Some(accepts_marketing) = request.accepts_marketing {
            query_builder = query_builder.bind(accepts_marketing);
        }
        if let Some(tax_exempt) = request.tax_exempt {
            query_builder = query_builder.bind(tax_exempt);
        }
        query_builder = query_builder.bind(id);
        
        let customer = query_builder
            .fetch_one(self.db.pool())
            .await?;
        
        Ok(customer)
    }
    
    pub async fn find_by_email(&self, email: &str) -> Result<Option<Customer>> {
        let customer = sqlx::query_as::<_, Customer>(
            "SELECT * FROM customers WHERE email = $1"
        )
        .bind(email)
        .fetch_optional(self.db.pool())
        .await?;
        
        Ok(customer)
    }
    
    pub async fn find_addresses(&self, customer_id: Uuid) -> Result<Vec<Address>> {
        let addresses = sqlx::query_as::<_, Address>(
            "SELECT * FROM addresses WHERE customer_id = $1 ORDER BY created_at"
        )
        .bind(customer_id)
        .fetch_all(self.db.pool())
        .await?;
        
        Ok(addresses)
    }

    pub async fn find_address_by_id(&self, customer_id: Uuid, address_id: Uuid) -> Result<Option<Address>> {
        let address = sqlx::query_as::<_, Address>(
            "SELECT * FROM addresses WHERE id = $1 AND customer_id = $2"
        )
        .bind(address_id)
        .bind(customer_id)
        .fetch_optional(self.db.pool())
        .await?;
        
        Ok(address)
    }

    pub async fn create_address(&self, customer_id: Uuid, address: Address) -> Result<Address> {
        let created = sqlx::query_as::<_, Address>(
            r#"
            INSERT INTO addresses (
                customer_id, first_name, last_name, company, phone,
                address1, address2, city, province, country, zip,
                is_default_shipping, is_default_billing
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING *
            "#
        )
        .bind(customer_id)
        .bind(address.first_name)
        .bind(address.last_name)
        .bind(address.company)
        .bind(address.phone)
        .bind(address.address1)
        .bind(address.address2)
        .bind(address.city)
        .bind(address.state)
        .bind(address.country)
        .bind(address.zip)
        .bind(address.is_default_shipping)
        .bind(address.is_default_billing)
        .fetch_one(self.db.pool())
        .await?;
        
        Ok(created)
    }

    pub async fn update_address(&self, customer_id: Uuid, address: Address) -> Result<Address> {
        let updated = sqlx::query_as::<_, Address>(
            r#"
            UPDATE addresses SET
                first_name = $1,
                last_name = $2,
                company = $3,
                phone = $4,
                address1 = $5,
                address2 = $6,
                city = $7,
                province = $8,
                country = $9,
                zip = $10,
                is_default_shipping = $11,
                is_default_billing = $12,
                updated_at = NOW()
            WHERE id = $13 AND customer_id = $14
            RETURNING *
            "#
        )
        .bind(address.first_name)
        .bind(address.last_name)
        .bind(address.company)
        .bind(address.phone)
        .bind(address.address1)
        .bind(address.address2)
        .bind(address.city)
        .bind(address.state)
        .bind(address.country)
        .bind(address.zip)
        .bind(address.is_default_shipping)
        .bind(address.is_default_billing)
        .bind(address.id)
        .bind(customer_id)
        .fetch_one(self.db.pool())
        .await?;
        
        Ok(updated)
    }

    pub async fn delete_address(&self, customer_id: Uuid, address_id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM addresses WHERE id = $1 AND customer_id = $2"
        )
        .bind(address_id)
        .bind(customer_id)
        .execute(self.db.pool())
        .await?;
        
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl Repository<Customer, Uuid> for CustomerRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Customer>> {
        let customer = sqlx::query_as::<_, Customer>(
            "SELECT * FROM customers WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;
        
        Ok(customer)
    }
    
    async fn list(&self) -> Result<Vec<Customer>> {
        let customers = sqlx::query_as::<_, Customer>(
            "SELECT * FROM customers ORDER BY created_at DESC"
        )
        .fetch_all(self.db.pool())
        .await?;
        
        Ok(customers)
    }
    
    async fn create(&self, _entity: Customer) -> Result<Customer> {
        Err(crate::Error::not_implemented("Use create_with_request".to_string()))
    }
    
    async fn update(&self, _entity: Customer) -> Result<Customer> {
        Err(crate::Error::not_implemented("Use update_with_request".to_string()))
    }
    
    async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM customers WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;
        
        Ok(result.rows_affected() > 0)
    }
}
