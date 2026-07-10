# Magento to R commerce Migration Guide

## Overview

Magento has a complex database structure due to its EAV (Entity-Attribute-Value) model, multiple store views, and extensive extension ecosystem. This guide covers migrating from Magento 2.x (Open Source or Commerce) to R commerce using API-based approaches.

## Pre-Migration Analysis

### Magento Store Audit

```bash
# Using Magento CLI (bin/magento)
php bin/magento info:adminuri
php bin/magento cache:status
php bin/magento indexer:status

# Get store information
php bin/magento config:show

# List all modules
php bin/magento module:status

# Get product count
php bin/magento info:product-count

# Get customer count (via custom command or API)
```

**Using Magento REST API:**

```bash
# Get access token
ACCESS_TOKEN=$(curl -X POST "https://your-magento-store.com/rest/V1/integration/admin/token" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"password"}' | tr -d '"')

# Get product count
curl -X GET "https://your-magento-store.com/rest/V1/products?searchCriteria[pageSize]=1" \
  -H "Authorization: Bearer $ACCESS_TOKEN" | jq '.search_criteria.total_count'

# Get customer count
curl -X GET "https://your-magento-store.com/rest/V1/customers/search?searchCriteria[pageSize]=1" \
  -H "Authorization: Bearer $ACCESS_TOKEN" | jq '.search_criteria.total_count'

# Get order count
curl -X GET "https://your-magento-store.com/rest/V1/orders?searchCriteria[pageSize]=1" \
  -H "Authorization: Bearer $ACCESS_TOKEN" | jq '.search_criteria.total_count'

# Get store information
curl -X GET "https://your-magento-store.com/rest/V1/store/storeConfigs" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

## Magento Data Structure Understanding

### EAV Model Overview

Magento's EAV model makes data access complex. When using the REST API, this complexity is abstracted:

```
catalog_product_entity (main table)
  ├── entity_id
  ├── sku
  ├── type_id (simple, configurable, bundle, etc.)
  ├── attribute_set_id
  └── [few other fields]

EAV Attribute Tables (accessed via API):
  - catalog_product_entity_varchar (for varchar attributes)
  - catalog_product_entity_int (for integer attributes)
  - catalog_product_entity_decimal (for decimal attributes)
  - catalog_product_entity_text (for text attributes)
  - catalog_product_entity_datetime (for datetime attributes)

eav_attribute (defines all attributes)
  ├── attribute_id
  ├── entity_type_id (4 = catalog_product)
  ├── attribute_code
  ├── backend_type (varchar, int, decimal, text, datetime)
  ├── frontend_label
  └── [many other columns]
```

### Understanding Attribute Sets

Magento organizes attributes into "attribute sets". These can be retrieved via API:

```bash
# Get attribute sets
curl -X GET "https://your-magento-store.com/rest/V1/products/attribute-sets/sets/list?searchCriteria[pageSize]=100" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

## Export Strategy

### Method 1: Magento REST API Export (Recommended)

```bash
#!/bin/bash
# export-magento-api.sh

MAGENTO_URL="https://your-magento-store.com"
ACCESS_TOKEN="YOUR_INTEGRATION_ACCESS_TOKEN"

# Create export directory
mkdir -p magento-export

# Get products using Magento REST API
echo "Exporting products..."
page=1
while true; do
  echo "Fetching products page $page..."
  
  response=$(curl -s -X GET "$MAGENTO_URL/rest/V1/products?searchCriteria[currentPage]=$page&searchCriteria[pageSize]=100" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json")
  
  items_count=$(echo $response | jq '.items | length')
  
  if [ "$items_count" -eq 0 ]; then
    break
  fi
  
  echo $response > magento-export/products-page-$page.json
  page=$((page + 1))
  
  # Magento API rate limiting
  sleep 2
done

# Export categories
echo "Exporting categories..."
curl -X GET "$MAGENTO_URL/rest/V1/categories" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" > magento-export/categories.json

# Export customers
echo "Exporting customers..."
page=1
while true; do
  echo "Fetching customers page $page..."
  
  response=$(curl -s -X GET "$MAGENTO_URL/rest/V1/customers/search?searchCriteria[currentPage]=$page&searchCriteria[pageSize]=100" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json")
  
  items_count=$(echo $response | jq '.items | length')
  
  if [ "$items_count" -eq 0 ]; then
    break
  fi
  
  echo $response > magento-export/customers-page-$page.json
  page=$((page + 1))
  sleep 2
done

# Export orders
echo "Exporting orders..."
page=1
while true; do
  echo "Fetching orders page $page..."
  
  response=$(curl -s -X GET "$MAGENTO_URL/rest/V1/orders?searchCriteria[currentPage]=$page&searchCriteria[pageSize]=100" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json")
  
  items_count=$(echo $response | jq '.items | length')
  
  if [ "$items_count" -eq 0 ]; then
    break
  fi
  
  echo $response > magento-export/orders-page-$page.json
  page=$((page + 1))
  sleep 2
done

echo "Export completed!"
```

### Method 2: Magento Data Migration Tool Approach

The official Magento approach uses migration scripts:

```bash
# Install Magento Data Migration Tool
composer require magento/data-migration-tool:2.4.x

# Configure migration
php bin/magento migrate:settings --reset vendor/magento/data-migration-tool/etc/opensource-to-opensource/1.9.4.5/config.xml
```

### Method 3: CSV Export via Magento Admin

1. Go to **System > Data Transfer > Export**
2. Select Entity Type (Products, Customers, Orders)
3. Choose export format (CSV)
4. Configure field filters if needed
5. Click **Continue**

## Advanced Python Migration Script

```python
#!/usr/bin/env python3
# migrate-magento.py

import requests
import json
import os
import sys
import time
from typing import List, Dict, Optional
from datetime import datetime

class MagentoAPIHelper:
    """Helper to handle Magento REST API interactions"""
    
    def __init__(self, base_url: str, access_token: str):
        self.base_url = base_url.rstrip('/')
        self.access_token = access_token
        self.headers = {
            'Authorization': f'Bearer {access_token}',
            'Content-Type': 'application/json'
        }
    
    def get(self, endpoint: str, params: dict = None) -> dict:
        """Make GET request to Magento API"""
        url = f"{self.base_url}/rest/V1/{endpoint}"
        response = requests.get(url, headers=self.headers, params=params)
        response.raise_for_status()
        return response.json()
    
    def get_all_pages(self, endpoint: str, page_size: int = 100) -> List[dict]:
        """Fetch all pages of a paginated endpoint"""
        all_items = []
        page = 1
        
        while True:
            params = {
                'searchCriteria[currentPage]': page,
                'searchCriteria[pageSize]': page_size
            }
            
            try:
                response = self.get(endpoint, params)
                items = response.get('items', [])
                
                if not items:
                    break
                
                all_items.extend(items)
                print(f"  Fetched page {page}, got {len(items)} items")
                
                # Check if we've got all items
                total_count = response.get('search_criteria', {}).get('total_count', 0)
                if len(all_items) >= total_count:
                    break
                
                page += 1
                time.sleep(1)  # Rate limiting
                
            except Exception as e:
                print(f"Error fetching page {page}: {e}")
                break
        
        return all_items

class MagentoMigrator:
    def __init__(self, magento_config: dict, rcommerce_config: dict):
        self.magento = MagentoAPIHelper(
            magento_config['url'],
            magento_config['access_token']
        )
        self.rcommerce_url = rcommerce_config['url']
        self.rcommerce_key = rcommerce_config['api_key']
        self.migration_log = []
        self.store_mapping = {}
        self.attribute_mapping = {}
    
    def migrate_all(self):
        """Execute complete Magento to R commerce migration"""
        print("Starting Magento to R commerce migration...")
        print(f"Magento: {self.magento.base_url}")
        print(f"R commerce: {self.rcommerce_url}")
        
        try:
            # Pre-migration analysis
            self.analyze_magento_structure()
            
            # Phase 1: Store configuration
            print("\n=== Phase 1: Store Configuration ===")
            self.migrate_store_configuration()
            
            # Phase 2: Categories
            print("\n=== Phase 2: Categories ===")
            self.migrate_categories()
            
            # Phase 3: Attributes (as product metafields)
            print("\n=== Phase 3: Product Attributes ===")
            self.migrate_attributes()
            
            # Phase 4: Simple products first
            print("\n=== Phase 4: Simple Products ===")
            self.migrate_products('simple')
            
            # Phase 5: Configurable products
            print("\n=== Phase 5: Configurable Products ===")
            self.migrate_products('configurable')
            
            # Phase 6: Other product types
            print("\n=== Phase 6: Bundle and Grouped Products ===")
            self.migrate_products('bundle')
            self.migrate_products('grouped')
            
            # Phase 7: Customers
            print("\n=== Phase 7: Customers ===")
            self.migrate_customers()
            
            # Phase 8: Orders (optional)
            if os.environ.get('MIGRATE_ORDERS'):
                print("\n=== Phase 8: Orders ===")
                self.migrate_orders()
            
            # Save log
            self.save_migration_log()
            
            print("\n Migration completed!")
            self.print_summary()
            
        except Exception as e:
            print(f"\n Migration failed: {e}")
            import traceback
            traceback.print_exc()
            sys.exit(1)
    
    def analyze_magento_structure(self):
        """Analyze Magento store structure"""
        print("Analyzing Magento structure...")
        
        try:
            # Get product count by type
            product_types = {}
            products = self.magento.get_all_pages('products', page_size=1)
            total_products = len(products)
            print(f"Total products found: {total_products}")
            
            # Get store information
            stores = self.magento.get('store/storeConfigs')
            print(f"Store configurations: {len(stores)}")
            
            # Get attribute sets
            attr_sets = self.magento.get('products/attribute-sets/sets/list?searchCriteria[pageSize]=100')
            print(f"Attribute sets: {len(attr_sets.get('items', []))}")
            
        except Exception as e:
            print(f"Warning: Could not analyze structure: {e}")
    
    def migrate_store_configuration(self):
        """Migrate store/website configuration as metadata"""
        try:
            stores = self.magento.get('store/storeConfigs')
            
            for store in stores:
                store_config = {
                    'store_id': store.get('id'),
                    'store_code': store.get('code'),
                    'website_id': store.get('website_id'),
                    'website_name': store.get('website_name'),
                    'base_url': store.get('base_url'),
                    'base_currency': store.get('base_currency_code'),
                    'timezone': store.get('timezone')
                }
                
                self.migration_log.append({
                    'type': 'store',
                    'operation': 'map',
                    'status': 'success',
                    'source_id': store.get('id'),
                    'config': store_config
                })
                
                print(f" Mapped store: {store.get('code')}")
                
        except Exception as e:
            print(f"Error migrating store config: {e}")
    
    def migrate_categories(self):
        """Migrate Magento categories"""
        try:
            categories_data = self.magento.get('categories')
            
            def process_category(category, parent_id=None):
                try:
                    category_data = {
                        'name': category['name'],
                        'slug': self.generate_slug(category['name']),
                        'description': '',  # Will be populated from custom attributes if available
                        'meta_data': {
                            'magento': {
                                'category_id': category['id'],
                                'parent_id': parent_id,
                                'path': category.get('path', ''),
                                'level': category.get('level', 0)
                            }
                        }
                    }
                    
                    # Create category in R commerce
                    response = requests.post(
                        f"{self.rcommerce_url}/v1/categories",
                        json=category_data,
                        headers={'Authorization': f'Bearer {self.rcommerce_key}'}
                    )
                    
                    if response.status_code == 201:
                        print(f" Migrated category: {category['name']}")
                        self.migration_log.append({
                            'type': 'category',
                            'operation': 'create',
                            'status': 'success',
                            'source_id': category['id'],
                            'target_id': response.json()['data']['id'],
                            'name': category['name']
                        })
                    else:
                        print(f" Failed to migrate category {category['name']}: {response.text}")
                        self.migration_log.append({
                            'type': 'category',
                            'operation': 'create',
                            'status': 'failed',
                            'source_id': category['id'],
                            'name': category['name'],
                            'error': response.text
                        })
                    
                    time.sleep(0.5)
                    
                    # Process children
                    for child in category.get('children_data', []):
                        process_category(child, category['id'])
                        
                except Exception as e:
                    print(f" Error migrating category {category.get('name')}: {e}")
            
            # Start with root category's children
            for category in categories_data.get('children_data', []):
                process_category(category)
                
        except Exception as e:
            print(f"Error in category migration: {e}")
    
    def migrate_attributes(self):
        """Migrate Magento attributes as product metafields schema"""
        try:
            attributes = self.magento.get_all_pages('products/attributes', page_size=100)
            
            for attr in attributes:
                try:
                    attribute_definition = {
                        'attribute_id': attr['attribute_id'],
                        'attribute_code': attr['attribute_code'],
                        'frontend_label': attr.get('default_frontend_label', ''),
                        'frontend_input': attr.get('frontend_input', ''),
                        'is_required': attr.get('is_required', False),
                        'is_user_defined': attr.get('is_user_defined', False)
                    }
                    
                    self.migration_log.append({
                        'type': 'attribute',
                        'operation': 'map',
                        'status': 'success',
                        'attribute_code': attr['attribute_code'],
                        'definition': attribute_definition
                    })
                    
                    self.attribute_mapping[attr['attribute_code']] = attribute_definition
                    print(f" Mapped attribute: {attr['attribute_code']}")
                    
                except Exception as e:
                    print(f" Error mapping attribute: {e}")
                    
        except Exception as e:
            print(f"Error in attribute migration: {e}")
    
    def migrate_products(self, product_type: str):
        """Migrate products of a specific type"""
        try:
            # Get all products and filter by type
            all_products = self.magento.get_all_pages('products', page_size=100)
            products = [p for p in all_products if p.get('type_id') == product_type]
            
            print(f"Found {len(products)} {product_type} products")
            
            for product in products:
                try:
                    # Get full product details
                    product_detail = self.magento.get(f"products/{product['sku']}")
                    
                    # Transform based on product type
                    if product_type == 'simple':
                        product_data = self.transform_simple_product(product_detail)
                    elif product_type == 'configurable':
                        product_data = self.transform_configurable_product(product_detail)
                    elif product_type == 'bundle':
                        product_data = self.transform_bundle_product(product_detail)
                    elif product_type == 'grouped':
                        product_data = self.transform_grouped_product(product_detail)
                    else:
                        print(f" Skipping unsupported type: {product_type}")
                        continue
                    
                    # Create in R commerce
                    response = requests.post(
                        f"{self.rcommerce_url}/v1/products",
                        json=product_data,
                        headers={'Authorization': f'Bearer {self.rcommerce_key}'}
                    )
                    
                    if response.status_code == 201:
                        print(f" Migrated {product_type} product: {product.get('name', product['sku'])}")
                        self.migration_log.append({
                            'type': 'product',
                            'product_type': product_type,
                            'operation': 'create',
                            'status': 'success',
                            'source_id': product.get('id'),
                            'sku': product['sku'],
                            'target_id': response.json()['data']['id'],
                            'name': product.get('name', product['sku'])
                        })
                    else:
                        print(f" Failed to migrate {product_type} product {product['sku']}: {response.text}")
                        self.migration_log.append({
                            'type': 'product',
                            'product_type': product_type,
                            'operation': 'create',
                            'status': 'failed',
                            'source_id': product.get('id'),
                            'sku': product['sku'],
                            'name': product.get('name', product['sku']),
                            'error': response.text
                        })
                    
                    time.sleep(0.5)
                    
                except Exception as e:
                    print(f" Error migrating {product_type} product {product.get('sku', 'unknown')}: {e}")
                    
        except Exception as e:
            print(f"Error in {product_type} product migration: {e}")
    
    def transform_simple_product(self, product: dict) -> dict:
        """Transform simple product from Magento API response"""
        custom_attrs = {attr['attribute_code']: attr['value'] 
                       for attr in product.get('custom_attributes', [])}
        
        return {
            'name': product.get('name', f"Product {product['sku']}"),
            'slug': custom_attrs.get('url_key') or self.generate_slug(product.get('name', '')),
            'description': custom_attrs.get('description', ''),
            'short_description': custom_attrs.get('short_description', ''),
            'sku': product['sku'],
            'price': float(custom_attrs.get('price', 0) or 0),
            'compare_at_price': float(custom_attrs.get('msrp', 0) or 0) if custom_attrs.get('msrp') else None,
            'cost': float(custom_attrs.get('cost', 0) or 0) if custom_attrs.get('cost') else None,
            'inventory_quantity': int(custom_attrs.get('qty', 0) or 0),
            'inventory_policy': 'deny' if custom_attrs.get('is_in_stock') == '0' else 'continue',
            'weight': float(custom_attrs.get('weight', 0) or 0) if custom_attrs.get('weight') else None,
            'status': 'active' if product.get('status') == 1 else 'draft',
            'is_taxable': custom_attrs.get('tax_class_id') != '0',
            'requires_shipping': bool(custom_attrs.get('weight') and float(custom_attrs.get('weight', 0)) > 0),
            'images': [{'url': media['file'], 'position': media.get('position', 0)} 
                      for media in product.get('media_gallery_entries', [])],
            'meta_data': {
                'magento': {
                    'entity_id': product.get('id'),
                    'type_id': product.get('type_id'),
                    'attribute_set_id': product.get('attribute_set_id'),
                    'visibility': custom_attrs.get('visibility'),
                    'tax_class_id': custom_attrs.get('tax_class_id'),
                    'custom_attributes': custom_attrs
                }
            }
        }
    
    def transform_configurable_product(self, product: dict) -> dict:
        """Transform configurable product and its variants"""
        main_product = self.transform_simple_product(product)
        main_product['options'] = []
        main_product['variants'] = []
        
        # Get configurable product options
        try:
            for option in product.get('extension_attributes', {}).get('configurable_product_options', []):
                option_values = [str(v['value_index']) for v in option.get('values', [])]
                main_product['options'].append({
                    'name': option.get('label', ''),
                    'position': option.get('position', 0),
                    'values': option_values
                })
            
            # Get child products (variants)
            child_skus = product.get('extension_attributes', {}).get('configurable_product_links', [])
            for child_sku in child_skus:
                try:
                    child_product = self.magento.get(f"products/{child_sku}")
                    variant = self.transform_simple_product(child_product)
                    variant['options'] = {}
                    
                    # Extract variant option values from custom attributes
                    for attr in child_product.get('custom_attributes', []):
                        if attr['attribute_code'] in [opt['name'] for opt in main_product['options']]:
                            variant['options'][attr['attribute_code']] = attr['value']
                    
                    main_product['variants'].append(variant)
                    time.sleep(0.2)
                except Exception as e:
                    print(f"  Error fetching variant {child_sku}: {e}")
                    
        except Exception as e:
            print(f" Error processing configurable options: {e}")
        
        return main_product
    
    def transform_bundle_product(self, product: dict) -> dict:
        """Transform bundle product"""
        main_product = self.transform_simple_product(product)
        
        # Bundle options are stored in meta_data
        bundle_options = product.get('extension_attributes', {}).get('bundle_product_options', [])
        main_product['meta_data']['magento']['bundle_options'] = bundle_options
        main_product['meta_data']['product_type_note'] = 'Bundle product - options stored in meta_data'
        
        return main_product
    
    def transform_grouped_product(self, product: dict) -> dict:
        """Transform grouped product"""
        main_product = self.transform_simple_product(product)
        
        # Grouped product links
        grouped_links = product.get('extension_attributes', {}).get('grouped_product_links', [])
        main_product['meta_data']['magento']['grouped_children'] = grouped_links
        main_product['meta_data']['product_type_note'] = 'Grouped product - child products stored in meta_data'
        
        return main_product
    
    def migrate_customers(self):
        """Migrate Magento customers"""
        try:
            customers = self.magento.get_all_pages('customers/search', page_size=100)
            
            for customer in customers:
                try:
                    customer_data = self.transform_customer(customer)
                    
                    response = requests.post(
                        f"{self.rcommerce_url}/v1/customers",
                        json=customer_data,
                        headers={'Authorization': f'Bearer {self.rcommerce_key}'}
                    )
                    
                    if response.status_code == 201:
                        print(f" Migrated customer: {customer['email']}")
                        self.migration_log.append({
                            'type': 'customer',
                            'operation': 'create',
                            'status': 'success',
                            'source_id': customer['id'],
                            'target_id': response.json()['data']['id'],
                            'email': customer['email']
                        })
                    else:
                        print(f" Failed to migrate customer {customer['email']}: {response.text}")
                        self.migration_log.append({
                            'type': 'customer',
                            'operation': 'create',
                            'status': 'failed',
                            'source_id': customer['id'],
                            'email': customer['email'],
                            'error': response.text
                        })
                    
                    time.sleep(0.5)
                    
                except Exception as e:
                    print(f" Error migrating customer {customer.get('email', 'unknown')}: {e}")
                    
        except Exception as e:
            print(f"Error in customer migration: {e}")
    
    def transform_customer(self, customer: dict) -> dict:
        """Transform Magento customer"""
        addresses = customer.get('addresses', [])
        default_billing = next((a for a in addresses if a.get('default_billing')), addresses[0] if addresses else {})
        default_shipping = next((a for a in addresses if a.get('default_shipping')), addresses[0] if addresses else {})
        
        return {
            'email': customer['email'],
            'first_name': customer.get('firstname', ''),
            'last_name': customer.get('lastname', ''),
            'phone': default_billing.get('telephone', ''),
            'accepts_marketing': customer.get('extension_attributes', {}).get('is_subscribed', False),
            'billing_address': {
                'first_name': default_billing.get('firstname', ''),
                'last_name': default_billing.get('lastname', ''),
                'company': default_billing.get('company', ''),
                'address1': ' '.join(default_billing.get('street', [])[:1]),
                'address2': ' '.join(default_billing.get('street', [])[1:]),
                'city': default_billing.get('city', ''),
                'state': default_billing.get('region', {}).get('region', ''),
                'postal_code': default_billing.get('postcode', ''),
                'country': default_billing.get('country_id', ''),
                'phone': default_billing.get('telephone', '')
            },
            'shipping_address': {
                'first_name': default_shipping.get('firstname', ''),
                'last_name': default_shipping.get('lastname', ''),
                'company': default_shipping.get('company', ''),
                'address1': ' '.join(default_shipping.get('street', [])[:1]),
                'address2': ' '.join(default_shipping.get('street', [])[1:]),
                'city': default_shipping.get('city', ''),
                'state': default_shipping.get('region', {}).get('region', ''),
                'postal_code': default_shipping.get('postcode', ''),
                'country': default_shipping.get('country_id', ''),
                'phone': default_shipping.get('telephone', '')
            },
            'meta_data': {
                'magento': {
                    'customer_id': customer['id'],
                    'group_id': customer.get('group_id'),
                    'store_id': customer.get('store_id'),
                    'website_id': customer.get('website_id')
                }
            }
        }
    
    def migrate_orders(self):
        """Migrate Magento orders"""
        try:
            orders = self.magento.get_all_pages('orders', page_size=50)  # Smaller batch for orders
            
            for order in orders:
                try:
                    order_data = self.transform_order(order)
                    
                    response = requests.post(
                        f"{self.rcommerce_url}/v1/orders",
                        json=order_data,
                        headers={'Authorization': f'Bearer {self.rcommerce_key}'}
                    )
                    
                    if response.status_code == 201:
                        print(f" Migrated order: {order.get('increment_id', order['entity_id'])}")
                        self.migration_log.append({
                            'type': 'order',
                            'operation': 'create',
                            'status': 'success',
                            'source_id': order['entity_id'],
                            'target_id': response.json()['data']['id'],
                            'order_number': order.get('increment_id')
                        })
                    else:
                        print(f" Failed to migrate order {order.get('increment_id')}: {response.text}")
                    
                    time.sleep(0.5)
                    
                except Exception as e:
                    print(f" Error migrating order {order.get('increment_id', 'unknown')}: {e}")
                    
        except Exception as e:
            print(f"Error in order migration: {e}")
    
    def transform_order(self, order: dict) -> dict:
        """Transform Magento order"""
        return {
            'order_number': order.get('increment_id', str(order['entity_id'])),
            'customer_email': order.get('customer_email', ''),
            'customer_first_name': order.get('customer_firstname', ''),
            'customer_last_name': order.get('customer_lastname', ''),
            'subtotal': float(order.get('subtotal', 0) or 0),
            'tax_amount': float(order.get('tax_amount', 0) or 0),
            'shipping_amount': float(order.get('shipping_amount', 0) or 0),
            'discount_amount': float(order.get('discount_amount', 0) or 0),
            'total': float(order.get('grand_total', 0) or 0),
            'status': self.map_order_status(order.get('status', 'pending')),
            'billing_address': self.transform_order_address(order.get('billing_address', {})),
            'shipping_address': self.transform_order_address(order.get('extension_attributes', {}).get('shipping_assignments', [{}])[0].get('shipping', {}).get('address', {})),
            'line_items': [self.transform_order_item(item) for item in order.get('items', [])],
            'meta_data': {
                'magento': {
                    'order_id': order['entity_id'],
                    'store_id': order.get('store_id'),
                    'state': order.get('state'),
                    'shipping_method': order.get('shipping_method'),
                    'shipping_description': order.get('shipping_description'),
                    'coupon_code': order.get('coupon_code')
                }
            }
        }
    
    def transform_order_address(self, address: dict) -> dict:
        """Transform order address"""
        return {
            'first_name': address.get('firstname', ''),
            'last_name': address.get('lastname', ''),
            'company': address.get('company', ''),
            'address1': ' '.join(address.get('street', [])[:1]),
            'address2': ' '.join(address.get('street', [])[1:]),
            'city': address.get('city', ''),
            'state': address.get('region', ''),
            'postal_code': address.get('postcode', ''),
            'country': address.get('country_id', ''),
            'phone': address.get('telephone', '')
        }
    
    def transform_order_item(self, item: dict) -> dict:
        """Transform order item"""
        return {
            'product_id': f"magento_{item.get('product_id')}",
            'name': item.get('name', ''),
            'sku': item.get('sku', ''),
            'quantity': float(item.get('qty_ordered', 0) or 0),
            'unit_price': float(item.get('price', 0) or 0),
            'tax_amount': float(item.get('tax_amount', 0) or 0),
            'discount_amount': float(item.get('discount_amount', 0) or 0),
            'total': float(item.get('row_total', 0) or 0),
            'meta_data': {
                'magento': {
                    'item_id': item.get('item_id'),
                    'product_type': item.get('product_type'),
                    'original_price': item.get('original_price')
                }
            }
        }
    
    def map_order_status(self, magento_status: str) -> str:
        """Map Magento order status to R commerce status"""
        status_map = {
            'pending': 'pending',
            'processing': 'processing',
            'complete': 'completed',
            'closed': 'completed',
            'canceled': 'cancelled',
            'holded': 'on_hold',
            'payment_review': 'on_hold',
            'fraud': 'fraud_review'
        }
        return status_map.get(magento_status, 'pending')
    
    def generate_slug(self, name: str) -> str:
        """Generate URL-friendly slug"""
        import re
        return re.sub(r'[^a-z0-9]+', '-', name.lower()).strip('-')
    
    def save_migration_log(self):
        """Save complete migration log"""
        timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
        filename = f'magento_migration_log_{timestamp}.json'
        
        summary = {
            'products': {'success': 0, 'failed': 0},
            'categories': {'success': 0, 'failed': 0},
            'customers': {'success': 0, 'failed': 0},
            'orders': {'success': 0, 'failed': 0}
        }
        
        for log_entry in self.migration_log:
            item_type = log_entry['type']
            status = log_entry['status']
            
            if item_type in summary:
                if status == 'success':
                    summary[item_type]['success'] += 1
                else:
                    summary[item_type]['failed'] += 1
        
        with open(filename, 'w') as f:
            json.dump({
                'timestamp': timestamp,
                'summary': summary,
                'details': self.migration_log
            }, f, indent=2)
        
        print(f"Migration log saved to {filename}")
        print(f"Summary: {json.dumps(summary, indent=2)}")
    
    def print_summary(self):
        """Print migration summary"""
        summary = {
            'products': {'success': 0, 'failed': 0},
            'categories': {'success': 0, 'failed': 0},
            'customers': {'success': 0, 'failed': 0},
            'orders': {'success': 0, 'failed': 0}
        }
        
        for log_entry in self.migration_log:
            item_type = log_entry['type']
            status = log_entry['status']
            
            if item_type in summary:
                if status == 'success':
                    summary[item_type]['success'] += 1
                else:
                    summary[item_type]['failed'] += 1
        
        print("\n" + "="*50)
        print("MIGRATION SUMMARY")
        print("="*50)
        for item_type, counts in summary.items():
            total = counts['success'] + counts['failed']
            print(f"{item_type.capitalize()}: {counts['success']}/{total} succeeded")
        print("="*50)

# Usage
if __name__ == '__main__':
    magento_config = {
        'url': os.environ.get('MAGENTO_URL', 'https://your-magento-store.com'),
        'access_token': os.environ.get('MAGENTO_ACCESS_TOKEN', 'your_access_token')
    }
    
    rcommerce_config = {
        'url': os.environ.get('RCOMMERCE_URL', 'https://api.yourstore.com'),
        'api_key': os.environ.get('RCOMMERCE_API_KEY', 'your_api_key')
    }
    
    migrator = MagentoMigrator(magento_config, rcommerce_config)
    migrator.migrate_all()
```

## Handling Enterprise Features

If migrating from Magento Commerce (Enterprise):

### Customer Segments

```bash
# Export customer segments via API
curl -X GET "https://your-magento-store.com/rest/V1/customerSegments" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

### CMS Blocks and Pages

```bash
# Export CMS blocks
curl -X GET "https://your-magento-store.com/rest/V1/cmsBlock/search?searchCriteria[pageSize]=100" \
  -H "Authorization: Bearer $ACCESS_TOKEN"

# Export CMS pages
curl -X GET "https://your-magento-store.com/rest/V1/cmsPage/search?searchCriteria[pageSize]=100" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

### Advanced Inventory (MSI)

```bash
# Get inventory sources
curl -X GET "https://your-magento-store.com/rest/V1/inventory/sources" \
  -H "Authorization: Bearer $ACCESS_TOKEN"

# Get stock items with source
curl -X GET "https://your-magento-store.com/rest/V1/inventory/source-items?searchCriteria[pageSize]=100" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

## Post-Migration Checklist

### Critical Verification

```bash
# 1. Verify product counts
mag_product_count=$(curl -s -H "Authorization: Bearer $MAGENTO_TOKEN" \
  "$MAGENTO_URL/rest/V1/products?searchCriteria[pageSize]=1" | jq '.search_criteria.total_count')
rc_product_count=$(curl -s -H "Authorization: Bearer $RC_KEY" \
  "$RC_URL/v1/products?per_page=1" | jq '.meta.pagination.total')

# 2. Verify categories
mag_category_count=$(curl -s -H "Authorization: Bearer $MAGENTO_TOKEN" \
  "$MAGENTO_URL/rest/V1/categories" | jq '[.. | objects | select(has("children_data")) | .children_data[]] | length')
rc_category_count=$(curl -s -H "Authorization: Bearer $RC_KEY" \
  "$RC_URL/v1/categories?per_page=1" | jq '.meta.pagination.total')

# 3. Verify customers
mag_customer_count=$(curl -s -H "Authorization: Bearer $MAGENTO_TOKEN" \
  "$MAGENTO_URL/rest/V1/customers/search?searchCriteria[pageSize]=1" | jq '.search_criteria.total_count')
rc_customer_count=$(curl -s -H "Authorization: Bearer $RC_KEY" \
  "$RC_URL/v1/customers?per_page=1" | jq '.meta.pagination.total')

echo "Product Count - Magento: $mag_product_count, R commerce: $rc_product_count"
echo "Category Count - Magento: $mag_category_count, R commerce: $rc_category_count"
echo "Customer Count - Magento: $mag_customer_count, R commerce: $rc_customer_count"
```

### R commerce Feature Testing

- [ ] Product search and filtering works
- [ ] Configurable product selection works
- [ ] Categories display correctly
- [ ] Customer login functional
- [ ] Cart and checkout process works
- [ ] Payment gateways configured
- [ ] Shipping methods set up
- [ ] Tax rules applied correctly
- [ ] Email notifications send
- [ ] Order management functional

This migration guide addresses the complexity of Magento's architecture using API-based approaches, including EAV handling, multi-store configurations, and enterprise features that require special attention during migration to R commerce.
