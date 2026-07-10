# Migration System Fix

## Problem Summary

The migration system had several critical issues:

1. **31 migration files** with duplicate numbers (006, 007, 008, 009, 010, 011, 012 all had duplicates)
2. **migrate.rs referenced wrong files** - e.g., migration 5 used file `006_customer_fields.sql`
3. **Partial migrations applied** - database had some tables/columns missing
4. **Migration 16 failing** - fulfillments table existed but was missing columns

## Solution Implemented

### Step 1: Created Comprehensive Fix-All Migration

Created `025_fix_all_schema_issues.sql` - a single idempotent migration that:
- Creates all enum types (if not exists)
- Creates all 26 core tables with complete column definitions
- Adds all missing columns to existing tables
- Creates all indexes
- Sets up all triggers for `updated_at` columns
- Can be safely run multiple times

### Step 2: Simplified migrate.rs

Updated `migrate.rs` to only include:
- Migrations 1-11 (original core migrations)
- Migration 25 (the comprehensive fix-all migration)

### Step 3: Cleaned Up Duplicate Files

Moved problematic/duplicate migration files to `archived/` folder:
- `006_subscriptions.sql` (duplicate)
- `007_subscriptions.sql` (duplicate)
- `008_fix_currency_type.sql` (duplicate)
- `009_add_order_fields.sql` (duplicate)
- `010_add_address_ids.sql` (duplicate)
- `011_add_all_order_columns.sql` (duplicate)
- `012_add_order_item_columns.sql` (duplicate)
- `012_statistics_views.sql` (duplicate)
- `013_statistics_views.sql` (duplicate)
- `014_fix_address_columns.sql` (consolidated into 025)
- `015_add_customer_role.sql` (consolidated into 025)
- `016_create_product_categories.sql` (consolidated into 025)
- `017_create_fulfillments.sql` (consolidated into 025)
- `018_create_order_notes.sql` (consolidated into 025)
- `019_create_subscription_items.sql` (consolidated into 025)
- `020_create_collections.sql` (consolidated into 025)
- `021_fix_coupon_applications_fk.sql` (consolidated into 025)
- `022_digital_products.sql` (consolidated into 025)
- `023_order_downloads.sql` (consolidated into 025)
- `024_bundle_components.sql` (consolidated into 025)

## Migration Files Remaining

```
migrations/
├── 001_initial_schema.sql          # Core tables (products, customers, orders, etc.)
├── 002_carts_and_coupons.sql       # Cart and coupon tables
├── 003_demo_products.sql           # Demo data
├── 004_fix_product_schema.sql      # Product schema fixes
├── 005_api_keys.sql                # API keys table
├── 006_customer_fields.sql         # Customer field additions
├── 007_fix_currency_type.sql       # Currency type fixes
├── 008_add_order_fields.sql        # Order field additions
├── 009_add_address_ids.sql         # Address ID additions
├── 010_add_all_order_columns.sql   # Complete order columns
├── 011_add_order_item_columns.sql  # Order item columns
├── 025_fix_all_schema_issues.sql   # Comprehensive fix-all migration
└── archived/                       # Old/problematic migrations
```

## Tables Covered by Migration 025

1. **products** - with digital product and bundle columns
2. **product_variants**
3. **product_images**
4. **product_options**
5. **product_option_values**
6. **customers** - with role column
7. **addresses** - with is_default_shipping/billing columns
8. **orders** - with coupon columns
9. **order_items** - with digital/bundle columns
10. **payments**
11. **refunds**
12. **carts**
13. **cart_items**
14. **coupons**
15. **coupon_applications**
16. **coupon_usages**
17. **api_keys**
18. **subscriptions**
19. **subscription_items**
20. **fulfillments**
21. **order_notes**
22. **product_categories**
23. **collections**
24. **collection_products**
25. **bundle_components**
26. **order_item_downloads**
27. **license_keys**

## Enum Types Created

- `currency`
- `weight_unit`
- `length_unit`
- `inventory_policy`
- `product_type`
- `subscription_interval`
- `order_status`
- `fulfillment_status`
- `payment_status`
- `payment_method_type`
- `discount_type`
- `customer_role`
- `subscription_status`

## How It Works

1. On fresh database: Migrations 1-11 run first, then migration 25 ensures everything is complete
2. On existing database: Migration 25 fills in any missing tables/columns idempotently
3. The migration uses `IF NOT EXISTS` for all CREATE operations
4. Uses `ADD COLUMN IF NOT EXISTS` for all ALTER operations
5. Uses PL/pgSQL DO blocks for conditional logic

## Benefits

- **Idempotent**: Can run multiple times safely
- **Self-healing**: Fixes any missing schema elements
- **Maintainable**: Single source of truth for schema
- **Clean**: No more duplicate file confusion
