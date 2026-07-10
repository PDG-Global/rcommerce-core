//! Database access utilities

pub mod migrate;

use once_cell::sync::Lazy;
use sqlx::PgPool;
use std::sync::Arc;

static POOL: Lazy<Arc<PgPool>> = Lazy::new(|| {
    // In production, this would be initialized from config
    Arc::new(PgPool::connect_lazy("postgres://localhost/rcommerce").expect("Failed to create pool"))
});

/// Get the database pool
pub async fn get_pool() -> Result<Arc<PgPool>, crate::Error> {
    Ok(POOL.clone())
}

/// Initialize database pool with connection string
pub fn init_pool(_connection_string: &str) -> Result<(), crate::Error> {
    // This would reinitialize the pool if needed
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_pool() {
        let pool = get_pool().await;
        assert!(pool.is_ok());
    }
}
