//! API Key Repository
//!
//! Database operations for API key management including:
//! - Finding API keys by prefix
//! - Validating API keys
//! - Updating last used timestamp
//! - Listing, creating, revoking API keys

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Error, Result};

/// API Key record from database
#[derive(Debug, Clone, FromRow)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub customer_id: Option<Uuid>,
    pub key_prefix: String,
    pub key_hash: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_used_ip: Option<String>,
    pub rate_limit_per_minute: Option<i32>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
}

/// API Key repository trait for database operations
#[async_trait]
pub trait ApiKeyRepository: Send + Sync + 'static {
    /// Find API key by prefix
    async fn find_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>>;

    /// Find API key by prefix (only active, non-expired keys)
    async fn find_active_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>>;

    /// Update last used timestamp and IP
    async fn update_last_used(&self, id: Uuid, ip: Option<&str>) -> Result<()>;

    /// List all API keys for a customer
    async fn list_by_customer(&self, customer_id: Uuid) -> Result<Vec<ApiKeyRecord>>;

    /// List all API keys (admin only)
    async fn list_all(&self) -> Result<Vec<ApiKeyRecord>>;

    /// Create a new API key
    async fn create(&self, record: CreateApiKeyRequest) -> Result<ApiKeyRecord>;

    /// Revoke an API key
    async fn revoke(&self, prefix: &str, reason: Option<&str>) -> Result<bool>;

    /// Delete an API key permanently
    async fn delete(&self, prefix: &str) -> Result<bool>;

    /// Verify if an API key is valid and return its record
    async fn verify_key(&self, full_key: &str) -> Result<Option<ApiKeyRecord>>;
}

/// Request to create a new API key
#[derive(Debug, Clone)]
pub struct CreateApiKeyRequest {
    pub customer_id: Option<Uuid>,
    pub key_prefix: String,
    pub key_hash: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub rate_limit_per_minute: Option<i32>,
}

/// PostgreSQL implementation of API key repository
#[derive(Clone)]
pub struct PostgresApiKeyRepository {
    pool: sqlx::Pool<sqlx::Postgres>,
}

impl PostgresApiKeyRepository {
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyRepository for PostgresApiKeyRepository {
    async fn find_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>> {
        let record = sqlx::query_as::<_, ApiKeyRecord>(
            r#"
            SELECT * FROM api_keys 
            WHERE key_prefix = $1
            "#,
        )
        .bind(prefix)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(record)
    }

    async fn find_active_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>> {
        let record = sqlx::query_as::<_, ApiKeyRecord>(
            r#"
            SELECT * FROM api_keys 
            WHERE key_prefix = $1 
            AND is_active = true 
            AND revoked_at IS NULL
            AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        )
        .bind(prefix)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(record)
    }

    async fn update_last_used(&self, id: Uuid, ip: Option<&str>) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE api_keys 
            SET last_used_at = NOW(), last_used_ip = $1
            WHERE id = $2
            "#,
        )
        .bind(ip)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn list_by_customer(&self, customer_id: Uuid) -> Result<Vec<ApiKeyRecord>> {
        let records = sqlx::query_as::<_, ApiKeyRecord>(
            r#"
            SELECT * FROM api_keys 
            WHERE customer_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(customer_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(records)
    }

    async fn list_all(&self) -> Result<Vec<ApiKeyRecord>> {
        let records = sqlx::query_as::<_, ApiKeyRecord>(
            r#"
            SELECT * FROM api_keys 
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(records)
    }

    async fn create(&self, request: CreateApiKeyRequest) -> Result<ApiKeyRecord> {
        let record = sqlx::query_as::<_, ApiKeyRecord>(
            r#"
            INSERT INTO api_keys (
                customer_id, key_prefix, key_hash, name, scopes, 
                expires_at, rate_limit_per_minute
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(request.customer_id)
        .bind(request.key_prefix)
        .bind(request.key_hash)
        .bind(request.name)
        .bind(request.scopes)
        .bind(request.expires_at)
        .bind(request.rate_limit_per_minute)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(record)
    }

    async fn revoke(&self, prefix: &str, reason: Option<&str>) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE api_keys 
            SET is_active = false, revoked_at = NOW(), revoked_reason = $1
            WHERE key_prefix = $2
            "#,
        )
        .bind(reason)
        .bind(prefix)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete(&self, prefix: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM api_keys 
            WHERE key_prefix = $1
            "#,
        )
        .bind(prefix)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    async fn verify_key(&self, full_key: &str) -> Result<Option<ApiKeyRecord>> {
        // Extract prefix from full key (format: prefix.secret)
        let parts: Vec<&str> = full_key.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Ok(None);
        }

        let prefix = parts[0];

        // Find the key by prefix
        let record = self.find_active_by_prefix(prefix).await?;

        match record {
            Some(key) => {
                // Verify the hash matches
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(full_key.as_bytes());
                let hash = hasher.finalize();
                let hash_hex = hex::encode(hash);

                if hash_hex == key.key_hash {
                    Ok(Some(key))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
}

/// In-memory API key repository for testing
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    pub struct MockApiKeyRepository {
        keys: Mutex<HashMap<String, ApiKeyRecord>>,
    }

    impl Default for MockApiKeyRepository {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockApiKeyRepository {
        pub fn new() -> Self {
            Self {
                keys: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl ApiKeyRepository for MockApiKeyRepository {
        async fn find_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>> {
            let keys = self.keys.lock().unwrap();
            Ok(keys.get(prefix).cloned())
        }

        async fn find_active_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>> {
            let keys = self.keys.lock().unwrap();
            Ok(keys
                .get(prefix)
                .filter(|k| k.is_active && k.revoked_at.is_none())
                .cloned())
        }

        async fn update_last_used(&self, id: Uuid, ip: Option<&str>) -> Result<()> {
            let mut keys = self.keys.lock().unwrap();
            for key in keys.values_mut() {
                if key.id == id {
                    key.last_used_at = Some(Utc::now());
                    key.last_used_ip = ip.map(|s| s.to_string());
                    break;
                }
            }
            Ok(())
        }

        async fn list_by_customer(&self, customer_id: Uuid) -> Result<Vec<ApiKeyRecord>> {
            let keys = self.keys.lock().unwrap();
            Ok(keys
                .values()
                .filter(|k| k.customer_id == Some(customer_id))
                .cloned()
                .collect())
        }

        async fn list_all(&self) -> Result<Vec<ApiKeyRecord>> {
            let keys = self.keys.lock().unwrap();
            Ok(keys.values().cloned().collect())
        }

        async fn create(&self, request: CreateApiKeyRequest) -> Result<ApiKeyRecord> {
            let mut keys = self.keys.lock().unwrap();
            let record = ApiKeyRecord {
                id: Uuid::new_v4(),
                customer_id: request.customer_id,
                key_prefix: request.key_prefix.clone(),
                key_hash: request.key_hash,
                name: request.name,
                scopes: request.scopes,
                expires_at: request.expires_at,
                last_used_at: None,
                last_used_ip: None,
                rate_limit_per_minute: request.rate_limit_per_minute,
                is_active: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                revoked_at: None,
                revoked_reason: None,
            };
            keys.insert(request.key_prefix, record.clone());
            Ok(record)
        }

        async fn revoke(&self, prefix: &str, reason: Option<&str>) -> Result<bool> {
            let mut keys = self.keys.lock().unwrap();
            if let Some(key) = keys.get_mut(prefix) {
                key.is_active = false;
                key.revoked_at = Some(Utc::now());
                key.revoked_reason = reason.map(|s| s.to_string());
                Ok(true)
            } else {
                Ok(false)
            }
        }

        async fn delete(&self, prefix: &str) -> Result<bool> {
            let mut keys = self.keys.lock().unwrap();
            Ok(keys.remove(prefix).is_some())
        }

        async fn verify_key(&self, full_key: &str) -> Result<Option<ApiKeyRecord>> {
            let parts: Vec<&str> = full_key.splitn(2, '.').collect();
            if parts.len() != 2 {
                return Ok(None);
            }
            self.find_active_by_prefix(parts[0]).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::MockApiKeyRepository;
    use super::*;

    #[tokio::test]
    async fn test_mock_create_and_find() {
        let repo = MockApiKeyRepository::new();

        let request = CreateApiKeyRequest {
            customer_id: None,
            key_prefix: "test123".to_string(),
            key_hash: "hash123".to_string(),
            name: "Test Key".to_string(),
            scopes: vec!["read".to_string()],
            expires_at: None,
            rate_limit_per_minute: None,
        };

        let created = repo.create(request).await.unwrap();
        assert_eq!(created.key_prefix, "test123");
        assert_eq!(created.scopes, vec!["read"]);

        let found = repo.find_by_prefix("test123").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Test Key");
    }

    #[tokio::test]
    async fn test_mock_revoke() {
        let repo = MockApiKeyRepository::new();

        let request = CreateApiKeyRequest {
            customer_id: None,
            key_prefix: "test123".to_string(),
            key_hash: "hash123".to_string(),
            name: "Test Key".to_string(),
            scopes: vec!["read".to_string()],
            expires_at: None,
            rate_limit_per_minute: None,
        };

        repo.create(request).await.unwrap();

        // Revoke the key
        let revoked = repo
            .revoke("test123", Some("Test revocation"))
            .await
            .unwrap();
        assert!(revoked);

        // Should not find active key
        let active = repo.find_active_by_prefix("test123").await.unwrap();
        assert!(active.is_none());
    }
}
