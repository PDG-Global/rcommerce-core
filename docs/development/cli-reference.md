# R Commerce CLI Reference

The R Commerce CLI (`rcommerce`) provides commands for server management, database operations, API key administration, and interactive product/customer creation.

## Global Options

```bash
rcommerce [OPTIONS] <COMMAND>

Options:
  -c, --config <CONFIG>        Configuration file path
  -l, --log-level <LOG_LEVEL>  Set log level (debug, info, warn, error)
  -h, --help                   Print help
  -V, --version                Print version
```

## Setup Wizard

The interactive setup wizard helps you configure a new R Commerce instance:

```bash
rcommerce setup [OPTIONS]

Options:
  -o, --output <OUTPUT>  Output configuration file path
```

**What the wizard configures:**

1. **Store Information** - Store name and default currency
2. **Database** - PostgreSQL configuration
3. **Database Setup** - Runs migrations, handles existing databases
4. **Data Import** - Optional import from WooCommerce, Shopify, Magento, or Medusa
5. **Server** - Bind address, port, worker threads
6. **Cache** - In-memory or Redis caching
7. **Security** - JWT secrets, rate limiting
8. **Media Storage** - Local filesystem or S3
9. **TLS/SSL** - Let's Encrypt (auto) or manual certificates
10. **Payments** - Stripe and other payment gateways
11. **Notifications** - Email (SMTP) configuration

**Examples:**

```bash
# Run interactive setup
rcommerce setup

# Save to specific file
rcommerce setup -o ./production.toml
```

**Database Setup Options:**

When the wizard detects an existing database, it offers:
- **Keep existing data** - Skip migrations
- **Reset database** - Delete all data and start fresh
- **Exit** - Investigate manually

**Data Import:**

The wizard can import from:
- WooCommerce (via REST API)
- Shopify (via Admin API)
- Magento (via REST API)
- Medusa (via REST API)

Import settings include:
- Store URL and API credentials
- Default currency for imported records
- Whether to update existing records

## Interactive Shell

The `shell` command launches an interactive REPL (Read-Eval-Print Loop) for managing your R Commerce installation:

```bash
rcommerce shell -c config.toml
```

This provides a command-line interface for listing products, orders, customers, and more without leaving your terminal.

### Shell Commands

Once inside the shell, you can use the following commands:

| Command | Description | Example |
|---------|-------------|---------|
| `help`, `h`, `?` | Show available commands | `help` |
| `exit`, `quit`, `q` | Exit the shell | `exit` |
| `clear`, `cls` | Clear the screen | `clear` |
| `dashboard`, `dash`, `d` | Show dashboard overview | `dashboard` |
| `status`, `st` | Show database status | `status` |
| `list <entity> [limit]` | List entities | `list products 10` |
| `get <entity> <id>` | Get entity details | `get product abc-123` |
| `create <entity>` | Create new entity | `create product` |
| `delete <entity> <id>` | Delete an entity | `delete customer xyz-789` |
| `search <entity> <query>` | Search for entities | `search products laptop` |

**Entity shortcuts:**
- `p` → product(s)
- `o` → order(s)
- `c` → customer(s)
- `k`, `keys` → api-keys

### Shell Examples

```
$ rcommerce shell -c config.toml

╔═══════════════════════════════════════════════════════════════╗
║                                                               ║
║           🛒 R Commerce Interactive Shell                     ║
║                                                               ║
║     Type 'help' for available commands or 'exit' to quit      ║
║                                                               ║
╚═══════════════════════════════════════════════════════════════╝

rcommerce> dashboard

📊 Dashboard

Key Metrics:
  Products:            150
  Orders:              42
  Customers:           28
  Total Revenue:       $12,450.00

Recent Orders:
  ID                                    Customer              Status       Total        Created
  ----------------------------------------------------------------------------------------------------
  550e8400-e29b-41d4-a716-446655440000  john@example.com      completed    $299.99      2024-01-31
  550e8400-e29b-41d4-a716-446655440001  jane@example.com      pending      $149.50      2024-01-30

rcommerce> list products 5

Products (showing 5)
  ID                                    Title                          Price      Currency   Status
  ----------------------------------------------------------------------------------------------------
  550e8400-e29b-41d4-a716-446655440000  Premium T-Shirt                29.99      USD        ✓ Active
  550e8400-e29b-41d4-a716-446655440001  Wireless Headphones            149.99     USD        ✓ Active

rcommerce> search products laptop

Products matching 'laptop' (3)
  ID                                    Title                          Price      Currency   Status
  ----------------------------------------------------------------------------------------------------
  550e8400-e29b-41d4-a716-446655440002  Gaming Laptop Pro              1299.99    USD        ✓ Active

rcommerce> exit

Bye: Goodbye! 👋
```

### Interactive Creation in Shell

The shell supports interactive creation of products and customers:

```
rcommerce> create product

📦 Create New Product
Product title: Premium T-Shirt
URL slug [premium-t-shirt]: premium-t-shirt
Product type:
  > Simple
    Variable
    Digital
    Bundle
Price: 29.99
...

✓ Product created successfully!
  ID:    550e8400-e29b-41d4-a716-446655440000
  Title: Premium T-Shirt
```

## Commands

### Server

Start the API server:

```bash
rcommerce server [OPTIONS]

Options:
  -H, --host <HOST>      Bind address [default: 0.0.0.0]
  -P, --port <PORT>      Port number [default: 8080]
      --skip-migrate     Skip automatic database migration
```

**Examples:**

```bash
# Start with default config
rcommerce server

# Start on custom port
rcommerce server -P 3000

# Start without migrations
rcommerce server --skip-migrate
```

### Database

Database management commands:

```bash
rcommerce db <COMMAND>

Commands:
  migrate    Run database migrations
  reset      Reset database (DANGEROUS - deletes all data)
  seed       Seed database with demo data
  status     Show database status
```

**Examples:**

```bash
# Run migrations
rcommerce db migrate -c config.toml

# Check database status
rcommerce db status -c config.toml

# Reset database (with confirmation)
rcommerce db reset -c config.toml

# Seed with demo data
rcommerce db seed -c config.toml
```

### API Key Management

Manage API keys for service-to-service authentication:

```bash
rcommerce api-key <COMMAND>

Commands:
  list       List all API keys
  create     Create a new API key
  get        Get API key details
  revoke     Revoke an API key
  delete     Delete an API key permanently
```

#### List API Keys

```bash
rcommerce api-key list [OPTIONS]

Options:
  -u, --customer-id <ID>  Filter by customer ID
```

**Example:**

```bash
rcommerce api-key list -c config.toml
```

Output:
```
API Keys
Prefix       Name                 Scopes                         Active     Expires
------------------------------------------------------------------------------------------
aB3dEfGh     Production Backend   read, write                    ✓          Never
Xy9zZzZz     Test Key             read                           ✗          2024-12-31
```

#### Create API Key

```bash
rcommerce api-key create [OPTIONS]

Options:
  -u, --customer-id <ID>     Customer ID (optional for system keys)
  -n, --name <NAME>          Key name/description
  -s, --scopes <SCOPES>      Scopes (comma-separated) [default: read]
  -e, --expires-days <DAYS>  Expiration in days (optional)
```

**Example:**

```bash
rcommerce api-key create \
  -c config.toml \
  --name "Production Backend" \
  --scopes "read,write"
```

Output:
```
✅ API Key created successfully!

IMPORTANT: Copy this key now - it won't be shown again!

  Key: aB3dEfGh.sEcReTkEy123456789

  Prefix:      aB3dEfGh
  Name:        Production Backend
  Scopes:      read, write
  Customer ID: System
  Expires:     Never
```

#### Get API Key Details

```bash
rcommerce api-key get <PREFIX>
```

**Example:**

```bash
rcommerce api-key get -c config.toml aB3dEfGh
```

Output:
```
API Key Details
  Prefix:       aB3dEfGh
  Name:         Production Backend
  Scopes:       read, write
  Active:       ✓ Yes
  Customer ID:  System
  Created:      2024-01-31 10:30:00 UTC
  Updated:      2024-01-31 10:30:00 UTC
  Expires:      Never
  Last Used:    Never
```

#### Revoke API Key

```bash
rcommerce api-key revoke [OPTIONS] <PREFIX>

Options:
  -r, --reason <REASON>  Reason for revocation
```

**Example:**

```bash
rcommerce api-key revoke \
  -c config.toml \
  aB3dEfGh \
  --reason "Key compromised"
```

#### Delete API Key

Permanently delete an API key (irreversible):

```bash
rcommerce api-key delete [OPTIONS] <PREFIX>

Options:
      --force  Skip confirmation
```

**Example:**

```bash
# With confirmation prompt
rcommerce api-key delete -c config.toml aB3dEfGh

# Skip confirmation
rcommerce api-key delete -c config.toml aB3dEfGh --force
```

### Product Management

```bash
rcommerce product <COMMAND>

Commands:
  list       List products
  create     Create a product (interactive)
  get        Get product details
  update     Update a product
  delete     Delete a product
```

#### List Products

```bash
rcommerce product list -c config.toml
```

Output:
```
Products
ID                                    Title                          Price      Currency   Status
----------------------------------------------------------------------------------------------------
550e8400-e29b-41d4-a716-446655440000  Premium T-Shirt                29.99      USD        ✓ Active
550e8400-e29b-41d4-a716-446655440001  Wireless Headphones            149.99     USD        ✓ Active

Total: 2 products
```

#### Create Product (Interactive)

```bash
rcommerce product create -c config.toml
```

This command launches an interactive prompt that guides you through product creation:

```
📦 Create New Product
Press Ctrl+C to cancel at any time.

Product title: Premium T-Shirt
URL slug [premium-t-shirt]: premium-t-shirt
Product type:
  > Simple
    Variable
    Digital
    Bundle
Price: 29.99
Currency:
  > USD
    EUR
    GBP
    JPY
    AUD
    CAD
    CNY
    HKD
    SGD
SKU (optional): TSHIRT-001
Inventory quantity [0]: 100
Description (optional): High quality cotton t-shirt
Make product active? [Y/n]: y
Mark as featured? [y/N]: n

📋 Product Summary
  Title:       Premium T-Shirt
  Slug:        premium-t-shirt
  Type:        Simple
  Price:       29.99 USD
  SKU:         TSHIRT-001
  Inventory:   100
  Description: High quality cotton t-shirt
  Active:      Yes
  Featured:    No

Create this product? [Y/n]: y

✅ Product created successfully!
  ID:    550e8400-e29b-41d4-a716-446655440000
  Title: Premium T-Shirt
  Slug:  premium-t-shirt
  Price: 29.99 USD
```

**Interactive prompts include:**
- Product title (required, max 255 chars)
- URL slug (auto-generated from title, editable)
- Product type selection (Simple/Variable/Digital/Bundle)
- Price (numeric validation)
- Currency selection (USD/EUR/GBP/JPY/AUD/CAD/CNY/HKD/SGD)
- SKU (optional, max 100 chars)
- Inventory quantity (default: 0)
- Description (optional)
- Active status (default: Yes)
- Featured status (default: No)

#### Get Product Details

```bash
rcommerce product get -c config.toml <product-id>
```

**Example:**

```bash
rcommerce product get -c config.toml 550e8400-e29b-41d4-a716-446655440000
```

Output:
```
Product Details
  ID:          550e8400-e29b-41d4-a716-446655440000
  Title:       Premium T-Shirt
  Slug:        premium-t-shirt
  Price:       29.99 USD
  Status:      ✓ Active
  Inventory:   100
  Created:     2024-01-31 10:30:00 UTC
  Description: High quality cotton t-shirt
```

#### Delete Product

```bash
rcommerce product delete -c config.toml <product-id>
```

**Example:**

```bash
rcommerce product delete -c config.toml 550e8400-e29b-41d4-a716-446655440000
```

This will prompt for confirmation:
```
⚠️  Product deletion
Type 'yes' to delete product '550e8400-e29b-41d4-a716-446655440000': yes
✅ Product '550e8400-e29b-41d4-a716-446655440000' deleted
```

### Order Management

```bash
rcommerce order <COMMAND>

Commands:
  list       List orders
  get        Get order details
  create     Create a test order
  update     Update order status
```

#### List Orders

```bash
rcommerce order list -c config.toml
```

Output:
```
Orders
ID                                    Customer             Status       Total           Created
----------------------------------------------------------------------------------------------------
550e8400-e29b-41d4-a716-446655440000  john@example.com     pending      149.99          2024-01-31
550e8400-e29b-41d4-a716-446655440001  jane@example.com     completed    299.98          2024-01-30

Total: 2 orders
```

### Customer Management

```bash
rcommerce customer <COMMAND>

Commands:
  list       List customers
  get        Get customer details
  create     Create a customer (interactive)
```

#### List Customers

```bash
rcommerce customer list -c config.toml
```

Output:
```
Customers
ID                                    Email                          Name                 Created
----------------------------------------------------------------------------------------------------
550e8400-e29b-41d4-a716-446655440000  john@example.com               John Doe             2024-01-31
550e8400-e29b-41d4-a716-446655440001  jane@example.com               Jane Smith           2024-01-30

Total: 2 customers
```

#### Create Customer (Interactive)

```bash
rcommerce customer create -c config.toml
```

This command launches an interactive prompt that guides you through customer creation:

```
👤 Create New Customer
Press Ctrl+C to cancel at any time.

Email address: john@example.com
First name: John
Last name: Doe
Phone number (optional): +1234567890
Preferred currency:
  > USD
    EUR
    GBP
    JPY
    AUD
    CAD
    CNY
    HKD
    SGD
Accepts marketing emails? [y/N]: n
Password: ********
Confirm password: ********

📋 Customer Summary
  Name:              John Doe
  Email:             john@example.com
  Phone:             +1234567890
  Currency:          USD
  Accepts Marketing: No

Create this customer? [Y/n]: y

✅ Customer created successfully!
  ID:    550e8400-e29b-41d4-a716-446655440000
  Name:  John Doe
  Email: john@example.com
```

**Interactive prompts include:**
- Email address (required, validated)
- First name (required, max 100 chars)
- Last name (required, max 100 chars)
- Phone number (optional)
- Preferred currency selection
- Marketing consent (default: No)
- Password (min 8 chars, with confirmation)

#### Get Customer Details

```bash
rcommerce customer get -c config.toml <customer-id>
```

### Configuration

Display the loaded configuration:

```bash
rcommerce config -c config.toml
```

### Import

Import data from external platforms or files:

```bash
rcommerce import <COMMAND>

Commands:
  platform   Import from ecommerce platforms (Shopify, WooCommerce, etc.)
  file       Import from file (CSV, JSON, XML)
```

#### Import from Platform

Import data directly from supported ecommerce platforms:

```bash
rcommerce import platform <PLATFORM> [OPTIONS]

Arguments:
  <PLATFORM>    Platform type: shopify, woocommerce, magento, medusa

Options:
  -u, --api-url <URL>          API endpoint URL
  -k, --api-key <KEY>          API key or access token
      --api-secret <SECRET>    API secret (if required)
  -e, --entities <ENTITIES>    Comma-separated list: products,customers,orders [default: all]
      --limit <LIMIT>          Maximum records to import per entity
      --dry-run                Validate data without importing
```

**Supported Platforms:**

| Platform | Status | Authentication | Entities |
|----------|--------|----------------|----------|
| Shopify | ✅ Full | API Key + Password | Products, Customers, Orders |
| WooCommerce | ✅ Full | Consumer Key + Secret | Products, Customers, Orders |
| Magento | 🚧 Planned | OAuth/API Token | Products, Customers, Orders |
| Medusa | 🚧 Planned | API Token | Products, Customers, Orders |

**Examples:**

```bash
# Import all data from Shopify
rcommerce import platform shopify \
  -c config.toml \
  --api-url https://your-store.myshopify.com \
  --api-key YOUR_API_KEY \
  --api-secret YOUR_API_PASSWORD

# Import only products and customers (dry run)
rcommerce import platform shopify \
  -c config.toml \
  --api-url https://your-store.myshopify.com \
  --api-key YOUR_API_KEY \
  --api-secret YOUR_API_PASSWORD \
  --entities products,customers \
  --dry-run

# Import from WooCommerce with limit
rcommerce import platform woocommerce \
  -c config.toml \
  --api-url https://your-store.com/wp-json/wc/v3 \
  --api-key YOUR_CONSUMER_KEY \
  --api-secret YOUR_CONSUMER_SECRET \
  --limit 100
```

**Dry Run Mode:**

Use `--dry-run` to validate data without actually importing:

```bash
rcommerce import platform shopify ... --dry-run
```

Output:
```
🧪 DRY RUN MODE - No data will be imported
Fetching products from Shopify (DRY RUN)...
Validating: Premium T-Shirt
Validating: Wireless Headphones
...

Import Summary (DRY RUN)
========================
Entity: products
  Total:     150
  Created:   150
  Updated:   0
  Skipped:   0
  Errors:    0

✅ Validation complete. Run without --dry-run to import.
```

#### Import from File

Import data from CSV, JSON, or XML files:

```bash
rcommerce import file [OPTIONS] --file <PATH> --format <FORMAT> --entity <ENTITY>

Options:
  -f, --file <PATH>        Path to import file
  -F, --format <FORMAT>    File format: csv, json, xml
  -e, --entity <ENTITY>    Entity type: products, customers, orders
  -l, --limit <LIMIT>      Maximum records to import
      --dry-run            Validate data without importing
```

**File Format Support:**

| Format | Status | Description |
|--------|--------|-------------|
| CSV | ✅ Full | Comma-separated values with headers |
| JSON | ✅ Full | JSON array of objects |
| XML | 🚧 Planned | XML document format |

**CSV Format:**

Expected columns for each entity type:

**Products:**
```csv
id,title,slug,description,price,compare_at_price,sku,inventory_quantity,status,product_type
TSHIRT-001,Premium T-Shirt,premium-t-shirt,High quality cotton,29.99,39.99,TSHIRT-001,100,active,physical
```

**Customers:**
```csv
id,email,first_name,last_name,phone,address1,city,state,postal_code,country
cust-001,john@example.com,John,Doe,+1234567890,123 Main St,New York,NY,10001,US
```

**Orders:**
```csv
id,order_number,customer_id,email,status,total,subtotal,tax_total,shipping_total
ORD-001,1001,cust-001,john@example.com,confirmed,59.98,54.99,4.99,0.00
```

**JSON Format:**

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "title": "Premium T-Shirt",
    "slug": "premium-t-shirt",
    "description": "High quality cotton t-shirt",
    "price": "29.99",
    "sku": "TSHIRT-001",
    "inventory_quantity": 100,
    "status": "active"
  }
]
```

**Examples:**

```bash
# Import products from CSV
rcommerce import file \
  -c config.toml \
  --file products.csv \
  --format csv \
  --entity products

# Import customers from JSON (dry run)
rcommerce import file \
  -c config.toml \
  --file customers.json \
  --format json \
  --entity customers \
  --dry-run

# Import with limit
rcommerce import file \
  -c config.toml \
  --file orders.csv \
  --format csv \
  --entity orders \
  --limit 50
```

#### Import Configuration

Import settings can also be configured in `config.toml`:

```toml
[import]
# Default batch size for imports
batch_size = 100

# Continue on error (skip failed records)
continue_on_error = true

# Skip existing records (based on unique identifiers)
skip_existing = true

[import.shopify]
api_version = "2024-01"
# Store-specific settings

[import.woocommerce]
verify_ssl = true
```

### Environment Variables

The CLI respects these environment variables:

| Variable | Description |
|----------|-------------|
| `RCOMMERCE_CONFIG` | Default config file path |
| `RUST_LOG` | Log level (debug, info, warn, error) |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |
| 3 | Database error |
| 4 | Configuration error |

## Security Features

The CLI includes several security features:

### Root User Prevention

The CLI will refuse to run as the root user for security reasons:

```
❌ ERROR: Running as root is not allowed!
   The rcommerce CLI should not be run as root for security reasons.
   Please run as a non-privileged user.
```

### Config File Permissions

The CLI warns if your config file has overly permissive permissions:

```
⚠️  WARNING: Config file is world-readable
   Path: /etc/rcommerce/config.toml
   Consider running: chmod 600 /etc/rcommerce/config.toml
```

## Interactive Features

The CLI uses the `dialoguer` crate to provide interactive prompts for:

- **Input validation**: Real-time validation with helpful error messages
- **Selection menus**: Arrow key navigation for enums and options
- **Confirmation prompts**: Yes/no confirmations with defaults
- **Password input**: Hidden input with confirmation matching
- **Summary preview**: Review all data before final submission

Press `Ctrl+C` at any time during interactive prompts to cancel the operation.

## See Also

- [Configuration Guide](../getting-started/configuration.md)
- [Authentication](../api-reference/authentication.md)
- [Deployment Guide](../deployment/index.md)
