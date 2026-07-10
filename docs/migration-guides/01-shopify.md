# Shopify to R commerce Migration Guide

## Overview

This guide provides step-by-step instructions for migrating from Shopify to R commerce using the compatibility layer and direct migration methods.

## Pre-Migration Checklist

### Shopify Store Audit

```bash
# Count products
curl -X GET "https://your-store.myshopify.com/admin/api/2024-01/products/count.json" \
  -H "X-Shopify-Access-Token: YOUR_ADMIN_API_TOKEN"

# Count customers
curl -X GET "https://your-store.myshopify.com/admin/api/2024-01/customers/count.json" \
  -H "X-Shopify-Access-Token: YOUR_ADMIN_API_TOKEN"

# Count orders
curl -X GET "https://your-store.myshopify.com/admin/api/2024-01/orders/count.json" \
  -H "X-Shopify-Access-Token: YOUR_ADMIN_API_TOKEN" \
  -H "Content-Type: application/json"
```

### Export Shopify Data

#### Method 1: Using Shopify Admin API

```bash
#!/bin/bash
# export-shopify.sh

SHOP_DOMAIN="your-store.myshopify.com"
API_TOKEN="YOUR_ADMIN_API_TOKEN"

# Create export directory
mkdir -p shopify-export

# Export products (with pagination)
echo "Exporting products..."
curl -s -X GET "https://${SHOP_DOMAIN}/admin/api/2024-01/products.json?limit=250" \
  -H "X-Shopify-Access-Token: ${API_TOKEN}" | \
  jq '.' > shopify-export/products.json

# Export customers
echo "Exporting customers..."
curl -s -X GET "https://${SHOP_DOMAIN}/admin/api/2024-01/customers.json?limit=250" \
  -H "X-Shopify-Access-Token: ${API_TOKEN}" | \
  jq '.' > shopify-export/customers.json

# Export orders
echo "Exporting orders..."
curl -s -X GET "https://${SHOP_DOMAIN}/admin/api/2024-01/orders.json?status=any&limit=250" \
  -H "X-Shopify-Access-Token: ${API_TOKEN}" | \
  jq '.' > shopify-export/orders.json

# Export collections/smart collections
echo "Exporting collections..."
curl -s -X GET "https://${SHOP_DOMAIN}/admin/api/2024-01/smart_collections.json?limit=250" \
  -H "X-Shopify-Access-Token: ${API_TOKEN}" | \
  jq '.' > shopify-export/collections.json

# Export custom collections
curl -s -X GET "https://${SHOP_DOMAIN}/admin/api/2024-01/custom_collections.json?limit=250" \
  -H "X-Shopify-Access-Token: ${API_TOKEN}" | \
  jq '.' >> shopify-export/collections.json

echo "Export complete!"
```

#### Method 2: Using Shopify CLI

```bash
# Install Shopify CLI
npm install -g @shopify/cli

# Login to your store
shopify login --store your-store.myshopify.com

# Export products
shopify product list --json > products.json

# Export customers
shopify customer list --json > customers.json
```

## Migration Strategy

### Option 1: Compatibility Layer (Zero Downtime)

This approach uses R commerce's Shopify compatibility layer to run both platforms simultaneously.

```toml
# config/compatibility.toml
[compatibility]
enabled = true
source_platform = "shopify"

[compatibility.shopify]
api_key = "YOUR_SHOPIFY_API_KEY"
store_domain = "your-store.myshopify.com"
shared_inventory = true
sync_orders = true
```

**Benefits:**
- Zero downtime migration
- Test R commerce features gradually
- Keep Shopify as backup
- Sync inventory between platforms

**Setup:**

1. Deploy R commerce with compatibility layer
2. Configure Shopify API credentials
3. Enable bidirectional sync
4. Gradually move features to R commerce
5. Update DNS when ready

### Option 2: Big Bang Migration

This approach migrates all data at once and switches over completely.

## Data Mapping

### Product Data Mapping

| Shopify Field | R commerce Field | Transformation |
|---------------|------------------|----------------|
| `id` | `meta_data.shopify_id` | Store reference |
| `title` | `name` | Direct mapping |
| `body_html` | `description` | Direct mapping |
| `vendor` | `meta_data.vendor` | Store as metadata |
| `product_type` | `category` | Map to category |
| `tags` | `tags` | Direct mapping |
| `handle` | `slug` | Direct mapping |
| `variants` | `variants` | See below |
| `images` | `images` | Map with position |
| `status` | `status` | `active`→`active`, `draft`→`draft` |
| `published_scope` | `meta_data.published_scope` | Store as metadata |

### Variant Mapping

```javascript
// Shopify Variant → R commerce Variant
{
  // Shopify
  "id": "gid://shopify/ProductVariant/123",
  "sku": "SHIRT-RED-L",
  "price": "29.99",
  "compare_at_price": "39.99",
  "inventory_quantity": 100,
  "inventory_policy": "deny",
  "option1": "Red",
  "option2": "Large",
  "barcode": "1234567890",
  "weight": 0.2,
  "weight_unit": "kg",
  "image_id": "gid://shopify/ProductImage/456",
  
  // R commerce
  "sku": "SHIRT-RED-L",
  "price": 29.99,
  "compare_at_price": 39.99,
  "cost": null, // Shopify doesn't have cost field
  "inventory_quantity": 100,
  "inventory_policy": "deny", // Shopify: deny/continue
  "weight": 0.2,
  "weight_unit": "kg",
  "barcode": "1234567890"
}
```

### Customer Mapping

| Shopify Field | R commerce Field | Notes |
|---------------|------------------|-------|
| `id` | `meta_data.shopify_id` | Reference only |
| `email` | `email` | Primary identifier |
| `first_name` | `first_name` | Direct mapping |
| `last_name` | `last_name` | Direct mapping |
| `phone` | `phone` | Direct mapping |
| `state` | `status` | `enabled` → `active` |
| `tags` | `tags` | Direct mapping |
| `addresses` | `addresses` | Array mapping |
| `orders_count` | `orders_count` | Direct mapping |
| `total_spent` | `total_spent` | Direct mapping |
| `email_marketing_consent` | `accepts_marketing` | Map consent status |

### Order Mapping

**Important Notes:**
- Shopify orders are immutable after creation
- R commerce allows order editing (with restrictions)
- Financial status mapping is critical
- Fulfillment status mapping to R commerce status

```javascript
// Shopify Order → R commerce Order
{
  // Map financial_status to R commerce payment_status
  shopify: {
    financial_status: "paid" // "pending", "authorized", "partially_refunded", "refunded", "voided"
  },
  
  r_commerce: {
    payment_status: "paid" // "pending", "authorized", "paid", "partially_refunded", "fully_refunded", "failed"
  }
}
```

## Migration Scripts

### Ruby Script for Product Migration

```ruby
#!/usr/bin/env ruby
# migrate_products.rb

require 'json'
require 'net/http'
require 'uri'

class ShopifyToRCommerce
  def initialize(shopify_domain, shopify_token, rcommerce_url, rcommerce_key)
    @shopify_domain = shopify_domain
    @shopify_token = shopify_token
    @rcommerce_url = rcommerce_url
    @rcommerce_key = rcommerce_key
    
    @http = Net::HTTP.new(URI.parse(@rcommerce_url).host, URI.parse(@rcommerce_url).port)
    @http.use_ssl = true
  end
  
  def migrate_products
    shopify_products = fetch_shopify_products
    
    shopify_products.each do |shopify_product|
      begin
        rcommerce_product = transform_product(shopify_product)
        create_rcommerce_product(rcommerce_product)
        puts " Migrated product: #{shopify_product['title']}"
      rescue => e
        puts " Failed to migrate product: #{shopify_product['title']} - #{e.message}"
      end
    end
  end
  
  private
  
  def fetch_shopify_products
    uri = URI("https://#{@shopify_domain}/admin/api/2024-01/products.json")
    uri.query = URI.encode_www_form(limit: 250)
    
    request = Net::HTTP::Get.new(uri)
    request['X-Shopify-Access-Token'] = @shopify_token
    
    response = @http.request(request)
    
    if response.code == '200'
      JSON.parse(response.body)['products']
    else
      raise "Failed to fetch products: #{response.code} - #{response.body}"
    end
  end
  
  def transform_product(shopify_product)
    {
      name: shopify_product['title'],
      slug: shopify_product['handle'],
      description: shopify_product['body_html'],
      short_description: shopify_product['body_html']&.truncate(200),
      sku: shopify_product['variants'].first['sku'],
      price: shopify_product['variants'].first['price'].to_f,
      compare_at_price: shopify_product['variants'].first['compare_at_price']&.to_f,
      inventory_quantity: shopify_product['variants'].first['inventory_quantity'],
      inventory_policy: shopify_product['variants'].first['inventory_policy'],
      status: map_status(shopify_product['status']),
      weight: shopify_product['variants'].first['weight']&.to_f,
      weight_unit: shopify_product['variants'].first['weight_unit'],
      tags: shopify_product['tags'],
      meta_data: {
        shopify_id: shopify_product['id'],
        vendor: shopify_product['vendor'],
        product_type: shopify_product['product_type'],
        published_at: shopify_product['published_at']
      },
      variants: transform_variants(shopify_product['variants']),
      images: transform_images(shopify_product['images'])
    }
  end
  
  def transform_variants(shopify_variants)
    shopify_variants.map do |variant|
      {
        sku: variant['sku'],
        price: variant['price'].to_f,
        compare_at_price: variant['compare_at_price']&.to_f,
        inventory_quantity: variant['inventory_quantity'],
        inventory_policy: variant['inventory_policy'],
        weight: variant['weight']&.to_f,
        weight_unit: variant['weight_unit'],
        barcode: variant['barcode'],
        meta_data: {
          shopify_id: variant['id'],
          option1: variant['option1'],
          option2: variant['option2'],
          option3: variant['option3']
        }
      }
    end
  end
  
  def transform_images(shopify_images)
    shopify_images.map do |image|
      {
        url: image['src'],
        alt: image['alt'],
        position: image['position']
      }
    end
  end
  
  def map_status(shopify_status)
    case shopify_status
    when 'active'
      'active'
    when 'draft'
      'draft'
    when 'archived'
      'archived'
    else
      'draft'
    end
  end
  
  def create_rcommerce_product(product_data)
    uri = URI("#{@rcommerce_url}/v1/products")
    
    request = Net::HTTP::Post.new(uri)
    request['Authorization'] = "Bearer #{@rcommerce_key}"
    request['Content-Type'] = 'application/json'
    request.body = product_data.to_json
    
    response = @http.request(request)
    
    if response.code != '201'
      raise "Failed to create product: #{response.code} - #{response.body}"
    end
    
    JSON.parse(response.body)
  end
end

# Usage
migrator = ShopifyToRCommerce.new(
  'your-store.myshopify.com',
  'YOUR_SHOPIFY_ADMIN_TOKEN',
  'https://api.yourstore.com',
  'YOUR_RCOMMERCE_API_KEY'
)

migrator.migrate_products
```

### Node.js Script for Customer Migration

```javascript
// migrate-customers.js
const axios = require('axios');
const fs = require('fs').promises;

class ShopifyCustomerMigrator {
  constructor(shopifyDomain, shopifyToken, rcommerceUrl, rcommerceKey) {
    this.shopifyClient = axios.create({
      baseURL: `https://${shopifyDomain}/admin/api/2024-01`,
      headers: {
        'X-Shopify-Access-Token': shopifyToken,
        'Content-Type': 'application/json'
      }
    });
    
    this.rcommerceClient = axios.create({
      baseURL: rcommerceUrl,
      headers: {
        'Authorization': `Bearer ${rcommerceKey}`,
        'Content-Type': 'application/json'
      }
    });
    
    this.migrationLog = [];
  }
  
  async migrateAll() {
    try {
      console.log('Fetching customers from Shopify...');
      const customers = await this.fetchAllCustomers();
      
      console.log(`Found ${customers.length} customers. Starting migration...`);
      
      for (const [index, customer] of customers.entries()) {
        try {
          await this.migrateCustomer(customer);
          console.log(` Migrated ${index + 1}/${customers.length}: ${customer.email}`);
        } catch (error) {
          console.error(` Failed to migrate ${customer.email}:`, error.message);
          this.migrationLog.push({
            customer: customer.email,
            status: 'failed',
            error: error.message
          });
        }
      }
      
      await this.saveMigrationLog();
      console.log('Migration complete!');
      
    } catch (error) {
      console.error('Migration failed:', error);
    }
  }
  
  async fetchAllCustomers() {
    let allCustomers = [];
    let pageInfo = null;
    
    do {
      const params = {
        limit: 250,
        ...(pageInfo && { page_info: pageInfo })
      };
      
      const response = await this.shopifyClient.get('/customers.json', { params });
      allCustomers = allCustomers.concat(response.data.customers);
      
      // Get next page from Link header
      const linkHeader = response.headers.link;
      if (linkHeader) {
        const nextLink = linkHeader.split(',').find(link => link.includes('rel="next"'));
        if (nextLink) {
          const match = nextLink.match(/page_info=([^>]+)/);
          pageInfo = match ? match[1] : null;
        } else {
          pageInfo = null;
        }
      } else {
        pageInfo = null;
      }
    } while (pageInfo);
    
    return allCustomers;
  }
  
  transformCustomer(shopifyCustomer) {
    const defaultAddress = shopifyCustomer.default_address;
    
    return {
      email: shopifyCustomer.email,
      first_name: shopifyCustomer.first_name,
      last_name: shopifyCustomer.last_name,
      phone: shopifyCustomer.phone,
      accepts_marketing: shopifyCustomer.email_marketing_consent?.state === 'subscribed',
      meta_data: {
        shopify_id: shopifyCustomer.id,
        state: shopifyCustomer.state,
        orders_count: shopifyCustomer.orders_count,
        total_spent: shopifyCustomer.total_spent,
        tax_exempt: shopifyCustomer.tax_exempt,
        tags: shopifyCustomer.tags
      },
      addresses: shopifyCustomer.addresses.map(addr => ({
        first_name: addr.first_name,
        last_name: addr.last_name,
        company: addr.company,
        street1: addr.address1,
        street2: addr.address2,
        city: addr.city,
        state: addr.province,
        postal_code: addr.zip,
        country: addr.country_code,
        phone: addr.phone,
        is_default: addr.default
      }))
    };
  }
  
  async migrateCustomer(shopifyCustomer) {
    const rcommerceCustomer = this.transformCustomer(shopifyCustomer);
    
    try {
      const response = await this.rcommerceClient.post('/v1/customers', rcommerceCustomer);
      
      this.migrationLog.push({
        email: shopifyCustomer.email,
        status: 'success',
        rcommerce_id: response.data.data.id,
        shopify_id: shopifyCustomer.id
      });
      
      return response.data;
    } catch (error) {
      if (error.response?.data?.error?.code === 'customer_duplicate_email') {
        // Handle duplicate customer
        console.log(`Customer ${shopifyCustomer.email} already exists, skipping...`);
        this.migrationLog.push({
          email: shopifyCustomer.email,
          status: 'skipped',
          reason: 'duplicate'
        });
      } else {
        throw error;
      }
    }
  }
  
  async saveMigrationLog() {
    await fs.writeFile(
      'customer-migration-log.json',
      JSON.stringify(this.migrationLog, null, 2)
    );
  }
}

// Usage
const migrator = new ShopifyCustomerMigrator(
  'your-store.myshopify.com',
  process.env.SHOPIFY_ADMIN_TOKEN,
  'https://api.yourstore.com',
  process.env.RCOMMERCE_API_KEY
);

migrator.migrateAll().catch(console.error);
```

## SEO and Redirects

### URL Redirect Mapping

Create redirects from Shopify URLs to R commerce URLs:

```ruby
#!/usr/bin/env ruby
# generate_redirects.rb

require 'json'

# Load exported products
products = JSON.parse(File.read('shopify-export/products.json'))

# Generate Nginx redirect rules
File.open('nginx-redirects.conf', 'w') do |file|
  products.each do |product|
    shopify_handle = product['handle']
    shopify_id = product['id']
    
    # Assuming you have a mapping of Shopify IDs to R commerce IDs
    # You'd need to create this from the migration logs
    rcommerce_id = get_rcommerce_product_id(shopify_id)
    
    next unless rcommerce_id
    
    # Generate redirect
    file.puts "# Redirect for #{product['title']}"
    file.puts "location = /products/#{shopify_handle} {"
    file.puts "  return 301 /products/#{rcommerce_id};"
    file.puts "}"
    file.puts ""
  end
end
```

### Nginx Redirect Configuration

```nginx
# Add to your Nginx configuration
server {
    listen 80;
    server_name your-store.myshopify.com;
    
    # Product redirects
    location ~ ^/products/(.+)$ {
        return 301 https://your-new-store.com/products/$1;
    }
    
    # Collection redirects
    location ~ ^/collections/(.+)$ {
        return 301 https://your-new-store.com/collections/$1;
    }
    
    # Page redirects
    location ~ ^/pages/(.+)$ {
        return 301 https://your-new-store.com/pages/$1;
    }
    
    # Blog redirects
    location ~ ^/blogs/(.+)$ {
        return 301 https://your-new-store.com/blog/$1;
    }
    
    # Cart redirect
    location /cart {
        return 301 https://your-new-store.com/cart;
    }
    
    # Checkout redirect
    location /checkout {
        return 301 https://your-new-store.com/checkout;
    }
    
    # Account redirects
    location ~ ^/account(.+)$ {
        return 301 https://your-new-store.com/account$1;
    }
}
```

## Post-Migration Checklist

Verify the following after migration:

### Products
- [ ] All products migrated (count matches)
- [ ] Product details correct (name, description, price)
- [ ] Images migrated successfully
- [ ] Variants/options correct
- [ ] Inventory quantities accurate
- [ ] SKUs preserved
- [ ] SEO data (meta titles, descriptions) migrated
- [ ] Product URLs redirect correctly

### Customers
- [ ] All customers migrated (count matches)
- [ ] Email addresses preserved
- [ ] Shipping addresses correct
- [ ] Customer groups restored (if applicable)
- [ ] Marketing preferences preserved
- [ ] Password reset emails sent

### Orders
- [ ] Order history accessible (if migrated)
- [ ] Order numbers preserved (or redirect mapping created)
- [ ] Order details accurate
- [ ] Payment history intact
- [ ] Fulfillment data accurate
- [ ] Refund information preserved

### Settings
- [ ] Payment gateways configured
- [ ] Shipping methods set up
- [ ] Tax rates configured
- [ ] Email templates customized
- [ ] Notification preferences set

### Integrations
- [ ] Payment webhooks configured
- [ ] Shipping provider connected
- [ ] Email marketing integrated
- [ ] Analytics tracking working
- [ ] CRM connected

### Testing
- [ ] Storefront loads correctly
- [ ] Products searchable
- [ ] Add to cart works
- [ ] Checkout process functional
- [ ] Payment processing works
- [ ] Order confirmation sent
- [ ] Account login functional
- [ ] Password reset works
- [ ] Email notifications send

### Performance
- [ ] Page load times acceptable (<3s)
- [ ] Search returns results quickly
- [ ] Checkout performs well under load
- [ ] Database queries optimized

## Rollback Plan

If migration fails:

1. **Revert DNS Changes**: Point back to Shopify
2. **Keep Shopify Store**: Don't cancel immediately
3. **Data Backup**: Keep exported data
4. **Document Issues**: Note what went wrong
5. **Retry with Fixes**: Address issues and retry

## Tips and Best Practices

### 1. Start Small
- Migrate a few products first
- Test checkout flow
- Verify integrations
- Then do full migration

### 2. Use Shopify Plus (if available)
- Higher API rate limits
- Bulk operations API
- Better support

### 3. Time Migration Carefully
- Weekday mornings work well
- Avoid holiday seasons
- Allow 2-4 hours for large stores

### 4. Monitor Performance
- Track API rate limits
- Monitor database performance
- Watch for slow queries

### 5. Preserve SEO
- Redirect every URL
- Submit new sitemap
- Monitor search console
- Expect 2-4 week ranking fluctuation

### 6. Customer Communication
- Notify customers in advance
- Explain benefits of new platform
- Provide clear instructions
- Offer support during transition

### 7. Staff Training
- Train on new admin interface
- Document new processes
- Test order management workflows
- Prepare customer service scripts

## Troubleshooting

### Rate Limit Issues

```bash
# Check current rate limit
curl -X GET "https://your-store.myshopify.com/admin/api/2024-01/shop.json" \
  -H "X-Shopify-Access-Token: YOUR_TOKEN" \
  -i | grep -i "X-Shopify-Shop-Api-Call-Limit"

# Add delays between requests
sleep 0.5

# Use bulk operations for large datasets
```

### Memory Issues

```bash
# Process in smaller chunks
# Instead of all products at once, process 100 at a time

# Use pagination
page = 1
per_page = 100
loop do
  products = fetch_products(page: page, per_page: per_page)
  break if products.empty?
  
  products.each { |p| migrate_product(p) }
  page += 1
end
```

### Data Validation Failures

```bash
# Log validation errors
each product do
  begin
    migrate_product(product)
  rescue ValidationError => e
    puts "Validation failed for #{product['id']}: #{e.message}"
    puts "Data: #{product.inspect}"
    next # Skip to next
  end
end
```

## Support and Resources

- **Shopify API Documentation**: https://shopify.dev/docs/api/admin
- **Shopify API Rate Limits**: https://shopify.dev/docs/api/usage/rate-limits
- **R commerce API Docs**: See docs/api/01-api-design.md
- **R commerce Discord**: https://discord.gg/rcommerce
- **Professional Help**: migration@rcommerce.app

## Example: Complete Migration with Monitoring

```javascript
// complete-migration.js
const { EventEmitter } = require('events');
const fs = require('fs').promises;

class MonitoredMigration extends EventEmitter {
  constructor(config) {
    super();
    this.config = config;
    this.stats = {
      products: { success: 0, failed: 0, skipped: 0 },
      customers: { success: 0, failed: 0, skipped: 0 },
      orders: { success: 0, failed: 0, skipped: 0 }
    };
    this.startTime = Date.now();
  }
  
  async run() {
    this.emit('start', this.stats);
    
    try {
      // Phase 1: Products
      await this.migrateProducts();
      
      // Phase 2: Customers
      await this.migrateCustomers();
      
      // Phase 3: Orders (optional)
      if (this.config.migrateOrders) {
        await this.migrateOrders();
      }
      
      this.emit('complete', this.stats);
      
    } catch (error) {
      this.emit('error', error);
      throw error;
    }
  }
  
  async migrateProducts() {
    this.emit('phase-start', { phase: 'products' });
    
    try {
      // Enhanced product migration with monitoring
      // ... implementation with progress updates
    } finally {
      this.emit('phase-complete', { 
        phase: 'products',
        stats: this.stats.products 
      });
    }
  }
}

// Usage with monitoring
const migration = new MonitoredMigration(config);

migration.on('start', (stats) => {
  console.log('Migration started:', stats);
});

migration.on('phase-start', ({ phase }) => {
  console.log(`Starting ${phase} migration...`);
});

migration.on('phase-complete', ({ phase, stats }) => {
  console.log(`${phase} migration complete:`, stats);
  
  // Send notification
  sendSlackNotification(`${phase} migration: ${JSON.stringify(stats)}`);
});

migration.on('error', (error) => {
  console.error('Migration failed:', error);
  sendSlackNotification(`❌ Migration failed: ${error.message}`);
});

migration.on('complete', (stats) => {
  console.log('Migration completed successfully!', stats);
  sendSlackNotification(` Migration complete: ${JSON.stringify(stats)}`);
});

migration.run().catch(console.error);
```

This guide provides a comprehensive approach to migrating from Shopify to R commerce. Adjust scripts and configurations based on your specific requirements and store complexity.
