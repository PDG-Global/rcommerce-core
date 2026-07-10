# Media & File Storage Architecture

## Overview

The Media & File Storage system provides a unified, provider-agnostic interface for storing and serving various types of files including product images, digital downloads, invoices, shipping labels, and other business assets. The system supports multiple storage backends with automatic fallbacks, CDN integration, and optimized delivery.

**Supported File Types:**
- Product images (JPEG, PNG, WebP, AVIF)
- Digital products (PDFs, ZIP files, software)
- Invoices and receipts (PDF)
- Shipping labels (PDF, PNG)
- Import/Export files (CSV, XLSX)
- Customer uploaded files
- Brand assets (logos, icons)

**Storage Backends:**
- Local filesystem
- S3-compatible storage (AWS S3, MinIO, Wasabi, DigitalOcean Spaces)
- Cloud storage (Google Cloud Storage, Azure Blob Storage)
- Hybrid configurations

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     API/Media Controller Layer                   │
│  - Upload endpoints                                            │
│  - Download endpoints                                          │
│  - Delete endpoints                                            │
│  - Transform endpoints (resize, crop, etc.)                    │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                   Media Service Layer                          │
│  - Provider abstraction                                        │
│  - File validation                                             │
│  - Image optimization                                          │
│  - File lifecycle management                                   │
│  - CDN integration                                             │
│  - Cache management                                            │
└────────────┬──────────────────────┬─────────────────────┬────────┘
             │                      │                     │
    ┌────────▼─────────┐   ┌────────▼────────┐   ┌──────▼───────┐
    │   Local Storage  │   │   S3 Storage    │   │  GCP/Azure   │
    │   (dev/small)    │   │   (production)  │   │   (multi)    │
    │                  │   │                 │   │              │
    │ - Fast, simple   │   │ - Scalable      │   │ - Regional   │
    │ - No network     │   │ - CDN-ready     │   │ - Enterprise │
    │ - Limited space  │   │ - Cost-effective│   │ - SLA        │
    └────────┬─────────┘   └────────┬────────┘   └──────┬───────┘
             │                      │                   │
             └──────────────────────┼───────────────────┘
                                    │
┌───────────────────────────────────▼─────────────────────────────┐
│                          CDN Layer                               │
│  - CloudFront, Cloudflare, Fastly                               │
│  - Automatic image optimization                                 │
│  - Geographic distribution                                      │
│  - Caching strategies                                           │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### File Entity

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub id: Uuid,
    pub entity_type: String,        // "product_image", "digital_product", "invoice"
    pub entity_id: Uuid,            // ID of the related entity
    pub original_name: String,      // Original filename
    pub stored_name: String,        // Storage system filename
    pub file_type: FileType,        // Image, PDF, etc.
    pub mime_type: String,          // MIME type
    pub size_bytes: i64,            // File size in bytes
    pub storage_backend: String,    // "local", "s3", "gcs", "azure"
    pub storage_path: String,       // Path in storage system
    pub public_url: String,         // Publicly accessible URL
    pub cdn_url: Option<String>,    // CDN-optimized URL
    pub checksum: String,           // SHA256 checksum
    pub metadata: FileMetadata,     // Additional metadata
    pub transformations: Vec<Transformation>, // Applied transformations
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>, // For temporary files
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "file_type", rename_all = "snake_case")]
pub enum FileType {
    Image,
    Pdf,
    Document,  // DOC, DOCX, etc.
    Spreadsheet, // XLS, XLSX, etc.
    Archive,   // ZIP, TAR, etc.
    Video,
    Audio,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub width: Option<i32>,          // Image width
    pub height: Option<i32>,         // Image height
    pub original_format: Option<String>, // Original format (JPEG, PNG, etc.)
    pub dominant_color: Option<String>, // For placeholder images
    pub alt_text: Option<String>,    // Accessibility
    pub title: Option<String>,       // SEO title
    pub caption: Option<String>,     // Image caption
    pub tags: Vec<String>,           // Search tags
    pub is_optimized: bool,          // Has been optimized for web
    pub is_processed: bool,          // Has transformations applied
    pub optimization_quality: Option<i32>, // JPEG quality (1-100)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transformation {
    pub id: String,                  // Unique transformation ID
    pub operation: String,           // "resize", "crop", "rotate", "format"
    pub params: serde_json::Value,   // Operation parameters
    pub created_at: DateTime<Utc>,
    pub output_file_id: Option<Uuid>, // Transformed file reference
}
```

### Storage Provider Trait

```rust
#[async_trait]
pub trait StorageProvider: Send + Sync + 'static {
    fn id(&self) -> &'static str;  // "local", "s3", "gcs", "azure"
    
    fn name(&self) -> &'static str;
    
    /// Upload a file from local path
    async fn upload_file(
        &self,
        source_path: &Path,
        destination_path: &str,
        mime_type: &str,
        metadata: Option<FileMetadata>,
    ) -> Result<UploadResult>;
    
    /// Upload from bytes (for in-memory files)
    async fn upload_bytes(
        &self,
        data: &[u8],
        destination_path: &str,
        mime_type: &str,
        metadata: Option<FileMetadata>,
    ) -> Result<UploadResult>;
    
    /// Download file to local path
    async fn download_file(&self, path: &str, destination: &Path) -> Result<()>;
    
    /// Download file as bytes
    async fn download_bytes(&self, path: &str) -> Result<Vec<u8>>;
    
    /// Delete file
    async fn delete_file(&self, path: &str) -> Result<bool>;
    
    /// Check if file exists
    async fn file_exists(&self, path: &str) -> Result<bool>;
    
    /// File size in bytes
    async fn file_size(&self, path: &str) -> Result<i64>;
    
    /// List files with prefix
    async fn list_files(&self, prefix: &str) -> Result<Vec<String>>;
    
    /// Generate signed URL for temporary access
    async fn generate_signed_url(
        &self,
        path: &str,
        expires_in: Duration,
    ) -> Result<String>;
    
    /// Get direct access URL
    fn get_public_url(&self, path: &str) -> String;
    
    /// Health check
    async fn health_check(&self) -> Result<bool>;
}

#[derive(Debug, Clone)]
pub struct UploadResult {
    pub storage_path: String,
    pub public_url: String,
    pub size_bytes: i64,
    pub checksum: String,
}
```

## Storage Implementations

### Local Filesystem Provider

```rust
pub struct LocalStorageProvider {
    base_path: PathBuf,
    base_url: String,
}

impl LocalStorageProvider {
    pub fn new(base_path: PathBuf, base_url: String) -> Result<Self> {
        // Ensure base directory exists
        fs::create_dir_all(&base_path)?;
        
        Ok(Self {
            base_path,
            base_url,
        })
    }
    
    fn full_path(&self, relative_path: &str) -> PathBuf {
        self.base_path.join(relative_path)
    }
}

#[async_trait]
impl StorageProvider for LocalStorageProvider {
    fn id(&self) -> &'static str { "local" }
    fn name(&self) -> &'static str { "Local Filesystem" }
    
    async fn upload_file(
        &self,
        source_path: &Path,
        destination_path: &str,
        mime_type: &str,
        _metadata: Option<FileMetadata>,
    ) -> Result<UploadResult> {
        let full_dest_path = self.full_path(destination_path);
        
        // Create parent directories
        if let Some(parent) = full_dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Copy file
        tokio::fs::copy(source_path, &full_dest_path).await?;
        
        // Calculate checksum
        let file_bytes = fs::read(&full_dest_path)?;
        let checksum = format!("{:x}", Sha256::digest(&file_bytes));
        
        let metadata = tokio::fs::metadata(full_dest_path).await?;
        let size_bytes = metadata.len() as i64;
        
        let public_url = format!("{}/{}", self.base_url, destination_path);
        
        Ok(UploadResult {
            storage_path: destination_path.to_string(),
            public_url,
            size_bytes,
            checksum,
        })
    }
    
    async fn upload_bytes(
        &self,
        data: &[u8],
        destination_path: &str,
        _mime_type: &str,
        _metadata: Option<FileMetadata>,
    ) -> Result<UploadResult> {
        let full_dest_path = self.full_path(destination_path);
        
        if let Some(parent) = full_dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        tokio::fs::write(&full_dest_path, data).await?;
        
        let checksum = format!("{:x}", Sha256::digest(data));
        let size_bytes = data.len() as i64;
        
        let public_url = format!("{}/{}", self.base_url, destination_path);
        
        Ok(UploadResult {
            storage_path: destination_path.to_string(),
            public_url,
            size_bytes,
            checksum,
        })
    }
    
    async fn download_file(&self, path: &str, destination: &Path) -> Result<()> {
        let full_src_path = self.full_path(path);
        tokio::fs::copy(full_src_path, destination).await?;
        Ok(())
    }
    
    async fn delete_file(&self, path: &str) -> Result<bool> {
        let full_path = self.full_path(path);
        
        match tokio::fs::remove_file(full_path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    
    fn get_public_url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path)
    }
    
    async fn health_check(&self) -> Result<bool> {
        Ok(fs::metadata(&self.base_path).is_ok())
    }
}
```

### S3-Compatible Provider

```rust
pub struct S3StorageProvider {
    client: aws_sdk_s3::Client,
    bucket: String,
    region: String,
    public_endpoint: String,
    cdn_endpoint: Option<String>,
}

impl S3StorageProvider {
    pub async fn new(
        region: String,
        bucket: String,
        access_key: String,
        secret_key: String,
        endpoint: Option<String>,  // For MinIO, Wasabi, etc.
    ) -> Result<Self> {
        let config_builder = aws_config::from_env()
            .region(aws_sdk_s3::config::Region::new(region.clone()));
        
        // Support S3-compatible endpoints (MinIO, Wasabi, etc.)
        let config = if let Some(endpoint_url) = endpoint {
            config_builder
                .endpoint_url(endpoint_url)
                .load()
                .await
        } else {
            config_builder.load().await
        };
        
        let sdk_config = aws_sdk_s3::config::Builder::from(&config)
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                access_key,
                secret_key,
                None,
                None,
                "static",
            ))
            .build();
        
        let client = aws_sdk_s3::Client::from_conf(sdk_config);
        
        // Verify bucket exists (or create it)
        match client.create_bucket().bucket(&bucket).send().await {
            Ok(_) => println!("Created S3 bucket: {}", bucket),
            Err(e) if e.as_service_error().map(|err| err.is_bucket_already_exists()).unwrap_or(false) => {
                // Bucket already exists
            }
            Err(e) => return Err(e.into()),
        }
        
        Ok(Self {
            client,
            bucket,
            region,
            public_endpoint: format!("https://{}.s3.amazonaws.com", bucket),
            cdn_endpoint: None,
        })
    }
    
    pub fn with_cdn(&mut self, cdn_endpoint: String) {
        self.cdn_endpoint = Some(cdn_endpoint);
    }
    
    fn object_url(&self, key: &str) -> String {
        if key.starts_with("http") {
            return key.to_string();
        }
        
        if let Some(cdn) = &self.cdn_endpoint {
            format!("{}/{}", cdn, key)
        } else if key.contains("://") {
            key.to_string()
        } else {
            format!("{}/{}", self.public_endpoint, key)
        }
    }
}

#[async_trait]
impl StorageProvider for S3StorageProvider {
    fn id(&self) -> &'static str { "s3" }
    fn name(&self) -> &'static str { "Amazon S3" }
    
    async fn upload_bytes(
        &self,
        data: &[u8],
        destination_path: &str,
        mime_type: &str,
        metadata: Option<FileMetadata>,
    ) -> Result<UploadResult> {
        let checksum = format!("{:x}", Sha256::digest(data));
        
        // Build metadata if provided
        let mut s3_metadata = HashMap::new();
        s3_metadata.insert("checksum-sha256".to_string(), checksum.clone());
        
        if let Some(file_meta) = metadata {
            if let Some(width) = file_meta.width {
                s3_metadata.insert("width".to_string(), width.to_string());
            }
            if let Some(height) = file_meta.height {
                s3_metadata.insert("height".to_string(), height.to_string());
            }
        }
        
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(destination_path)
            .content_type(mime_type)
            .set_metadata(Some(s3_metadata))
            .body(data.into())
            .send()
            .await?;
        
        let size_bytes = data.len() as i64;
        let public_url = self.object_url(destination_path);
        
        Ok(UploadResult {
            storage_path: destination_path.to_string(),
            public_url,
            size_bytes,
            checksum,
        })
    }
    
    async fn upload_file(
        &self,
        source_path: &Path,
        destination_path: &str,
        mime_type: &str,
        metadata: Option<FileMetadata>,
    ) -> Result<UploadResult> {
        let file_bytes = fs::read(source_path)?;
        self.upload_bytes(&file_bytes, destination_path, mime_type, metadata).await
    }
    
    async fn delete_file(&self, path: &str) -> Result<bool> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await?;
        
        Ok(true)
    }
    
    async fn download_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let resp = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await?;
        
        let data = resp.body.collect().await?.into_bytes();
        Ok(data.to_vec())
    }
    
    fn get_public_url(&self, path: &str) -> String {
        self.object_url(path)
    }
    
    async fn generate_signed_url(
        &self,
        path: &str,
        expires_in: Duration,
    ) -> Result<String> {
        let expires_at = SystemTime::now() + expires_in;
        
        let presigned_request = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(path)
            .presigned(PresigningConfig::expires_in(expires_in.as_secs())?)
            .await?;
        
        Ok(presigned_request.uri().to_string())
    }
    
    async fn health_check(&self) -> Result<bool> {
        self.client.head_bucket().bucket(&self.bucket).send().await?;
        Ok(true)
    }
}
```

### Image Processing with Storage

```rust
pub struct ImageProcessor {
    storage_provider: Arc<dyn StorageProvider>,
    cdn_config: Option<CdnConfig>,
}

impl ImageProcessor {
    pub async fn upload_and_optimize_product_image(
        &self,
        product_id: Uuid,
        image_data: &[u8],
        filename: &str,
    ) -> Result<File> {
        // Validate image
        let img = image::load_from_memory(image_data)?;
        let (width, height) = img.dimensions();
        
        // Generate optimized versions
        let sizes = vec![
            ("thumbnail", 150, 150),
            ("small", 300, 300),
            ("medium", 600, 600),
            ("large", 1200, 1200),
            ("original", width, height),
        ];
        
        let mut files = Vec::new();
        
        for (size_name, target_width, target_height) in sizes {
            let resized = img.resize(target_width, target_height, image::imageops::FilterType::Lanczos3);
            
            // Convert to WebP for web optimization
            let mut buffer = Vec::new();
 resized.write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::WebP)?;
            
            // Generate unique filename
            let extension = Path::new(filename).extension().unwrap_or_default();
            let mut new_filename = filename.to_string();
            new_filename.insert_str(filename.rfind('.').unwrap_or(filename.len()), &format!("_{}", size_name));
            
            let storage_path = format!("products/{}/{}", product_id, new_filename);
            
            // Upload to storage
            let upload_result = self.storage_provider
                .upload_bytes(&buffer, &storage_path, "image/webp", Some(FileMetadata {
                    width: Some(target_width as i32),
                    height: Some(target_height as i32),
                    original_format: Some(extension.to_string_lossy().to_string()),
                    is_optimized: true,
                    ..Default::default()
 }))
                .await?;
            
            files.push(upload_result);
        }
        
        // Create primary file record for original
        let file = File {
            id: Uuid::new_v4(),
            entity_type: "product_image".to_string(),
            entity_id: product_id,
            original_name: filename.to_string(),
            stored_name: filename.to_string(),
            file_type: FileType::Image,
            mime_type: "image/webp".to_string(),
            size_bytes: files[4].size_bytes, // Original size
            storage_backend: self.storage_provider.id().to_string(),
            storage_path: format!("products/{}/{}", product_id, filename),
            public_url: files[4].public_url.clone(),
            cdn_url: None,
            checksum: files[4].checksum.clone(),
            metadata: FileMetadata {
                width: Some(width as i32),
                height: Some(height as i32),
                is_optimized: true,
                ..Default::default()
            },
            transformations: sizes.iter().enumerate().map(|(i, (name, w, h))| {
                Transformation {
                    id: format!("{}_{}", product_id, name),
                    operation: "resize".to_string(),
                    params: json!({"width": w, "height": h}),
                    created_at: Utc::now(),
                    output_file_id: Some(files[i].storage_path.parse().unwrap()),
                }
            }).collect(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
        };
        
        Ok(file)
    }
}
```

## File Organization Strategy

### Structured Storage Paths

```rust
impl FileService {
    fn generate_storage_path(
        &self,
        file_type: &str,
        entity_id: Uuid,
        filename: &str,
    ) -> String {
        let timestamp = Utc::now().format("%Y/%m/%d");
        let extension = Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("dat");
        
        // Generate unique filename
        let unique_id = Uuid::new_v4().to_string()[0..8];
        let safe_filename = sanitize_filename(filename);
        
        match file_type {
            "product_image" => format!(
                "products/{}/{}_{}_{}.webp",
                entity_id,
                timestamp,
                unique_id,
                safe_filename
            ),
            "digital_product" => format!(
                "digital/{}/{}/{}_{}_{}",
                entity_id,
                timestamp,
                unique_id,
                safe_filename
            ),
            "invoice" => format!(
                "invoices/{}/{}_{}_{}.pdf",
                entity_id,
                timestamp,
                entity_id,
                unique_id
            ),
            "shipping_label" => format!(
                "labels/{}/{}/{}_{}.pdf",
                entity_id,
                timestamp,
                unique_id
            ),
            _ => format!(
                "misc/{}/{}/{}_{}",
                file_type,
                timestamp,
                unique_id,
                safe_filename
            ),
        }
    }
}

fn sanitize_filename(filename: &str) -> String {
    // Remove path separators and dangerous characters
    filename
        .replace('/', "_")
        .replace('\\', "_")
        .replace('..', "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}
```

## CDN Integration

### CDN Configuration

```toml
[cdn]
enabled = true
provider = "cloudflare"  # "cloudflare", "cloudfront", "fastly", "bunny"

# Cloudflare configuration
[cdn.cloudflare]
api_token = "YOUR_API_TOKEN"
zone_id = "YOUR_ZONE_ID"

# CloudFront configuration
[cdn.cloudfront]
distribution_id = "YOUR_DISTRIBUTION_ID"
access_key_id = "YOUR_AWS_ACCESS_KEY"
secret_access_key = "YOUR_AWS_SECRET_KEY"

# Fastly configuration
[cdn.fastly]
service_id = "YOUR_SERVICE_ID"
api_token = "YOUR_FASTLY_API_TOKEN"
```

### Automatic CDN URL Generation

```rust
impl FileService {
    pub async fn handle_file_upload(
        &self,
        mut file: File,
    ) -> Result<File> {
        // Upload to storage provider
        let upload_result = self.storage_provider.upload_bytes(
            /* data */,
            &file.storage_path,
            &file.mime_type,
            Some(file.metadata.clone()),
        ).await?;
        
        // Set public URL
        file.public_url = upload_result.public_url;
        
        // Generate CDN URL if configured
        if let Some(cdn) = &self.cdn_config {
            file.cdn_url = Some(self.generate_cdn_url(&upload_result.public_url));
            
            // Purge old cache if updating
            if file.updated_at != file.created_at {
                cdn.purge_cache(&file.public_url).await?;
            }
        }
        
        // Save file record
        self.file_repository.save(file.clone()).await?;
        
        Ok(file)
    }
    
    fn generate_cdn_url(&self, origin_url: &str) -> String {
        match self.cdn_config.as_ref().map(|c| &c.provider) {
            Some("cloudflare") => {
                // Transform: https://bucket.s3.amazonaws.com/path
                // To: https://cdn.example.com/path
                let path = origin_url.split(".s3.amazonaws.com/").nth(1)
                    .or_else(|| origin_url.split(".com/").nth(1))
                    .unwrap_or(origin_url);
                format!("https://cdn.example.com/{}", path)
            }
            Some("cloudfront") => {
                // CloudFront uses the same path structure
                let path = origin_url.split(".amazonaws.com/").nth(1)
                    .unwrap_or(origin_url);
                format!("https://{}.cloudfront.net/{}", 
                    self.cdn_config.as_ref().unwrap().distribution_id, path)
            }
            _ => origin_url.to_string(),
        }
    }
}
```

## Configuration

```toml
[media]
# Default storage provider
storage_provider = "s3"  # "local", "s3", "gcs", "azure"

# Image processing
[media.image_processing]
enabled = true
auto_optimize = true  # Automatically optimize images on upload
default_quality = 85  # JPEG/WebP quality (1-100)
generate_sizes = ["thumbnail", "small", "medium", "large"]

# Thumbnail sizes (width, height)
[media.image_processing.sizes]
thumbnail = { width = 150, height = 150, crop = true }
small = { width = 300, height = 300 }
medium = { width = 600, height = 600 }
large = { width = 1200, height = 1200 }
original = { preserve = true }

# Allowed file types
[media.allowed_types]
images = ["jpg", "jpeg", "png", "gif", "webp", "avif"]
documents = ["pdf", "doc", "docx", "xls", "xlsx"]
archives = ["zip", "tar", "gz"]
max_file_size = "10MB"
max_image_size = "5MB"

# Local filesystem config
[media.local]
base_path = "./uploads"
base_url = "http://localhost:8080/uploads"

# S3 configuration
[media.s3]
region = "us-east-1"
bucket = "rcommerce-media"
access_key_id = "${AWS_ACCESS_KEY_ID}"
secret_access_key = "${AWS_SECRET_ACCESS_KEY}"
endpoint = "${S3_ENDPOINT}"  # For MinIO, Wasabi, etc.
presigned_url_expiry = "1h"

# CDN configuration
[media.cdn]
enabled = true
provider = "cloudflare"
api_token = "${CLOUDFLARE_API_TOKEN}"
zone_id = "${CLOUDFLARE_ZONE_ID}"

cache_ttl = "1d"  # Default cache TTL
auto_purge = true  # Purge cache on file update
```

## File Upload API

```rust
// API handler for file uploads
pub async fn upload_product_images(
    State(file_service): State<Arc<FileService>>,
    State(media_validator): State<MediaValidator>,
    Path(product_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<Vec<File>>, ApiError> {
    let mut uploaded_files = Vec::new();
    
    while let Some(field) = multipart.next_field().await? {
        let field_name = field.name().map(|s| s.to_string());
        let file_name = field.file_name().map(|s| s.to_string());
        let content_type = field.content_type().map(|s| s.to_string());
        
        // Validate file
        let bytes = field.bytes().await?;
        media_validator.validate(&bytes, &content_type)?;
        
        // Generate storage path
        let storage_path = file_service.generate_storage_path(
            "product_image",
            product_id,
            file_name.as_ref().unwrap_or(&"upload".to_string()),
        );
        
        // Upload to storage
        let metadata = FileMetadata {
            width: None, // Will be populated by image processor
            height: None,
            original_format: Some(Path::new(file_name.as_ref().unwrap()).extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string()),
            is_optimized: false,
            ..Default::default()
        };
        
        let upload_result = file_service.storage_provider
            .upload_bytes(&bytes, &storage_path, &content_type.unwrap_or_default(), Some(metadata))
            .await?;
        
        // Create file record
        let file = File {
            id: Uuid::new_v4(),
            entity_type: "product_image".to_string(),
            entity_id: product_id,
            original_name: file_name.unwrap_or_else(|| "upload".to_string()),
            stored_name: Path::new(&storage_path).file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upload")
                .to_string(),
            file_type: FileType::Image,
            mime_type: content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
            size_bytes: bytes.len() as i64,
            storage_backend: file_service.storage_provider.id().to_string(),
            storage_path,
            public_url: upload_result.public_url,
            cdn_url: None,
            checksum: upload_result.checksum,
            metadata: FileMetadata {
                is_optimized: true,
                ..Default::default()
            },
            transformations: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
        };
        
        // Save to database
        file_service.file_repository.save(file.clone()).await?;
        uploaded_files.push(file);
    }
    
    Ok(Json(uploaded_files))
}
```

## Digital Products

For digital products (software, PDFs, etc.):

```rust
impl FileService {
    pub async fn upload_digital_product(
        &self,
        product_id: Uuid,
        file_data: &[u8],
        filename: &str,
        mime_type: &str,
    ) -> Result<File> {
        // Validate secure file types
        let allowed_types = [
            "application/pdf",
            "application/zip",
            "application/x-zip-compressed",
        ];
        
        if !allowed_types.contains(&mime_type) {
            return Err(MediaError::InvalidFileType(mime_type.to_string()));
        }
        
        // Scan for viruses (optional but recommended)
        #[cfg(feature = "virus_scan")]
        self.scan_for_malware(file_data).await?;
        
        let storage_path = self.generate_storage_path(
            "digital_product",
            product_id,
            filename,
        );
        
        let upload_result = self.storage_provider
            .upload_bytes(file_data, &storage_path, mime_type, None)
            .await?;
        
        let file = File {
            id: Uuid::new_v4(),
            entity_type: "digital_product".to_string(),
            entity_id: product_id,
            original_name: filename.to_string(),
            stored_name: filename.to_string(),
            file_type: FileType::Document,
            mime_type: mime_type.to_string(),
            size_bytes: file_data.len() as i64,
            storage_backend: self.storage_provider.id().to_string(),
            storage_path,
            public_url: upload_result.public_url,
            cdn_url: None, // Digital products are private
            checksum: upload_result.checksum,
            metadata: FileMetadata {
                is_optimized: false,
                ..Default::default()
            },
            transformations: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
        };
        
        // For digital products, store file but don't expose URL publicly
        // URL is only accessible via signed URL when purchased
        self.file_repository.save(file.clone()).await?;
        
        Ok(file)
    }
    
    pub async fn generate_download_url(
        &self,
        file_id: Uuid,
        order_id: Uuid,
    ) -> Result<String> {
        let file = self.file_repository.find_by_id(file_id).await?
            .ok_or_else(|| MediaError::FileNotFound(file_id))?;
        
        // Verify customer has purchased the product
        self.verify_order_ownership(order_id, file.entity_id).await?;
        
        // Generate limited-time signed URL
        let signed_url = self.storage_provider
            .generate_signed_url(&file.storage_path, Duration::hours(24))
            .await?;
        
        Ok(signed_url)
    }
}
```

## Cleanup Strategy

```rust
impl FileService {
    pub async fn cleanup_orphaned_files(&self) -> Result<CleanupResult> {
        // Find files without associated entities
        let orphaned_files = self.file_repository
            .find_orphaned_files()
            .await?;
        
        let mut deleted_count = 0;
        let mut saved_space = 0i64;
        
        for file in orphaned_files {
            match self.storage_provider.delete_file(&file.storage_path).await {
                Ok(_) => {
                    self.file_repository.delete(&file.id).await?;
                    deleted_count += 1;
                    saved_space += file.size_bytes;
                }
                Err(e) => {
                    error!("Failed to delete orphaned file {}: {}", file.id, e);
                }
            }
        }
        
        Ok(CleanupResult {
            files_deleted: deleted_count,
            space_reclaimed_bytes: saved_space,
        })
    }
    
    pub async fn cleanup_expired_files(&self) -> Result<CleanupResult> {
        let expired_files = self.file_repository
            .find_expired_files()
            .await?;
        
        let mut deleted_count = 0;
        let mut saved_space = 0i64;
        
        for file in expired_files {
            match self.storage_provider.delete_file(&file.storage_path).await {
                Ok(_) => {
                    self.file_repository.delete(&file.id).await?;
                    deleted_count += 1;
                    saved_space += file.size_bytes;
                }
                Err(e) => {
                    error!("Failed to delete expired file {}: {}", file.id, e);
                }
            }
        }
        
        Ok(CleanupResult {
            files_deleted: deleted_count,
            space_reclaimed_bytes: saved_space,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CleanupResult {
    pub files_deleted: i64,
    pub space_reclaimed_bytes: i64,
}
```

## Configuration Examples

```toml
# Development configuration
[media]
storage_provider = "local"

[media.local]
base_path = "./uploads"
base_url = "http://localhost:8080/uploads"

[media.image_processing]
auto_optimize = true
default_quality = 80

# Production with S3
[media]
storage_provider = "s3"

[media.s3]
region = "us-east-1"
bucket = "rcommerce-media-prod"
access_key_id = "${AWS_ACCESS_KEY_ID}"
secret_access_key = "${AWS_SECRET_ACCESS_KEY}"

[media.cdn]
enabled = true
provider = "cloudfront"
distribution_id = "${CLOUDFRONT_DISTRIBUTION_ID}"

# Hybrid setup (images on CDN, documents in S3, backups locally)
[media]
primary_storage = "s3"
backup_storage = "local"

[media.multi_tier]
small_files = "local"          # Images < 100KB
documents = "s3"               # PDFs, etc.
archives = "s3_glacier"        # Old backups
temp_files = "local_volatile"  # Auto-deleted after 24h
```

This comprehensive media storage system provides flexible, scalable file management with support for multiple storage backends, automatic optimization, CDN integration, and proper security controls suitable for production ecommerce environments.
