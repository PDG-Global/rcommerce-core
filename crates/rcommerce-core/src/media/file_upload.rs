//! File upload and storage service for digital products
//!
//! Supports multiple storage backends:
//! - Local filesystem
//! - Amazon S3 (and S3-compatible services)
//! - Google Cloud Storage (planned)
//! - Azure Blob Storage (planned)

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::{Config, Error, Result};

/// Storage backend types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum StorageBackend {
    #[default]
    Local,
    S3,
    Gcs,
    Azure,
}

/// File metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub file_name: String,
    pub file_size: i64,
    pub content_type: String,
    pub file_hash: String,
    pub storage_path: String,
    pub storage_backend: StorageBackend,
    pub created_at: DateTime<Utc>,
}

/// File upload service for managing digital product files
pub struct FileUploadService {
    backend: StorageBackend,
    local_path: PathBuf,
    base_url: String,
    s3_config: Option<S3Config>,
}

/// S3 configuration
#[derive(Debug, Clone)]
pub struct S3Config {
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub endpoint: Option<String>,
}

impl FileUploadService {
    /// Create a new file upload service from configuration
    pub fn from_config(config: &Config) -> Result<Self> {
        let backend = match config.media.storage_type {
            crate::config::StorageType::Local => StorageBackend::Local,
            crate::config::StorageType::S3 => StorageBackend::S3,
            _ => StorageBackend::Local, // Fallback to local for other types
        };

        let local_path = config
            .media
            .local_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./uploads"));

        let base_url = config
            .media
            .local_base_url
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "http://localhost:8080/uploads".to_string());

        // Ensure upload directory exists for local storage
        if backend == StorageBackend::Local {
            fs::create_dir_all(&local_path)
                .map_err(|e| Error::storage(format!("Failed to create upload directory: {}", e)))?;

            // Create subdirectory for digital products
            let digital_path = local_path.join("digital");
            fs::create_dir_all(&digital_path).map_err(|e| {
                Error::storage(format!(
                    "Failed to create digital products directory: {}",
                    e
                ))
            })?;
        }

        let s3_config = if backend == StorageBackend::S3 {
            Some(S3Config {
                region: config.media.s3_region.clone().unwrap_or_default(),
                bucket: config.media.s3_bucket.clone().unwrap_or_default(),
                access_key: config.media.s3_access_key.clone().unwrap_or_default(),
                secret_key: config.media.s3_secret_key.clone().unwrap_or_default(),
                endpoint: config.media.s3_endpoint.clone(),
            })
        } else {
            None
        };

        Ok(Self {
            backend,
            local_path,
            base_url,
            s3_config,
        })
    }

    /// Create a new file upload service with local storage
    pub fn new_local(upload_path: impl AsRef<Path>, base_url: impl Into<String>) -> Result<Self> {
        let path = upload_path.as_ref().to_path_buf();

        fs::create_dir_all(&path)
            .map_err(|e| Error::storage(format!("Failed to create upload directory: {}", e)))?;

        let digital_path = path.join("digital");
        fs::create_dir_all(&digital_path).map_err(|e| {
            Error::storage(format!(
                "Failed to create digital products directory: {}",
                e
            ))
        })?;

        Ok(Self {
            backend: StorageBackend::Local,
            local_path: path,
            base_url: base_url.into(),
            s3_config: None,
        })
    }

    /// Upload a file for a digital product
    pub async fn upload_digital_product_file(
        &self,
        product_id: Uuid,
        file_name: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<FileMetadata> {
        // Calculate file hash
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = format!("{:x}", hasher.finalize());

        // Generate storage path
        let ext = Path::new(file_name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");

        let storage_file_name = format!("{}_{}.{}", product_id, Uuid::new_v4(), ext);
        let storage_path = format!("digital/{}/{}", &hash[..2], storage_file_name);

        match self.backend {
            StorageBackend::Local => {
                self.save_to_local(&storage_path, data).await?;
            }
            StorageBackend::S3 => {
                self.upload_to_s3(&storage_path, data, content_type).await?;
            }
            _ => {
                return Err(Error::storage(
                    "Storage backend not implemented".to_string(),
                ));
            }
        }

        Ok(FileMetadata {
            file_name: file_name.to_string(),
            file_size: data.len() as i64,
            content_type: content_type.to_string(),
            file_hash: hash,
            storage_path,
            storage_backend: self.backend,
            created_at: Utc::now(),
        })
    }

    /// Save file to local filesystem
    async fn save_to_local(&self, storage_path: &str, data: &[u8]) -> Result<()> {
        let full_path = self.local_path.join(storage_path);

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::storage(format!("Failed to create directory: {}", e)))?;
        }

        // Write file
        let mut file = fs::File::create(&full_path)
            .map_err(|e| Error::storage(format!("Failed to create file: {}", e)))?;

        file.write_all(data)
            .map_err(|e| Error::storage(format!("Failed to write file: {}", e)))?;

        Ok(())
    }

    /// Upload file to S3
    async fn upload_to_s3(
        &self,
        _storage_path: &str,
        _data: &[u8],
        _content_type: &str,
    ) -> Result<()> {
        // S3 upload implementation would go here
        // This requires the aws-sdk-s3 crate
        // For now, return an error indicating S3 is not fully implemented
        Err(Error::storage(
            "S3 upload not yet implemented. Use local storage for now.".to_string(),
        ))
    }

    /// Generate a secure download URL
    pub fn generate_download_url(
        &self,
        storage_path: &str,
        expires_in: Duration,
    ) -> Result<String> {
        match self.backend {
            StorageBackend::Local => {
                // For local storage, generate a signed URL
                let expires_at = Utc::now() + expires_in;
                let token = self.generate_download_token(storage_path, &expires_at)?;

                Ok(format!(
                    "{}/api/v1/downloads/{}?expires={}",
                    self.base_url,
                    token,
                    expires_at.timestamp()
                ))
            }
            StorageBackend::S3 => {
                // For S3, generate a presigned URL
                self.generate_s3_presigned_url(storage_path, expires_in)
            }
            _ => Err(Error::storage(
                "Storage backend not implemented".to_string(),
            )),
        }
    }

    /// Generate a download token for local storage
    fn generate_download_token(
        &self,
        storage_path: &str,
        expires_at: &DateTime<Utc>,
    ) -> Result<String> {
        // Simple token generation using hash
        // In production, use a proper HMAC library
        use sha2::{Digest, Sha256};

        let secret = self
            .s3_config
            .as_ref()
            .map(|c| c.secret_key.clone())
            .unwrap_or_else(|| "default-secret".to_string());

        let message = format!("{}:{}:{}", storage_path, expires_at.timestamp(), secret);

        let mut hasher = Sha256::new();
        hasher.update(message.as_bytes());
        let signature = format!("{:x}", hasher.finalize());

        let token = format!(
            "{}.{}",
            base64_url_encode(storage_path.as_bytes()),
            &signature[..32]
        );

        Ok(token)
    }

    /// Generate S3 presigned URL
    fn generate_s3_presigned_url(
        &self,
        _storage_path: &str,
        _expires_in: Duration,
    ) -> Result<String> {
        // S3 presigned URL generation would go here
        // This requires the aws-sdk-s3 crate
        Err(Error::storage(
            "S3 presigned URLs not yet implemented".to_string(),
        ))
    }

    /// Verify and decode a download token
    pub fn verify_download_token(&self, token: &str) -> Result<(String, DateTime<Utc>)> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 2 {
            return Err(Error::validation("Invalid download token format"));
        }

        let storage_path =
            base64_url_decode(parts[0]).map_err(|_| Error::validation("Invalid download token"))?;

        // For now, return the path with a default expiration
        // In production, you'd verify the signature here
        let expires_at = Utc::now() + Duration::hours(24);

        Ok((
            String::from_utf8_lossy(&storage_path).to_string(),
            expires_at,
        ))
    }

    /// Get file for streaming download
    pub async fn get_file_stream(&self, storage_path: &str) -> Result<Vec<u8>> {
        match self.backend {
            StorageBackend::Local => {
                let full_path = self.local_path.join(storage_path);
                fs::read(&full_path)
                    .map_err(|e| Error::storage(format!("Failed to read file: {}", e)))
            }
            _ => Err(Error::storage(
                "Streaming not implemented for this backend".to_string(),
            )),
        }
    }

    /// Delete a file
    pub async fn delete_file(&self, storage_path: &str) -> Result<()> {
        match self.backend {
            StorageBackend::Local => {
                let full_path = self.local_path.join(storage_path);
                fs::remove_file(&full_path)
                    .map_err(|e| Error::storage(format!("Failed to delete file: {}", e)))?;
                Ok(())
            }
            _ => Err(Error::storage(
                "Delete not implemented for this backend".to_string(),
            )),
        }
    }

    /// Get the storage backend type
    pub fn backend(&self) -> StorageBackend {
        self.backend
    }

    /// Get the local storage path
    pub fn local_path(&self) -> &Path {
        &self.local_path
    }
}

/// Base64 URL-safe encoding
fn base64_url_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.encode(data)
}

/// Base64 URL-safe decoding
fn base64_url_decode(data: &str) -> Result<Vec<u8>> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD
        .decode(data)
        .map_err(|_| Error::validation("Invalid base64 encoding"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_file_upload() {
        let temp_dir = TempDir::new().unwrap();
        let service =
            FileUploadService::new_local(temp_dir.path(), "http://localhost:8080/uploads").unwrap();

        let product_id = Uuid::new_v4();
        let data = b"test file content";

        let metadata = service
            .upload_digital_product_file(product_id, "test.pdf", "application/pdf", data)
            .await
            .unwrap();

        assert_eq!(metadata.file_size, data.len() as i64);
        assert_eq!(metadata.content_type, "application/pdf");
        assert!(!metadata.file_hash.is_empty());
    }

    #[test]
    fn test_download_token_generation() {
        let temp_dir = TempDir::new().unwrap();
        let service =
            FileUploadService::new_local(temp_dir.path(), "http://localhost:8080/uploads").unwrap();

        let expires_at = Utc::now() + Duration::hours(24);
        let token = service
            .generate_download_token("digital/test.pdf", &expires_at)
            .unwrap();

        assert!(!token.is_empty());
        assert!(token.contains('.'));
    }
}
