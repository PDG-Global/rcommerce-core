# Migration Guides Index

This directory contains comprehensive migration guides for migrating from popular ecommerce platforms to R commerce.

## Available Migration Guides

### 1. [Shopify to R commerce](01-shopify.md)
- Product migration
- Customer migration
- Order history migration
- Theme and frontend transition
- App ecosystem alternatives
- SEO preservation

### 2. [WooCommerce to R commerce](02-woocommerce.md)
- WordPress integration removal
- Product and variant migration
- Customer and order migration
- Plugin ecosystem mapping
- Payment gateway transition
- Theme migration

### 3. [Magento to R commerce](03-magento.md)
- Complex product structure migration
- Customer group mapping
- Order and invoice migration
- Extension ecosystem alternatives
- Performance considerations
- B2B feature migration

### 4. [Medusa.js to R commerce](04-medusa.md)
- API compatibility layer usage
- Direct migration path
- Feature comparison
- Migration script examples

## Migration Approach

### Two Migration Strategies

#### 1. Big Bang Migration
- All data migrated at once
- Downtime required
- Simpler but riskier
- Good for: Small stores, off-season migration

#### 2. Phased Migration
- Migrate in stages
- Zero or minimal downtime
- More complex but safer
- Good for: Large stores, high-traffic stores

### Using the Compatibility Layer

R commerce provides compatibility layers that allow you to:

1. **Run both platforms simultaneously**
   - Use R commerce for new features
   - Keep existing platform running
   - Gradually migrate functionality

2. **Test before full migration**
   - Verify data integrity
   - Test integrations
   - Validate performance

3. **Rollback capability**
   - If issues arise, revert to original platform
   - Minimal business disruption

## Migration Checklist

### Pre-Migration
- [ ] Audit current platform data
- [ ] Clean up unused products/customers/orders
- [ ] Export all data from current platform
- [ ] Set up R commerce environment
- [ ] Choose migration strategy
- [ ] Create migration timeline
- [ ] Prepare rollback plan

### Migration Execution
- [ ] Migrate products and categories
- [ ] Migrate customers
- [ ] Migrate orders (optional, for reporting)
- [ ] Set up payment gateways
- [ ] Configure shipping
- [ ] Set up tax rules
- [ ] Test checkout flow
- [ ] Test order management
- [ ] Test customer accounts

### Post-Migration
- [ ] Verify data integrity
- [ ] Test all integrations
- [ ] Update DNS/point domain
- [ ] Monitor performance
- [ ] Test backup/restore
- [ ] Train staff on new system
- [ ] Update documentation
- [ ] Archive old platform data

## Migration Tools

### Built-in Import Tool (Recommended)

R Commerce includes a built-in import tool accessible via the CLI:

```bash
# Import from Shopify
rcommerce import platform \
  -c config.toml \
  --platform shopify \
  --api-url https://your-store.myshopify.com \
  --api-key YOUR_API_KEY \
  --api-secret YOUR_API_PASSWORD \
  --entities products,customers,orders

# Import from WooCommerce
rcommerce import platform \
  -c config.toml \
  --platform woocommerce \
  --api-url https://your-store.com/wp-json/wc/v3 \
  --api-key YOUR_CONSUMER_KEY \
  --api-secret YOUR_CONSUMER_SECRET

# Import from CSV file
rcommerce import file \
  -c config.toml \
  --file products.csv \
  --format csv \
  --entity products
```

#### Dry Run Mode

Always test your migration first with `--dry-run`:

```bash
rcommerce import platform shopify \
  --api-url https://your-store.myshopify.com \
  --api-key YOUR_API_KEY \
  --api-secret YOUR_API_PASSWORD \
  --dry-run
```

This validates all data and shows what would be imported without modifying your database.

#### Supported Platforms

| Platform | Status | Authentication | Entities |
|----------|--------|----------------|----------|
| Shopify | ✅ Full | API Key + Password | Products, Customers, Orders |
| WooCommerce | ✅ Full | Consumer Key + Secret | Products, Customers, Orders |
| Magento | 🚧 Planned | OAuth/API Token | Products, Customers, Orders |
| Medusa | 🚧 Planned | API Token | Products, Customers, Orders |

#### Supported File Formats

| Format | Status | Use Case |
|--------|--------|----------|
| CSV | ✅ Full | Spreadsheet exports, bulk uploads |
| JSON | ✅ Full | API exports, structured data |
| XML | 🚧 Planned | Legacy system exports |

### Legacy CLI Migration Tool

For advanced migration scenarios, the standalone migration tool is also available:

```bash
# Install migration tool
cargo install rcommerce-migrate

# List available migrations
rcommerce-migrate list

# Analyze current platform
rcommerce-migrate analyze --platform shopify --api-key SHOP_KEY

# Perform migration
rcommerce-migrate run \
  --from shopify \
  --to rcommerce \
  --config migration.toml

# Dry run (test without committing)
rcommerce-migrate run --dry-run

# Verbose logging
rcommerce-migrate run --verbose
```

### Migration Config Format

```toml
# migration.toml
[migration]
source_platform = "shopify"  # or "woocommerce", "magento"
target_platform = "rcommerce"
strategy = "phased"          # or "big_bang"
dry_run = false

[source]
api_key = "YOUR_API_KEY"
api_secret = "YOUR_API_SECRET"
store_url = "https://your-store.myshopify.com"

[target]
r_commerce_url = "https://api.yourstore.com"
r_commerce_api_key = "sk_xxx"

[migration.options]
migrate_products = true
migrate_customers = true
migrate_orders = true
migrate_categories = true
migrate_reviews = false
skip_duplicates = true
update_existing = false
chunk_size = 100
rate_limit_per_second = 2

[migration.mapping]
# Map source fields to target fields
# Shopify product type → R commerce category
product_type_field = "custom.product_type"

# WooCommerce attributes → R commerce tags
attribute_to_tags = true

# Magento customer groups → R commerce groups
customer_group_mapping = "auto"

[migration.validation]
check_inventory = true
verify_pricing = true
validate_addresses = true
run_fraud_check = false
```

## Data Mapping Challenges

### Product Complexity
- **Shopify**: Simple products with variants
- **WooCommerce**: Variable products with attributes
- **Magento**: Complex product types (configurable, bundled, grouped)
- **R commerce**: Unified product model with variants

### Customer Accounts
- Password migration (usually not possible, require reset)
- Customer group structures
- Loyalty points/programs
- Subscription data

### Order History
- Order numbering schemes
- Order status mapping
- Partial refunds/exchanges
- Multi-currency orders

### SEO Preservation
- URL redirects (critical!)
- Meta data migration
- Sitemap structure
- Search engine indexing

## Platform-Specific Considerations

### Shopify
- **API Rate Limits**: 2 requests/second by default
- **Plus Stores**: Higher limits, better for migration
- **Shopify Plus**: Use GraphQL Admin API for bulk operations
- **Assets**: Images, files need separate download/upload

### WooCommerce
- **WordPress Integration**: Must handle WP users, posts
- **Plugin Data**: Many plugins store data in postmeta
- **Custom Attributes**: Variable mapping required
- **Database Direct**: Can use direct database access for speed

### Magento
- **EAV Model**: Complex database structure
- **Multiple Stores**: Store view mapping
- **Custom Attributes**: Extensive attribute systems
- **Enterprise Features**: B2B features may not have equivalents

## Common Pitfalls

1. **Underestimating Time**
   - Large catalogs take days, not hours
   - Order history migration is slow
   - Testing phase often takes longer than expected

2. **Data Loss Risks**
   - Always backup before migrating
   - Test integrity after migration
   - Keep original data until confirmed

3. **SEO Impact**
   - Missing redirects = lost rankings
   - Change content = re-indexing time
   - Plan for temporary traffic drop

4. **Integration Breakage**
   - Payment webhooks need updating
   - Shipping integrations may break
   - CRM/email marketing connections
   - Accounting system integrations

5. **Performance Issues**
   - New platform may need tuning
   - Different caching strategies
   - Database optimization needed

## Testing Checklist

- [ ] Product catalog complete (count matches)
- [ ] Product details accurate (prices, images, descriptions)
- [ ] Customer data migrated (emails, addresses)
- [ ] Order history accessible (for customer service)
- [ ] Payment processing works (test transaction)
- [ ] Shipping calculations accurate
- [ ] Tax rules applied correctly
- [ ] Email notifications sent
- [ ] Webhooks received by integrations
- [ ] Mobile app works (if applicable)
- [ ] Admin functions work
- [ ] Reports generate correctly
- [ ] Backup and restore tested
- [ ] Performance acceptable
- [ ] SEO redirects working
- [ ] SSL certificates valid
- [ ] Monitoring alerts configured

## Support and Resources

- [R commerce Documentation](https://github.com/creativebastard/rcommerce)
- [Migration Tool Repository](https://github.com/creativebastard/rcommerce)
- [Community Forum](https://forum.rcommerce.app)
- [Migration Webinars](https://rcommerce.app/webinars)

## Professional Migration Services

For complex migrations, consider:

1. **R commerce Partners**: Certified migration specialists
2. **Platform Partners**: Shopify, WooCommerce, Magento agencies
3. **Data Migration Tools**: LitExtension, Cart2Cart

Contact sales@rcommerce.app for professional migration assistance.
