# Medusa.js to R commerce Migration Guide

## Overview

Medusa.js to R commerce is the most straightforward migration due to architectural similarities. Both are headless, API-first platforms built on modern technology stacks. This guide focuses on leveraging API compatibility and direct data mapping.

## Pre-Migration Analysis

### Audit Medusa.js Store

```javascript
// audit-medusa.js
const axios = require('axios');

const medusaClient = axios.create({
  baseURL: 'https://api.your-medusa-store.com',
  headers: {
    'x-medusa-access-token': process.env.MEDUSA_API_KEY
  }
});

async function auditMedusaStore() {
  console.log('Auditing Medusa.js store...');
  
  // Products
  const products = await medusaClient.get('/store/products?limit=1');
  console.log(`Products: ${products.data.count || 'Unknown'}`);
  
  // Customers
  const customers = await medusaClient.get('/admin/customers?limit=1');
  console.log(`Customers: ${customers.data.count || 'Unknown'}`);
  
  // Orders
  const orders = await medusaClient.get('/admin/orders?limit=1');
  console.log(`Orders: ${orders.data.count || 'Unknown'}`);
  
  // Regions (Medusa-specific)
  const regions = await medusaClient.get('/admin/regions?limit=1');
  console.log(`Regions: ${regions.data.count || 'Unknown'}`);
  
  // Sales Channels
  const channels = await medusaClient.get('/admin/sales-channels?limit=1');
  console.log(`Sales Channels: ${channels.data.count || 'Unknown'}`);
  
  // Price Lists
  const priceLists = await medusaClient.get('/admin/price-lists?limit=1');
  console.log(`Price Lists: ${priceLists.data.count || 'Unknown'}`);
  
  console.log('Audit complete!');
}

auditMedusaStore().catch(console.error);
```

### Medusa.js Data Structure Map

| Medusa.js Concept | R commerce Equivalent | Notes |
|-------------------|-----------------------|-------|
| **Regions** | Shipping Zones + Currencies | Direct mapping |
| **Sales Channels** | Not directly supported | Use multiple stores or tagging |
| **Price Lists** | Customer Groups + Discounts | Similar functionality |
| **Swaps/Returns** | Returns API | R commerce has simpler model |
| **Claims** | Not directly supported | Use custom implementation |
| **Draft Orders** | Manual Orders | Similar functionality |
| **Published API Keys** | API Keys | R commerce has different system |
| **Gift Cards** | Gift Cards | Supported in R commerce |
| **Discount Conditions** | Discount Rules | R commerce has different engine |

## Export Medusa.js Data

### Using Medusa.js Admin API

```javascript
// export-medusa.js
const fs = require('fs').promises;
const path = require('path');

class MedusaExporter {
  constructor(config) {
    this.client = axios.create({
      baseURL: config.url,
      headers: {
        'x-medusa-access-token': config.api_key
      }
    });
    this.exportDir = path.join(process.cwd(), 'medusa-export');
  }
  
  async exportAll() {
    await fs.mkdir(this.exportDir, { recursive: true });
    
    console.log('Exporting Medusa.js data...');
    
    await this.exportProducts();
    await this.exportCustomers();
    await this.exportOrders();
    await this.exportRegions();
    await this.exportDiscounts();
    await this.exportPriceLists();
    
    console.log('Export complete!');
  }
  
  async exportProducts() {
    console.log('Exporting products...');
    let page = 0;
    let hasMore = true;
    const allProducts = [];
    
    while (hasMore) {
      const response = await this.client.get(`/admin/products`, {
        params: { limit: 100, offset: page * 100 }
      });
      
      const products = response.data.products;
      allProducts.push(...products);
      
      hasMore = products.length === 100;
      page++;
      
      console.log(`  - Page ${page}: ${products.length} products`);
    }
    
    await fs.writeFile(
      path.join(this.exportDir, 'products.json'),
      JSON.stringify(allProducts, null, 2)
    );
    
    console.log(` Exported ${allProducts.length} products`);
  }
  
  async exportCustomers() {
    console.log('Exporting customers...');
    let page = 0;
    let hasMore = true;
    const allCustomers = [];
    
    while (hasMore) {
      const response = await this.client.get(`/admin/customers`, {
        params: { limit: 100, offset: page * 100 }
      });
      
      const customers = response.data.customers;
      allCustomers.push(...customers);
      
      hasMore = customers.length === 100;
      page++;
      
      console.log(`  - Page ${page}: ${customers.length} customers`);
    }
    
    await fs.writeFile(
      path.join(this.exportDir, 'customers.json'),
      JSON.stringify(allCustomers, null, 2)
    );
    
    console.log(` Exported ${allCustomers.length} customers`);
  }
  
  async exportOrders() {
    console.log('Exporting orders...');
    let page = 0;
    let hasMore = true;
    const allOrders = [];
    
    while (hasMore) {
      const response = await this.client.get(`/admin/orders`, {
        params: { limit: 100, offset: page * 100 }
      });
      
      const orders = response.data.orders;
      allOrders.push(...orders);
      
      hasMore = orders.length === 100;
      page++;
      
      console.log(`  - Page ${page}: ${orders.length} orders`);
    }
    
    await fs.writeFile(
      path.join(this.exportDir, 'orders.json'),
      JSON.stringify(allOrders, null, 2)
    );
    
    console.log(` Exported ${allOrders.length} orders`);
  }
  
  async exportRegions() {
    console.log('Exporting regions...');
    const response = await this.client.get(`/admin/regions`);
    const regions = response.data.regions;
    
    await fs.writeFile(
      path.join(this.exportDir, 'regions.json'),
      JSON.stringify(regions, null, 2)
    );
    
    console.log(` Exported ${regions.length} regions`);
  }
  
  async exportDiscounts() {
    console.log('Exporting discounts...');
    let page = 0;
    let hasMore = true;
    const allDiscounts = [];
    
    while (hasMore) {
      const response = await this.client.get(`/admin/discounts`, {
        params: { limit: 100, offset: page * 100 }
      });
      
      const discounts = response.data.discounts;
      allDiscounts.push(...discounts);
      
      hasMore = discounts.length === 100;
      page++;
    }
    
    await fs.writeFile(
      path.join(this.exportDir, 'discounts.json'),
      JSON.stringify(allDiscounts, null, 2)
    );
    
    console.log(` Exported ${allDiscounts.length} discounts`);
  }
  
  async exportPriceLists() {
    console.log('Exporting price lists...');
    let page = 0;
    let hasMore = true;
    const allPriceLists = [];
    
    while (hasMore) {
      const response = await this.client.get(`/admin/price-lists`, {
        params: { limit: 100, offset: page * 100 }
      });
      
      const priceLists = response.data.price_lists;
      allPriceLists.push(...priceLists);
      
      hasMore = priceLists.length === 100;
      page++;
    }
    
    await fs.writeFile(
      path.join(this.exportDir, 'price-lists.json'),
      JSON.stringify(allPriceLists, null, 2)
    );
    
    console.log(` Exported ${allPriceLists.length} price lists`);
  }
}

// Usage
const exporter = new MedusaExporter({
  url: 'https://api.your-medusa-store.com',
  api_key: process.env.MEDUSA_API_KEY
});

exporter.exportAll().catch(console.error);
```

## Data Mapping Reference

### Product Mapping (Simple Products)

```javascript
// Medusa Product → R commerce Product
{
  // Medusa
  id: "prod_123",
  title: "T-Shirt",
  subtitle: "Premium Cotton",
  description: "High quality t-shirt",
  handle: "t-shirt-premium",
  is_giftcard: false,
  status: "published",
  images: [{ url: "...", metadata: {} }],
  thumbnail: "https://...",
  options: [{ id: "opt_1", title: "Size", values: ["S", "M", "L"] }],
  variants: [
    {
      id: "var_1",
      title: "S",
      prices: [{ currency_code: "usd", amount: 1999 }],
      options: [{ option_id: "opt_1", value: "S" }],
      inventory_quantity: 100,
      manage_inventory: true
    }
  ],
  tags: [{ value: "clothing" }],
  type: { value: "shirt" },
  collection_id: "col_123",
  
  // R commerce equivalent
  id: "generated_uuid",
  name: "T-Shirt",
  slug: "t-shirt-premium",
  description: "High quality t-shirt",
  sku: "TSHIRT-001",
  price: 19.99,
  status: "active", // published → active
  images: [{ url: "...", alt: "", position: 0 }],
  category_id: "category_uuid",
  tags: ["clothing", "shirt"], // tags + type
  inventory_quantity: 100, // From default variant
  inventory_policy: "deny",
  requires_shipping: true,
  is_gift_card: false,
  variants: [
    {
      id: "variant_uuid",
      title: "S",
      price: 19.99,
      inventory_quantity: 100,
      options: [{ name: "Size", value: "S" }]
    }
  ]
}
```

### Region Mapping

Medusa regions are complex - they handle currencies, taxes, payment providers, and fulfillment:

```javascript
// Medusa Region → R commerce Mapping
{
  // Medusa Region
  id: "reg_01",
  name: "United States",
  currency_code: "usd",
  tax_rate: 0,
  tax_code: null,
  gift_cards_taxable: true,
  automatic_taxes: true,
  payment_providers: ["stripe"],
  fulfillment_providers: ["manual"],
  countries: [{ iso_2: "us", display_name: "United States" }],
  
  // R commerce Mapping (multiple configs)
  // 1. Currency Configuration
  ["currencies"]
  supported_currencies = ["USD"]
  default_currency = "USD"
  
  // 2. Tax Configuration
  ["tax_rates"]
  [[tax_rates]]
  name = "US Tax"
  rate = 0.0
  country = "US"
  
  // 3. Payment Providers
  ["payments.stripe"]
  enabled = true
  secret_key = "sk_..."
  
  // 4. Shipping Zones
  [[shipping.zones]]
  name = "United States"
  countries = ["US"]
}
```

### Customer Group Mapping

```javascript
// Medusa Customer Group → R commerce Customer Group
{
  // Medusa
  id: "cgroup_01",
  name: "Wholesale",
  metadata: {},
  customers: [...],
  price_lists: [...],
  
  // R commerce
  id: "generated_uuid",
  name: "Wholesale",
  description: "Wholesale customers",
  meta_data: {
    medusa_id: "cgroup_01"
  }
}
```

## Migration Scripts

### Direct Database Migration (Recommended)

```javascript
// migrate-medusa-direct.js
const { Client } = require('pg'); // Medusa uses PostgreSQL
const axios = require('axios');

class DirectMedusaMigrator {
  constructor(medusaConfig, rcommerceConfig) {
    this.medusaDb = new Client(medusaConfig.database);
    this.rcommerceClient = axios.create({
      baseURL: rcommerceConfig.url,
      headers: { 'Authorization': `Bearer ${rcommerceConfig.api_key}` }
    });
  }
  
  async connect() {
    await this.medusaDb.connect();
    console.log('Connected to Medusa database');
  }
  
  async disconnect() {
    await this.medusaDb.end();
  }
  
  async migrateProducts() {
    console.log('Migrating products directly...');
    
    // Get products with all their data in a single query
    const query = `
      SELECT 
        p.id,
        p.title,
        p.subtitle,
        p.description,
        p.handle,
        p.thumbnail,
        p.type_id,
        p.collection_id,
        p.is_giftcard,
        p.status,
        p.discountable,
        p.created_at,
        p.updated_at,
        p.category_id,
        COALESCE(json_agg(DISTINCT jsonb_build_object('id', pi.id, 'url', pi.url, 'metadata', pi.metadata)) FILTER (WHERE pi.id IS NOT NULL), '[]') as images,
        COALESCE(json_agg(DISTINCT jsonb_build_object('id', pt.id, 'value', pt.value)) FILTER (WHERE pt.id IS NOT NULL), '[]') as tags,
        COALESCE(json_agg(DISTINCT jsonb_build_object('id', pov.id, 'value', pov.value)) FILTER (WHERE pov.id IS NOT NULL), '[]') as options,
        COALESCE(json_agg(DISTINCT jsonb_build_object(
          'id', pv.id,
          'title', pv.title,
          'sku', pv.sku,
          'barcode', pv.barcode,
          'inventory_quantity', pv.inventory_quantity,
          'manage_inventory', pv.manage_inventory,
          'allow_backorder', pv.allow_backorder,
          'variant_rank', pv.variant_rank,
          'metadata', pv.metadata,
          'prices', (SELECT json_agg(jsonb_build_object('currency', pp.currency_code, 'amount', pp.amount)) 
                     FROM product_variant pp WHERE pp.variant_id = pv.id),
          'options', (SELECT json_agg(jsonb_build_object('value', pvo.value, 'option_id', pvo.option_id)) 
                      FROM product_variant pvo WHERE pvo.variant_id = pv.id)
        )) FILTER (WHERE pv.id IS NOT NULL), '[]') as variants
      FROM product p
      LEFT JOIN product_image pi ON p.id = pi.product_id
      LEFT JOIN product_tag pt ON p.id = pt.product_id
      LEFT JOIN product_option pov ON p.id = pov.product_id
      LEFT JOIN product_variant pv ON p.id = pv.product_id
      GROUP BY p.id
    `;
    
    const result = await this.medusaDb.query(query);
    
    for (const product of result.rows) {
      try {
        // Transform to R commerce format
        const rcommerceProduct = {
          name: product.title,
          subtitle: product.subtitle,
          slug: product.handle,
          description: product.description,
          sku: product.variants?.[0]?.sku || null,
          price: product.variants?.[0]?.prices?.[0]?.amount ? 
                 product.variants[0].prices[0].amount / 100 : 0,
          status: product.status === 'published' ? 'active' : 'draft',
          is_gift_card: product.is_giftcard,
          images: product.images.map(img => ({
            url: img.url,
            alt: img.metadata?.alt || '',
            position: img.metadata?.position || 0
          })),
          tags: product.tags.map(tag => tag.value),
          inventory_quantity: product.variants?.[0]?.inventory_quantity || 0,
          inventory_policy: product.variants?.[0]?.allow_backorder ? 
                           'continue' : 'deny',
          variants: product.variants.map(variant => ({
            title: variant.title,
            sku: variant.sku,
            price: variant.prices?.[0]?.amount ? variant.prices[0].amount / 100 : 0,
            inventory_quantity: variant.inventory_quantity,
            options: variant.options.map(opt => ({
              name: product.options.find(o => o.id === opt.option_id)?.title || 'Option',
              value: opt.value
            }))
          })),
          meta_data: {
            medusa: {
              id: product.id,
              collection_id: product.collection_id,
                discountable: product.discountable,
              created_at: product.created_at,
              updated_at: product.updated_at
            }
          }
        };
        
        // Create in R commerce
        const response = await this.rcommerceClient.post('/v1/products', rcommerceProduct);
        console.log(` Migrated product: ${rcommerceProduct.name}`);
        
      } catch (error) {
        console.error(` Failed to migrate product: ${product.title}`, error.message);
      }
    }
  }
  
  async migrateRegions() {
    console.log('Migrating regions...');
    
    const query = `
      SELECT 
        r.id,
        r.name,
        r.currency_code,
        r.tax_rate,
        r.tax_code,
        r.automatic_taxes,
        r.gift_cards_taxable,
        r.payment_providers,
        r.fulfillment_providers,
        COALESCE(json_agg(DISTINCT jsonb_build_object('iso_2', rc.iso_2, 'display_name', rc.display_name)) 
                 FILTER (WHERE rc.iso_2 IS NOT NULL), '[]') as countries
      FROM region r
      LEFT JOIN region_countries rc_link ON r.id = rc_link.region_id
      LEFT JOIN country rc ON rc_link.country_code = rc.iso_2
      GROUP BY r.id
    `;
    
    const result = await this.medusaDb.query(query);
    
    for (const region of result.rows) {
      try {
        // Map region to R commerce configuration
        const regionConfig = {
          name: region.name,
          currency: region.currency_code.toUpperCase(),
          countries: region.countries.map(c => c.iso_2),
          tax_config: {
            rate: parseFloat(region.tax_rate) || 0,
            automatic: region.automatic_taxes,
            gift_cards_taxable: region.gift_cards_taxable
          },
          payment_providers: region.payment_providers,
          fulfillment_providers: region.fulfillment_providers
        };
        
        // Store region config (would need to be applied to R commerce system config)
        console.log(` Mapped region: ${region.name} (${region.currency_code})`);
        
      } catch (error) {
        console.error(` Failed to migrate region: ${region.name}`, error.message);
      }
    }
  }
}

// Usage
const migrator = new DirectMedusaMigrator({
  database: {
    host: process.env.MEDUSA_DB_HOST,
    database: process.env.MEDUSA_DB_NAME,
    user: process.env.MEDUSA_DB_USER,
    password: process.env.MEDUSA_DB_PASS
  }
}, {
  url: process.env.RCOMMERCE_URL,
  api_key: process.env.RCOMMERCE_API_KEY
});

migrator.connect()
  .then(() => migrator.migrateProducts())
  .then(() => migrator.migrateRegions())
  .then(() => migrator.disconnect())
  .catch(console.error);
```

## Migration Configuration

```toml
# migration-medusa.toml

[migration]
strategy = "direct"  # or "compatibility_layer"
dry_run = false

[source.medusa]
api_url = "https://api.medusa-store.com"
api_key = "YOUR_MEDUSA_API_KEY"

[target.rcommerce]
api_url = "https://api.rcommerce-store.com"
api_key = "YOUR_RCOMMERCE_API_KEY"

[migration.mapping]
# Feature flags for migration
transfer_products = true
transfer_customers = true
transfer_orders = true
transfer_regions = true
transfer_discounts = true
transfer_price_lists = true

# Special mappings
map_medusa_regions = true
map_price_lists_to_customer_groups = true
map_sales_channels_to_tags = true

# Optional: Migrate to R commerce compatibility layer
[compatibility]
enabled = false  # Set to true for gradual migration
maintain_medusa_compatibility = false
```

## Post-Migration Steps

1. **Verify Data Integrity**
   ```bash
   # Compare counts
   medusa_count=$(curl -s -H "x-medusa-access-token: $MEDUSA_KEY" $MEDUSA_URL/admin/products | jq '.count')
   rcommerce_count=$(curl -s -H "Authorization: Bearer $RC_KEY" $RC_URL/v1/products?per_page=1 | jq '.meta.pagination.total')
   
   echo "Products - Medusa: $medusa_count, R commerce: $rcommerce_count"
   ```

2. **Test Critical Paths**
   - Product browsing
   - Variant selection
   - Add to cart
   - Checkout flow
   - Payment processing
   - Order confirmation

3. **Update Frontend**
   - Change API endpoints
   - Update authentication headers
   - Test all API calls
   - Verify error handling

4. **Update Webhooks**
    ```javascript
    // Reconfigure Medusa admin to point to R commerce hooks
    const { data } = await medusa.admin.store.update({
      webhook_url: "https://api.rcommerce.app/webhooks/medusa"
    });
    ```

## Differences to Note

### Medusa.js Features Not in R commerce

- **Sales Channels**: Use tagging or customer groups
- **Regions with multiple currencies**: R commerce simpler model
- **Draft Orders**: R commerce has manual orders
- **Claims**: Custom implementation needed
- **Swaps**: R commerce has simpler return system
- **Published API Keys**: Different authentication model

### R commerce Features Not in Medusa.js

- **WooCommerce API compatibility**: Useful for existing integrations
- **Dianxiaomi integration**: Chinese-specific features
- **Advanced fraud detection**: Built-in system
- **PostgreSQL database support**
- **Cross-platform deployments**: FreeBSD support

This migration is the most straightforward due to architectural similarities between Medusa.js and R commerce.
