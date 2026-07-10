/// Core repository trait - simplified
use async_trait::async_trait;

use crate::Result;

/// Repository pattern trait for basic CRUD operations
#[async_trait]
pub trait Repository<T, ID>: Send + Sync {
    /// Find entity by ID
    async fn find_by_id(&self, id: ID) -> Result<Option<T>>;

    /// List entities
    async fn list(&self) -> Result<Vec<T>>;

    /// Create new entity
    async fn create(&self, entity: T) -> Result<T>;

    /// Update existing entity
    async fn update(&self, entity: T) -> Result<T>;

    /// Delete entity by ID
    async fn delete(&self, id: ID) -> Result<bool>;
}

/// Service pattern trait for business logic
#[async_trait]
pub trait Service: Send + Sync {
    /// Health check for the service
    async fn health_check(&self) -> Result<()>;
}
