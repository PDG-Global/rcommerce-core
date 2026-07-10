//! Media module for file handling and storage
//!
//! Provides file upload, storage, and retrieval services for digital products
//! and other media assets.

pub mod file_upload;

pub use file_upload::{FileMetadata, FileUploadService, S3Config, StorageBackend};
