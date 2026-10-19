//! PostgreSQL repository implementations

pub mod customer_repository;
pub mod newsletter_repository;
pub mod product_repository;
pub mod review_repository;

pub use customer_repository::PostgresCustomerRepository;
pub use newsletter_repository::PostgresNewsletterRepository;
pub use product_repository::PostgresProductRepository;
pub use review_repository::PostgresProductReviewRepository;

use sqlx::{Pool, Postgres};

/// PostgreSQL database handle
#[derive(Clone)]
pub struct PostgresDb {
    pool: Pool<Postgres>,
}

impl PostgresDb {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }
}

/// Create PostgreSQL connection pool
pub async fn create_pool(
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: &str,
    pool_size: u32,
) -> crate::Result<Pool<Postgres>> {
    use sqlx::postgres::PgPoolOptions;

    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        username, password, host, port, database
    );

    tracing::info!(
        "Connecting to PostgreSQL at {}:{}/{}...",
        host,
        port,
        database
    );

    let pool = PgPoolOptions::new()
        .max_connections(pool_size)
        .connect(&database_url)
        .await
        .map_err(crate::Error::Database)?;

    tracing::info!("PostgreSQL connected successfully");
    Ok(pool)
}
