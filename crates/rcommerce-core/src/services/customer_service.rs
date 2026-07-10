use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::{
        Address, CreateAddressRequest, CreateCustomerRequest, Customer, UpdateCustomerRequest,
    },
    repository::traits::CustomerRepositoryTrait,
    repository::CustomerRepository,
    services::{PaginationParams, Service},
    Error, Result,
};

#[derive(Clone)]
pub struct CustomerService {
    repository: Arc<CustomerRepository>,
}

impl CustomerService {
    pub fn new(repository: CustomerRepository) -> Self {
        Self {
            repository: Arc::new(repository),
        }
    }

    /// Create a new customer
    pub async fn create_customer(&self, request: CreateCustomerRequest) -> Result<Customer> {
        // Validate email
        if !crate::common::validation::validate_email(&request.email) {
            return Err(Error::validation("Invalid email format"));
        }

        // Check if email already exists
        if let Some(_existing) = self.repository.find_by_email(&request.email).await? {
            return Err(Error::validation("Email already exists"));
        }

        let customer = self.repository.create_with_request(request).await?;

        Ok(customer)
    }

    /// Create a new customer with password (for registration)
    pub async fn create_customer_with_password(
        &self,
        request: CreateCustomerRequest,
        password_hash: String,
    ) -> Result<Customer> {
        // Validate email
        if !crate::common::validation::validate_email(&request.email) {
            return Err(Error::validation("Invalid email format"));
        }

        // Check if email already exists
        if let Some(_existing) = self.repository.find_by_email(&request.email).await? {
            return Err(Error::validation("Email already exists"));
        }

        let customer = self
            .repository
            .create_with_password(request, password_hash)
            .await?;

        Ok(customer)
    }

    /// Get customer by email (for login)
    pub async fn find_by_email(&self, email: &str) -> Result<Option<Customer>> {
        self.repository.find_by_email(email).await
    }

    /// Update password hash (for password rehashing after migration)
    pub async fn update_password_hash(&self, id: Uuid, password_hash: &str) -> Result<()> {
        self.repository
            .update_password_hash(id, password_hash)
            .await
    }

    /// Update email verification status
    pub async fn update_email_verified(&self, id: Uuid, verified: bool) -> Result<()> {
        self.repository.update_email_verified(id, verified).await
    }

    /// Get customer by ID
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Customer>> {
        self.repository.find_by_id(id).await
    }

    /// Get customer by ID with addresses
    pub async fn get_customer(&self, id: Uuid) -> Result<Option<CustomerDetail>> {
        let customer = match self.repository.find_by_id(id).await? {
            Some(c) => c,
            None => return Ok(None),
        };

        let addresses = self.repository.find_addresses(id).await?;

        Ok(Some(CustomerDetail {
            customer,
            addresses,
        }))
    }

    /// Get customer by email
    pub async fn get_customer_by_email(&self, email: &str) -> Result<Option<CustomerDetail>> {
        let customer = match self.repository.find_by_email(email).await? {
            Some(c) => c,
            None => return Ok(None),
        };

        let id = customer.id;
        let addresses = self.repository.find_addresses(id).await?;

        Ok(Some(CustomerDetail {
            customer,
            addresses,
        }))
    }

    /// List customers with pagination
    pub async fn list_customers(&self, pagination: PaginationParams) -> Result<CustomerList> {
        // For MVP, get all customers and paginate in memory
        // In production, do pagination in database
        let all_customers = self.repository.list().await?;
        let total = all_customers.len() as i64;

        let start = pagination.offset() as usize;
        let end = (start + pagination.limit() as usize).min(all_customers.len());

        let customers = all_customers[start..end].to_vec();

        Ok(CustomerList {
            customers,
            pagination: crate::services::PaginationInfo {
                page: pagination.page,
                per_page: pagination.per_page,
                total,
                total_pages: (total as f64 / pagination.per_page as f64).ceil() as i64,
            },
        })
    }

    /// Update customer
    pub async fn update_customer(
        &self,
        id: Uuid,
        request: UpdateCustomerRequest,
    ) -> Result<Customer> {
        // Check if customer exists
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::not_found("Customer not found"))?;

        // If email is being updated, check if it already exists
        if let Some(ref email) = request.email {
            if let Some(existing) = self.repository.find_by_email(email).await? {
                if existing.id != id {
                    return Err(Error::validation("Email already exists"));
                }
            }
        }

        let customer = self.repository.update_with_request(id, request).await?;

        Ok(customer)
    }

    /// Delete customer (soft delete in production)
    pub async fn delete_customer(&self, id: Uuid) -> Result<bool> {
        // Check if customer exists
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::not_found("Customer not found"))?;

        // Check if customer has orders
        // For MVP, we'll do hard delete

        self.repository.delete(id).await
    }

    /// Add address to customer
    pub async fn add_address(
        &self,
        customer_id: Uuid,
        request: CreateAddressRequest,
    ) -> Result<Address> {
        // Check if customer exists
        self.repository
            .find_by_id(customer_id)
            .await?
            .ok_or_else(|| Error::not_found("Customer not found"))?;

        let address = Address {
            id: uuid::Uuid::new_v4(),
            customer_id,
            first_name: request.first_name,
            last_name: request.last_name,
            company: request.company,
            phone: request.phone,
            address1: request.address1,
            address2: request.address2,
            city: request.city,
            state: request.state,
            country: request.country,
            zip: request.zip,
            is_default_shipping: request.is_default_shipping,
            is_default_billing: request.is_default_billing,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.repository.create_address(customer_id, address).await
    }

    /// Update customer address
    pub async fn update_address(
        &self,
        customer_id: Uuid,
        address_id: Uuid,
        request: CreateAddressRequest,
    ) -> Result<Address> {
        // Ensure address exists and belongs to customer
        self.repository
            .find_address_by_id(customer_id, address_id)
            .await?
            .ok_or_else(|| Error::not_found("Address not found"))?;

        let address = Address {
            id: address_id,
            customer_id,
            first_name: request.first_name,
            last_name: request.last_name,
            company: request.company,
            phone: request.phone,
            address1: request.address1,
            address2: request.address2,
            city: request.city,
            state: request.state,
            country: request.country,
            zip: request.zip,
            is_default_shipping: request.is_default_shipping,
            is_default_billing: request.is_default_billing,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.repository.update_address(customer_id, address).await
    }

    /// Delete customer address
    pub async fn delete_address(&self, customer_id: Uuid, address_id: Uuid) -> Result<bool> {
        // Ensure address exists and belongs to customer
        self.repository
            .find_address_by_id(customer_id, address_id)
            .await?
            .ok_or_else(|| Error::not_found("Address not found"))?;

        self.repository
            .delete_address(customer_id, address_id)
            .await
    }
}

#[async_trait::async_trait]
impl Service for CustomerService {
    async fn health_check(&self) -> Result<()> {
        let _ = self.repository.list().await?;
        Ok(())
    }
}

/// Customer detail with addresses
#[derive(Debug, Clone)]
pub struct CustomerDetail {
    pub customer: Customer,
    pub addresses: Vec<Address>,
}

/// Customer list with pagination
#[derive(Debug, Clone)]
pub struct CustomerList {
    pub customers: Vec<Customer>,
    pub pagination: crate::services::PaginationInfo,
}
