# Compatibility & Migration Layer Architecture

## Overview

The Compatibility & Migration Layer provides API compatibility with existing ecommerce platforms, enabling seamless migration paths for merchants who want to switch to R commerce without rebuilding their integrations. This layer acts as a translation service, mapping requests and responses between R commerce's native API and the APIs of other platforms.

**Supported Compatibility Modes:**
- **WooCommerce REST API** - Compatibility with WooCommerce v3 API
- **Medusa.js API** - Compatibility with Medusa.js endpoints
- **Shopify API** (Future) - REST and GraphQL compatibility
- **BigCommerce API** (Future) - BigCommerce REST API compatibility

**Key Benefits:**
- Drop-in replacement for existing integrations
- Incremental migration capability
- Existing mobile apps, themes, and plugins continue working
- Reduced migration risk and complexity

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     External Clients                             │
│  (Existing Mobile Apps, Themes, Integrations)                  │
└───────────────────────────┬──────────────────────────────────────┘
                            │
                            │ WooCommerce API Format
                            │ Medusa.js Format
                            │ Shopify Format
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│             Compatibility Layer Gateway                        │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐  │
│  │  WooCommerce    │  │   Medusa.js     │  │   Shopify    │  │
│  │   Adapter       │  │    Adapter      │  │   Adapter    │  │
│  └────────┬────────┘  └────────┬────────┘  └───────┬──────┘  │
│           │                    │                   │         │
│           └────────────────────┴───────────────────┴─────────┘
│                                      │
│        ┌─────────────────────────────▼────────────────────┐   │
│        │      Translation & Mapping Service               │   │
│        │                                                   │   │
│        │  - Request translation to R commerce format    │   │
│        │  - Response translation to target format       │   │
│        │  - Field mapping & transformation              │   │
│        │  - Endian compatibility & type conversion      │   │
│        │  - Error message translation                   │   │
│        └────────────────────────┬────────────────────────┘   │
│                                 │                            │
│                                 │ R commerce Native API      │
│                                 ▼                            │
│              ┌────────────────────────────────┐              │
│              │      R commerce Core API       │              │
│              │                                │              │
│              │  - Native/Canonical API        │              │
│              │  - Direct repository access    │              │
│              │  - Standardized responses      │              │
│              └────────────────────────────────┘              │
└───────────────────────────────────────────────────────────────┘
```

## Adapter Pattern Implementation

### Core Adapter Trait

```rust
#[async_trait]
pub trait CompatibilityAdapter: Send + Sync + 'static {
    /// Platform identifier (e.g., "woocommerce", "medusa", "shopify")
    fn platform(&self) -> &'static str;
    
    /// API version supported
    fn api_version(&self) -> &'static str;
    
    /// Check if this adapter can handle the request
    fn can_handle(&self, request: &HttpRequest) -> bool;
    
    /// Translate incoming request to R commerce format
    async fn translate_request(
        &self,
        request: HttpRequest,
    ) -> Result<TranslatedRequest>;
    
    /// Translate R commerce response to platform format
    async fn translate_response(
        &self,
        r_response: RCommerceResponse,
        original_request: &HttpRequest,
    ) -> Result<HttpResponse>;
    
    /// Translate error to platform format
    fn translate_error(&self, error: &RCommerceError) -> PlatformError;
    
    /// Get endpoint mappings for documentation
    fn endpoint_mappings(&self) -> Vec<EndpointMapping>;
}

#[derive(Debug, Clone)]
pub struct EndpointMapping {
    pub platform_endpoint: String,
    pub platform_method: HttpMethod,
    pub r_commerce_endpoint: String,
    pub r_commerce_method: HttpMethod,
    pub complexity: MappingComplexity,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MappingComplexity {
    Direct,      // One-to-one mapping
    Simple,      // Minor transformations
    Complex,     // Significant restructuring
    Custom,      // Requires custom logic
    Unsupported, // Feature not available
}
```

## WooCommerce Compatibility Adapter

```rust
pub struct WooCommerceAdapter {
    base_url: String,
    adapter_config: WooCommerceConfig,
}

#[async_trait]
impl CompatibilityAdapter for WooCommerceAdapter {
    fn platform(&self) -> &'static str { "woocommerce" }
    
    fn api_version(&self) -> &'static str { "v3" }
    
    fn can_handle(&self, request: &HttpRequest) -> bool {
        // Check if request has WooCommerce API headers or matches pattern
        request.headers.contains_key("X-WC-Store-API-Nonce") ||
        request.path.starts_with("/wc-api/v3") ||
        request.path.starts_with("/wp-json/wc/v3")
    }
    
    async fn translate_request(
        &self,
        request: HttpRequest,
    ) -> Result<TranslatedRequest> {
        let (r_endpoint, r_body) = match request.path.as_str() {
            // Products
            "/wc-api/v3/products" if request.method == HttpMethod::GET => {
                self.translate_list_products(request.query_params())
            }
            "/wc-api/v3/products" if request.method == HttpMethod::POST => {
                self.translate_create_product(request.body()?).await
            }
            "/wc-api/v3/products/<id>" if request.method == HttpMethod::GET => {
                self.translate_get_product(request.path_param("id")?)
            }
            "/wc-api/v3/products/<id>" if request.method == HttpMethod::PUT => {
                self.translate_update_product(
                    request.path_param("id")?,
                    request.body()?
                ).await
            }
            "/wc-api/v3/products/<id>" if request.method == HttpMethod::DELETE => {
                self.translate_delete_product(request.path_param("id")?)
            }
            
            // Orders
            "/wc-api/v3/orders" if request.method == HttpMethod::GET => {
                self.translate_list_orders(request.query_params())
            }
            "/wc-api/v3/orders" if request.method == HttpMethod::POST => {
                self.translate_create_order(request.body()?).await
            }
            "/wc-api/v3/orders/<id>" if request.method == HttpMethod::GET => {
                self.translate_get_order(request.path_param("id")?)
            }
            
            // Customers
            "/wc-api/v3/customers" if request.method == HttpMethod::GET => {
                self.translate_list_customers(request.query_params())
            }
            "/wc-api/v3/customers/<id>" if request.method == HttpMethod::GET => {
                self.translate_get_customer(request.path_param("id")?)
            }
            
            // Order Notes
            "/wc-api/v3/orders/<id>/notes" if request.method == HttpMethod::GET => {
                self.translate_list_order_notes(request.path_param("id")?)
            }
            
            // Coupons
            "/wc-api/v3/coupons" if request.method == HttpMethod::GET => {
                self.translate_list_coupons(request.query_params())
            }
            
            _ => {
                return Err(CompatibilityError::UnsupportedEndpoint {
                    platform: "WooCommerce",
                    endpoint: request.path.clone(),
                    method: request.method,
                })
            }
        }?;
        
        Ok(TranslatedRequest {
            endpoint: r_endpoint,
            method: self.map_http_method(request.method),
            headers: self.translate_headers(request.headers),
            body: r_body,
            query_params: request.query_params(),
        })
    }
    
    async fn translate_response(
        &self,
        r_response: RCommerceResponse,
        original_request: &HttpRequest,
    ) -> Result<HttpResponse> {
        match original_request.path.as_str() {
            "/wc-api/v3/products" => self.translate_products_list_response(r_response).await,
            "/wc-api/v3/products/<id>" => self.translate_product_response(r_response).await,
            "/wc-api/v3/orders" => self.translate_orders_list_response(r_response).await,
            "/wc-api/v3/orders/<id>" => self.translate_order_response(r_response).await,
            "/wc-api/v3/customers" => self.translate_customers_list_response(r_response).await,
            _ => self.translate_generic_response(r_response).await,
        }
    }
}
```

### WooCommerce Product Mapping Example

```rust
impl WooCommerceAdapter {
    fn translate_product_from_woocommerce(
        &self,
        wc_product: WooCommerceProduct
    ) -> CreateProductRequest {
        let status = match wc_product.status.as_str() {
            "publish" => ProductStatus::Active,
            "draft" => ProductStatus::Draft,
            "private" => ProductStatus::Archived,
            _ => ProductStatus::Draft,
        };
        
        let inventory_policy = if wc_product.backorders == "no" {
            InventoryPolicy::DenyWhenOversold
        } else {
            InventoryPolicy::ContinueSelling
        };
        
        CreateProductRequest {
            name: wc_product.name,
            slug: wc_product.slug,
            description: wc_product.description,
            short_description: wc_product.short_description,
            sku: wc_product.sku,
            price: Decimal::from_str(&wc_product.price).unwrap_or(Decimal::ZERO),
            regular_price: Some(Decimal::from_str(&wc_product.regular_price).ok()),
            sale_price: wc_product.sale_price.map(|p| Decimal::from_str(&p).ok()).flatten(),
            on_sale: wc_product.on_sale,
            currency: "USD".to_string(), // WooCommerce default
            inventory_quantity: wc_product.stock_quantity.unwrap_or(0),
            inventory_policy,
            manage_stock: wc_product.manage_stock,
            weight: wc_product.weight.map(|w| w.parse().ok()).flatten(),
            length: wc_product.dimensions.length.map(|l| l.parse().ok()).flatten(),
            width: wc_product.dimensions.width.map(|w| w.parse().ok()).flatten(),
            height: wc_product.dimensions.height.map(|h| h.parse().ok()).flatten(),
            status,
            categories: wc_product.categories.into_iter()
                .map(|c| c.name)
                .collect(),
            images: wc_product.images.into_iter()
                .map(|img| ProductImage {
                    url: img.src,
                    alt: img.alt,
                    position: img.position as i32,
                })
                .collect(),
            meta_data: self.translate_metadata(wc_product.meta_data),
        }
    }
    
    fn translate_product_to_woocommerce(
        &self,
        r_product: Product
    ) -> WooCommerceProduct {
        let stock_status = if r_product.inventory_quantity > 0 {
            "instock"
        } else if r_product.inventory_policy == InventoryPolicy::ContinueSelling {
            "onbackorder"
        } else {
            "outofstock"
        };
        
        WooCommerceProduct {
            id: r_product.id.to_string(),
            name: r_product.name,
            slug: r_product.slug,
            description: r_product.description,
            short_description: r_product.short_description,
            sku: r_product.sku,
            price: r_product.price.to_string(),
            regular_price: r_product.regular_price
                .map(|p| p.to_string())
                .unwrap_or_else(|| r_product.price.to_string()),
            sale_price: r_product.sale_price.map(|p| p.to_string()),
            on_sale: r_product.on_sale,
            stock_quantity: Some(r_product.inventory_quantity),
            stock_status: stock_status.to_string(),
            manage_stock: r_product.manage_stock,
            backorders: if r_product.inventory_policy == InventoryPolicy::ContinueSelling {
                "yes"
            } else {
                "no"
            }.to_string(),
            weight: r_product.weight.map(|w| w.to_string()),
            dimensions: WooCommerceDimensions {
                length: r_product.length.map(|l| l.to_string()),
                width: r_product.width.map(|w| w.to_string()),
                height: r_product.height.map(|h| h.to_string()),
            },
            categories: r_product.categories.iter()
                .map(|cat| WooCommerceCategory {
                    id: 0, // Would need to lookup WooCommerce category ID
                    name: cat.name.clone(),
                })
                .collect(),
            images: r_product.images.into_iter()
                .map(|img| WooCommerceImage {
                    src: img.url,
                    alt: img.alt,
                    position: img.position as u32,
                })
                .collect(),
            meta_data: self.reverse_translate_metadata(r_product.meta_data),
            status: match r_product.status {
                ProductStatus::Active => "publish",
                ProductStatus::Draft => "draft",
                _ => "private",
            }.to_string(),
        }
    }
    
    fn translate_product_list_params(
        &self,
        params: &QueryParams
    ) -> (String, QueryParams) {
        let mut r_params = params.clone();
        
        // Map WooCommerce pagination to R commerce
        if let Some(page) = params.get("page") {
            r_params.insert("page".to_string(), page.clone());
            
            // WooCommerce uses per_page, we use per_page
            if let Some(per_page) = params.get("per_page") {
                r_params.insert("per_page".to_string(), per_page.clone());
            }
        }
        
        // Map status filter
        if let Some(status) = params.get("status") {
            let r_status = match status.as_str() {
                "publish" => "active",
                "draft" => "draft",
                "private" => "archived",
                _ => status,
            };
            r_params.insert("status".to_string(), r_status.to_string());
        }
        
        // Map category filter
        if let Some(category) = params.get("category") {
            r_params.insert("category".to_string(), category.clone());
        }
        
        // Map search
        if let Some(search) = params.get("search") {
            r_params.insert("q".to_string(), search.clone());
        }
        
        ("/v1/products".to_string(), r_params)
    }
}
```

## Medusa.js Compatibility Adapter

```rust
pub struct MedusaAdapter {
    base_url: String,
    config: MedusaConfig,
}

#[async_trait]
impl CompatibilityAdapter for MedusaAdapter {
    fn platform(&self) -> &'static str { "medusa" }
    
    fn api_version(&self) -> &'static str { "v1" }
    
    fn can_handle(&self, request: &HttpRequest) -> bool {
        request.headers.get("user-agent")
            .map(|ua| ua.contains("medusa-js"))
            .unwrap_or(false) ||
        request.path.starts_with("/store") ||
        request.path.starts_with("/admin")
    }
    
    async fn translate_request(
        &self,
        request: HttpRequest,
    ) -> Result<TranslatedRequest> {
        // Medusa.js uses different conventions than WooCommerce
        // It uses /store for customer-facing and /admin for admin
        
        match (request.method, request.path.as_str()) {
            // Store API (Customer-facing)
            (HttpMethod::GET, "/store/products") => {
                self.translate_medusa_store_products(request.query_params())
            }
            (HttpMethod::GET, "/store/products/<id>") => {
                self.translate_get_product(request.path_param("id")?)
            }
            (HttpMethod::POST, "/store/carts") => {
                self.translate_create_cart(request.body()?).await
            }
            (HttpMethod::POST, "/store/carts/<id>/line-items") => {
                self.translate_add_to_cart(
                    request.path_param("id")?,
                    request.body()?
                ).await
            }
            (HttpMethod::POST, "/store/carts/<id>/complete") => {
                self.translate_complete_order(request.path_param("id")?)
            }
            
            // Admin API
            (HttpMethod::GET, "/admin/products") => {
                self.translate_list_products_admin(request.query_params())
            }
            (HttpMethod::POST, "/admin/products") => {
                self.translate_create_product_admin(request.body()?).await
            }
            (HttpMethod::POST, "/admin/orders") => {
                self.translate_create_order_admin(request.body()?).await
            }
            
            _ => Err(CompatibilityError::UnsupportedEndpoint {
                platform: "Medusa.js",
                endpoint: request.path.clone(),
                method: request.method,
            }),
        }
    }
}
```

## Compatibility Gateway Service

```rust
pub struct CompatibilityGateway {
    adapters: Vec<Box<dyn CompatibilityAdapter>>,
    r_commerce_client: Arc<RCommerceClient>,
    logger: Logger,
}

impl CompatibilityGateway {
    pub fn new(r_commerce_client: Arc<RCommerceClient>) -> Self {
        let mut adapters: Vec<Box<dyn CompatibilityAdapter>> = Vec::new();
        
        // Register adapters
        adapters.push(Box::new(WooCommerceAdapter::new(
            r_commerce_client.clone()
        )));
        
        adapters.push(Box::new(MedusaAdapter::new(
            r_commerce_client.clone()
        )));
        
        // Add more adapters as needed
        
        Self {
            adapters,
            r_commerce_client,
            logger: Logger::new("compatibility_gateway"),
        }
    }
    
    pub async fn handle_request(&self, request: HttpRequest) -> Result<HttpResponse> {
        // Find adapter that can handle this request
        let adapter = self.adapters.iter()
            .find(|a| a.can_handle(&request))
            .ok_or_else(|| {
                CompatibilityError::NoAdapterFound {
                    path: request.path.clone(),
                    method: request.method.clone(),
                }
            })?;
        
        let platform = adapter.platform().to_string();
        
        // Log compatibility request
        self.logger.info(&format!(
            "Compatibility request for {} platform: {} {}",
            platform,
            request.method,
            request.path
        ));
        
        // Translate request to R commerce format
        let translated_request = adapter.translate_request(request).await?;
        
        // Call R commerce API
        let r_response = self.call_r_commerce(translated_request).await?;
        
        // Translate response back to platform format
        let platform_response = adapter.translate_response(r_response, &request).await?;
        
        Ok(platform_response)
    }
    
    async fn call_r_commerce(
        &self,
        request: TranslatedRequest,
    ) -> Result<RCommerceResponse> {
        // Make internal call to R commerce API
        self.r_commerce_client.request(
            request.endpoint,
            request.method,
            request.headers,
            request.body,
            request.query_params,
        ).await
    }
    
    /// Generate compatibility documentation for all adapters
    pub fn generate_compatibility_docs(&self) -> CompatibilityMatrix {
        let mut matrix = CompatibilityMatrix::new();
        
        for adapter in &self.adapters {
            let platform = adapter.platform();
            let api_version = adapter.api_version();
            
            let endpoint_mappings = adapter.endpoint_mappings();
            
            for mapping in endpoint_mappings {
                matrix.add_endpoint(PlatformEndpoint {
                    platform: platform.to_string(),
                    platform_version: api_version.to_string(),
                    platform_endpoint: mapping.platform_endpoint,
                    platform_method: mapping.platform_method,
                    r_commerce_endpoint: mapping.r_commerce_endpoint,
                    r_commerce_method: mapping.r_commerce_method,
                    complexity: mapping.complexity,
                    notes: mapping.notes,
                    supported: mapping.complexity != MappingComplexity::Unsupported,
                });
            }
        }
        
        matrix
    }
}
```

## Migration Tools & Utilities

### Migration CLI Tool

```rust
pub struct MigrationTool {
    compatibility_gateway: Arc<CompatibilityGateway>,
    logger: Logger,
}

impl MigrationTool {
    /// Analyze compatibility for a specific platform
    pub async fn analyze_compatibility(
        &self,
        platform: &str,
    ) -> Result<CompatibilityReport> {
        let matrix = self.compatibility_gateway.generate_compatibility_docs();
        
        let platform_endpoints = matrix.get_by_platform(platform);
        
        let total_endpoints = platform_endpoints.len();
        let supported_endpoints = platform_endpoints.iter()
            .filter(|ep| ep.supported)
            .count();
        
        let unsupported_endpoints = platform_endpoints.iter()
            .filter(|ep| !ep.supported && ep.complexity == MappingComplexity::Unsupported)
            .collect::<Vec<_>>();
        
        Ok(CompatibilityReport {
            platform: platform.to_string(),
            total_endpoints,
            supported_endpoints,
            compatibility_percentage: (supported_endpoints as f64 / total_endpoints as f64) * 100.0,
            unsupported_endpoints,
            recommendations: self.generate_recommendations(platform, &platform_endpoints),
        })
    }
    
    /// Test compatibility by making actual API calls
    pub async fn test_compatibility(
        &self,
        platform: &str,
        test_config: TestConfig,
    ) -> Result<TestReport> {
        let adapter = self.get_adapter(platform)?;
        let test_suite = self.load_test_suite(platform);
        
        let mut results = Vec::new();
        
        for test in test_suite.tests {
            let result = self.execute_test(&adapter, &test, &test_config).await;
            results.push(result);
        }
        
        let passed = results.iter().filter(|r| r.status == TestStatus::Passed).count();
        let failed = results.iter().filter(|r| r.status == TestStatus::Failed).count();
        let skipped = results.iter().filter(|r| r.status == TestStatus::Skipped).count();
        
        Ok(TestReport {
            platform: platform.to_string(),
            total_tests: results.len(),
            passed,
            failed,
            skipped,
            success_rate: (passed as f64 / results.len() as f64) * 100.0,
            results,
        })
    }
    
    /// Generate migration script from one platform to another
    pub fn generate_migration_script(
        &self,
        from_platform: &str,
        to_platform: &str,
    ) -> Result<MigrationScript> {
        let from_adapter = self.get_adapter(from_platform)?;
        let to_adapter = self.get_adapter(to_platform)?;
        
        let from_mappings = from_adapter.endpoint_mappings();
        let to_mappings = to_adapter.endpoint_mappings();
        
        let mut script_steps = Vec::new();
        
        // Find differences in data models
        script_steps.push(self.generate_data_model_migration(from_platform, to_platform));
        
        // Find API differences
        for from_mapping in &from_mappings {
            let to_mapping = to_mappings.iter()
                .find(|m| m.platform_endpoint == from_mapping.platform_endpoint);
            
            match to_mapping {
                Some(to_map) => {
                    if from_mapping.complexity != to_map.complexity {
                        script_steps.push(MigrationStep::ApiDifference {
                            endpoint: from_mapping.platform_endpoint.clone(),
                            complexity_change: format!("{:?} -> {:?}",
                                from_mapping.complexity,
                                to_map.complexity
                            ),
                            notes: to_map.notes.clone(),
                        });
                    }
                }
                None => {
                    script_steps.push(MigrationStep::UnsupportedEndpoint {
                        endpoint: from_mapping.platform_endpoint.clone(),
                        notes: "No equivalent endpoint in target platform".to_string(),
                    });
                }
            }
        }
        
        Ok(MigrationScript {
            from_platform: from_platform.to_string(),
            to_platform: to_platform.to_string(),
            steps: script_steps,
            warnings: self.generate_warnings(&script_steps),
        })
    }
}
```

## Configuration

```toml
[compatibility]
# Enable/disable compatibility layer
enabled = true

# Default mode when platform can't be detected
default_platform = "woocommerce"

[compatibility.woocommerce]
enabled = true
# WooCommerce-specific settings
legacy_api = false  # Set to true for older WC API versions
translate_user_ids = true  # Map WP user IDs to R commerce customer IDs

[compatibility.medusa]
enabled = true
# Medusa.js-specific settings
store_endpoint = "/store"
admin_endpoint = "/admin"

[compatibility.shopify]
enabled = false  # Not yet implemented
# Shopify API version to mimic
api_version = "2024-01"

[migration]
# Migration settings
bulk_import_chunk_size = 100
rate_limit_per_second = 10
concurrent_workers = 5
retry_failed_items = true

[migration.logging]
log_level = "info"
log_file = "/var/log/rcommerce/migration.log"
```

## API Differences & Limitations

### WooCommerce → R commerce

| WooCommerce Feature | R commerce Support | Notes |
|---------------------|-------------------|-------|
| WordPress Integration | ❌ Limited | R commerce is standalone |
| PHP Hooks/Filters | ❌ No | Use webhooks instead |
| wp_users table | ❌ No | Separate customer system |
| Post Meta |  Yes | Mapped to meta_data |
| Taxonomies |  Yes | Mapped to collections |
| Product Types |  Partial | Simple, variable supported |
| Downloadable Products |  Yes | Digital product support |
| Bookings | ❌ No | Plugin not available |
| Subscriptions | ⚠️ Partial | Core subscription support |
| Multi-site | ❌ No | Single store |

### Medusa.js → R commerce

| Medusa.js Feature | R commerce Support | Notes |
|-------------------|-------------------|-------|
| Regions |  Yes | Mapped to shipping zones |
| Price Lists |  Yes | Customer group pricing |
| Sales Channels |  Yes | Channel support |
| Publishable API Keys |  Yes | API key system |
| Order Edits |  Yes | Order editing |
| Swaps/Returns |  Yes | RMA system |
| Claims |  Yes | Support tickets |
| Draft Orders |  Yes | Manual orders |
| Fulfillment Providers |  Yes | Pluggable system |
| Payment Providers |  Yes | Pluggable system |
| Inventory Management |  Yes | Multi-location |
| Analytics | ⚠️ Partial | Basic reports |
| Plugin System | ⚠️ Planned | Coming soon |

## Usage Examples

### WordPress Plugin for Compatibility

```php
<?php
/**
 * Plugin Name: R commerce Compatibility Bridge
 * Description: Routes WooCommerce API requests to R commerce backend
 * Version: 1.0.0
 * Author: Your Team
 */

// Add filter to override WooCommerce API requests
add_filter('woocommerce_api_is_api_request', function($is_api) {
    if ($is_api && strpos($_SERVER['REQUEST_URI'], '/wc-api/v3') === 0) {
        // Route to R commerce compatibility layer
        return route_to_rcommerce();
    }
    return $is_api;
});

function route_to_rcommerce() {
    $rcommerce_url = get_option('rcommerce_api_url');
    $api_key = get_option('rcommerce_api_key');
    
    // Forward request to R commerce compatibility layer
    $response = wp_remote_request(
        $rcommerce_url . $_SERVER['REQUEST_URI'],
        array(
            'headers' => array(
                'Authorization' => 'Bearer ' . $api_key,
                'X-Compatibility' => 'woocommerce',
            ),
            'method' => $_SERVER['REQUEST_METHOD'],
            'body' => file_get_contents('php://input'),
        )
    );
    
    // Return response
    wp_send_json(
        json_decode(wp_remote_retrieve_body($response), true),
        wp_remote_retrieve_response_code($response)
    );
}
```

### Medusa.js Frontend with R commerce Backend

```typescript
// medusa-config.js
module.exports = {
  projectConfig: {
    // Point to R commerce compatibility layer
    storeUrl: "https://api.yourstore.com/store",
    adminUrl: "https://api.yourstore.com/admin",
    
    // Compatibility mode
    compatibility: {
      platform: "medusa",
      apiKey: process.env.RCOMMERCE_API_KEY,
    },
  },
  
  plugins: [
    // Standard Medusa plugins work with compatibility layer
    `@medusajs/admin`,
  ],
};
```

### Migration Script Example

```bash
#!/bin/bash
# WooCommerce to R commerce Migration Script

WOOCOMMERCE_URL="https://your-woocommerce-site.com"
RCOMMERCE_URL="https://api.yourstore.com/wc-api/v3"
RCOMMERCE_API_KEY="sk_xxx"

# Migrate products
echo "Migrating products..."
curl -s "${WOOCOMMERCE_URL}/wp-json/wc/v3/products?per_page=100" \
  -u "${WOOCOMMERCE_CONSUMER_KEY}:${WOOCOMMERCE_CONSUMER_SECRET}" \
  | jq -c '.[]' | while read -r product; do
    
    # Transform product and post to R commerce
    transformed=$(echo "$product" | jq '{       
      name: .name,
      slug: .slug,
      description: .description,
      sku: .sku,
      price: .price | tonumber,
      inventory_quantity: .stock_quantity // 0,
      status: (if .status == "publish" then "active" else "draft" end)
    }')
    
    curl -X POST "${RCOMMERCE_URL}/products" \
      -H "Authorization: Bearer ${RCOMMERCE_API_KEY}" \
      -H "Content-Type: application/json" \
      -d "$transformed" \
      | jq '.id'
done

# Migrate customers
echo "Migrating customers..."
curl -s "${WOOCOMMERCE_URL}/wp-json/wc/v3/customers?per_page=100" \
  -u "${WOOCOMMERCE_CONSUMER_KEY}:${WOOCOMMERCE_CONSUMER_SECRET}" \
  | jq -c '.[]' | while read -r customer; do
    
    transformed=$(echo "$customer" | jq '{
      email: .email,
      first_name: .first_name,
      last_name: .last_name,
      billing_address: {
        first_name: .billing.first_name,
        last_name: .billing.last_name,
        company: .billing.company,
        address1: .billing.address_1,
        address2: .billing.address_2,
        city: .billing.city,
        state: .billing.state,
        postal_code: .billing.postcode,
        country: .billing.country,
        phone: .billing.phone
      }
    }')
    
    curl -X POST "${RCOMMERCE_URL}/customers" \
      -H "Authorization: Bearer ${RCOMMERCE_API_KEY}" \
      -H "Content-Type: application/json" \
      -d "$transformed" \
      | jq '.id'
done

echo "Migration complete!"
```

## Testing Compatibility

```rust
#[cfg(test)]
mod compatibility_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_woocommerce_product_compatibility() {
        let adapter = WooCommerceAdapter::new(test_config());
        let gateway = CompatibilityGateway::new(adapter).
        
        // Test WooCommerce-style request
        let request = HttpRequest {
            method: HttpMethod::GET,
            path: "/wc-api/v3/products".to_string(),
            headers: HashMap::new(),
            query_params: vec![("per_page".to_string(), "20".to_string())]
                .into_iter()
                .collect(),
            body: None,
        };
        
        let response = gateway.handle_request(request).await.unwrap();
        
        // Response should be in WooCommerce format
        assert!(response.is_woo_commerce_format());
        assert_eq!(response.status_code, 200);
    }
    
    #[test]
    fn test_compatibility_analysis() {
        let tool = MigrationTool::new();
        let report = tool.analyze_compatibility("woocommerce").unwrap();
        
        assert!(report.compatibility_percentage > 80.0);
        assert!(report.unsupported_endpoints.is_empty());
    }
}
```

---

Next: [07-order-management.md](07-order-management.md) - Order management system details
