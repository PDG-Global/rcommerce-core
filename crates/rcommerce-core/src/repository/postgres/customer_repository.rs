use async_trait::async_trait;
use uuid::Uuid;

use super::PostgresDb;
use crate::repository::traits::CustomerRepositoryTrait;
use crate::{
    models::{CreateCustomerRequest, Customer, UpdateCustomerRequest},
    Result,
};

/// The e-mail's unique index refusing a second account is a client error: two
/// registrations can both pass the service's existence check and race here.
fn duplicate_email(e: sqlx::Error) -> crate::Error {
    match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            crate::Error::validation("Email already exists")
        }
        _ => crate::Error::Database(e),
    }
}

#[derive(Clone)]
pub struct PostgresCustomerRepository {
    db: PostgresDb,
}

impl PostgresCustomerRepository {
    pub fn new(db: PostgresDb) -> Self {
        Self { db }
    }

    // Inherent methods for direct access
    pub async fn find_by_email(&self, email: &str) -> Result<Option<Customer>> {
        let customer = sqlx::query_as::<_, Customer>("SELECT * FROM customers WHERE email = $1")
            .bind(email)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(customer)
    }

    pub async fn create_with_request(&self, request: CreateCustomerRequest) -> Result<Customer> {
        let customer = sqlx::query_as::<_, Customer>(
            r#"
            INSERT INTO customers (email, first_name, last_name, phone, accepts_marketing, currency)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(request.email)
        .bind(request.first_name)
        .bind(request.last_name)
        .bind(request.phone)
        .bind(request.accepts_marketing)
        .bind(request.currency)
        .fetch_one(self.db.pool())
        .await
        .map_err(duplicate_email)?;

        Ok(customer)
    }

    pub async fn create_with_password(
        &self,
        request: CreateCustomerRequest,
        password_hash: String,
    ) -> Result<Customer> {
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
        .await
        .map_err(duplicate_email)?;

        Ok(customer)
    }

    pub async fn update_with_request(
        &self,
        id: Uuid,
        request: UpdateCustomerRequest,
    ) -> Result<Customer> {
        let mut sets = Vec::new();
        let mut param_count = 0;

        // Track which fields are present
        let has_email = request.email.is_some();
        let has_first_name = request.first_name.is_some();
        let has_last_name = request.last_name.is_some();

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
        query_builder = query_builder.bind(id);

        let customer = query_builder.fetch_one(self.db.pool()).await?;

        Ok(customer)
    }

    pub async fn find_addresses(&self, customer_id: Uuid) -> Result<Vec<crate::models::Address>> {
        let addresses = sqlx::query_as::<_, crate::models::Address>(
            "SELECT * FROM addresses WHERE customer_id = $1 ORDER BY created_at",
        )
        .bind(customer_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(addresses)
    }

    pub async fn find_address_by_id(
        &self,
        customer_id: Uuid,
        address_id: Uuid,
    ) -> Result<Option<crate::models::Address>> {
        let address = sqlx::query_as::<_, crate::models::Address>(
            "SELECT * FROM addresses WHERE id = $1 AND customer_id = $2",
        )
        .bind(address_id)
        .bind(customer_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(address)
    }

    pub async fn create_address(
        &self,
        customer_id: Uuid,
        address: crate::models::Address,
    ) -> Result<crate::models::Address> {
        let created = sqlx::query_as::<_, crate::models::Address>(
            r#"
            INSERT INTO addresses (
                customer_id, first_name, last_name, company, phone,
                address1, address2, city, province, country, zip,
                is_default_shipping, is_default_billing
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING *
            "#,
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

    pub async fn update_address(
        &self,
        customer_id: Uuid,
        address: crate::models::Address,
    ) -> Result<crate::models::Address> {
        let updated = sqlx::query_as::<_, crate::models::Address>(
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
            "#,
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
        let result = sqlx::query("DELETE FROM addresses WHERE id = $1 AND customer_id = $2")
            .bind(address_id)
            .bind(customer_id)
            .execute(self.db.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl CustomerRepositoryTrait for PostgresCustomerRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Customer>> {
        let customer = sqlx::query_as::<_, Customer>("SELECT * FROM customers WHERE id = $1")
            .bind(id)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(customer)
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<Customer>> {
        let customer = sqlx::query_as::<_, Customer>("SELECT * FROM customers WHERE email = $1")
            .bind(email)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(customer)
    }

    async fn list(&self) -> Result<Vec<Customer>> {
        let customers =
            sqlx::query_as::<_, Customer>("SELECT * FROM customers ORDER BY created_at DESC")
                .fetch_all(self.db.pool())
                .await?;

        Ok(customers)
    }

    async fn create(&self, request: CreateCustomerRequest) -> Result<Customer> {
        let customer = sqlx::query_as::<_, Customer>(
            r#"
            INSERT INTO customers (email, first_name, last_name, phone, accepts_marketing, currency)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(request.email)
        .bind(request.first_name)
        .bind(request.last_name)
        .bind(request.phone)
        .bind(request.accepts_marketing)
        .bind(request.currency)
        .fetch_one(self.db.pool())
        .await
        .map_err(duplicate_email)?;

        Ok(customer)
    }

    async fn update(&self, id: Uuid, request: UpdateCustomerRequest) -> Result<Customer> {
        let mut sets = Vec::new();
        let mut param_count = 0;

        // Track which fields are present
        let has_email = request.email.is_some();
        let has_first_name = request.first_name.is_some();
        let has_last_name = request.last_name.is_some();

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
        query_builder = query_builder.bind(id);

        let customer = query_builder.fetch_one(self.db.pool()).await?;

        Ok(customer)
    }

    async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM customers WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

impl PostgresCustomerRepository {
    /// Update email verification status
    pub async fn update_email_verified(&self, id: Uuid, verified: bool) -> Result<()> {
        sqlx::query("UPDATE customers SET email_verified = $1, updated_at = NOW() WHERE id = $2")
            .bind(verified)
            .bind(id)
            .execute(self.db.pool())
            .await?;

        Ok(())
    }

    /// Update password hash (for password rehashing after migration)
    pub async fn update_password_hash(&self, id: Uuid, password_hash: &str) -> Result<()> {
        sqlx::query(
            "UPDATE customers SET password_hash = $1, is_imported = false, updated_at = NOW() WHERE id = $2"
        )
        .bind(password_hash)
        .bind(id)
        .execute(self.db.pool())
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;

    /// DB-backed tests: require PostgreSQL via TEST_DATABASE_URL; skipped without it
    async fn test_pool() -> Option<sqlx::PgPool> {
        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string());
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                eprintln!("Skipping DB test: cannot connect to {url}: {e}");
                return None;
            }
        };
        if let Err(e) = crate::ensure_test_migrations(&pool).await {
            eprintln!("Skipping DB test: migration failed: {e}");
            return None;
        }
        Some(pool)
    }

    fn request(email: &str) -> CreateCustomerRequest {
        CreateCustomerRequest {
            email: email.to_string(),
            first_name: "Twice".to_string(),
            last_name: "Registered".to_string(),
            phone: None,
            accepts_marketing: false,
            currency: crate::models::Currency::USD,
        }
    }

    #[tokio::test]
    async fn a_second_account_for_an_email_is_a_validation_error() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = PostgresCustomerRepository::new(PostgresDb::new(pool.clone()));
        let email = format!("twice_{}@example.com", Uuid::new_v4());
        repo.create_with_password(request(&email), "hash".to_string())
            .await
            .unwrap();
        let again = repo
            .create_with_password(request(&email), "hash".to_string())
            .await
            .unwrap_err();
        assert!(
            matches!(again, crate::Error::Validation(_)),
            "a client error, not a database failure: {again:?}"
        );
        let again = repo.create_with_request(request(&email)).await.unwrap_err();
        assert!(matches!(again, crate::Error::Validation(_)), "{again:?}");
        sqlx::query("DELETE FROM customers WHERE email = $1")
            .bind(&email)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    /// Regression guard: the addresses table has a `province` column, not
    /// `state` (prod incident). The Rust field stays `state` via
    /// `#[sqlx(rename = "province")]` on `common::Address`; the INSERT and
    /// UPDATE here must reference the real column.
    #[test]
    fn test_address_sql_uses_province_column() {
        let src = include_str!("customer_repository.rs");
        assert!(
            src.contains("city, province, country, zip"),
            "INSERT must use the province column"
        );
        assert!(
            src.contains("province = $8"),
            "UPDATE must set the province column"
        );
        // The INSERT column list itself must not reference a state column
        // (scan only the SQL so this assertion literal cannot match itself).
        let insert_sql = src
            .split("INSERT INTO addresses")
            .nth(1)
            .and_then(|s| s.split("VALUES").next())
            .expect("INSERT INTO addresses SQL not found");
        assert!(
            !insert_sql.contains("state"),
            "INSERT column list must not reference a state column: {insert_sql}"
        );
    }

    #[test]
    fn test_address_field_renames_to_province() {
        // The struct field is `state` for the API/serde contract but must map
        // to the `province` column for sqlx FromRow / query_as.
        let src = include_str!("../../common.rs");
        assert!(
            src.contains("#[sqlx(rename = \"province\")]"),
            "Address.state must be renamed to the province column"
        );
    }
}
