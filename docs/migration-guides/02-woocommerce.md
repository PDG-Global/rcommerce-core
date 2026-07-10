# WooCommerce to R commerce Migration Guide

## Overview

Migrating from WooCommerce to R commerce requires handling WordPress integration, plugin data, and WooCommerce-specific features. This guide covers API-based migration approaches.

## Pre-Migration Analysis

### Audit WooCommerce Store

**Using WP-CLI:**
```bash
# Install WP-CLI if not available
wp --version

# Get product count
wp wc product list --user=1 --format=count

# Get customer count
wp wc customer list --user=1 --format=count

# Get order count
wp wc order list --user=1 --format=count --status=any

# Export all products
wp wc product list --user=1 --format=json > products.json

# List all plugins
wp plugin list --status=active --format=json
```

**Using WooCommerce REST API:**
```bash
# Get store information
curl -u "consumer_key:consumer_secret" \
  "https://your-store.com/wp-json/wc/v3/system_status"

# Get product count (check X-WP-Total header)
curl -I -u "consumer_key:consumer_secret" \
  "https://your-store.com/wp-json/wc/v3/products?per_page=1"

# Get customer count
curl -I -u "consumer_key:consumer_secret" \
  "https://your-store.com/wp-json/wc/v3/customers?per_page=1"

# Get order count
curl -I -u "consumer_key:consumer_secret" \
  "https://your-store.com/wp-json/wc/v3/orders?per_page=1"
```

## Export Strategies

### Option 1: Using WooCommerce REST API (Recommended)

```bash
#!/bin/bash
# export-woocommerce-api.sh

# Authentication
CONSUMER_KEY="your_consumer_key"
CONSUMER_SECRET="your_consumer_secret"
SHOP_URL="https://your-store.com"

# Create export directory
mkdir -p woocommerce-export

# Get products
# Note: WooCommerce API has rate limits (depends on hosting)
curl -u "${CONSUMER_KEY}:${CONSUMER_SECRET}" \
  "${SHOP_URL}/wp-json/wc/v3/products?per_page=100" \
  > woocommerce-export/products.json

# Get products with pagination
page=1
while true; do
  echo "Fetching products page $page..."
  response=$(curl -s -u "${CONSUMER_KEY}:${CONSUMER_SECRET}" \
    "${SHOP_URL}/wp-json/wc/v3/products?per_page=100&page=${page}")
  
  if [ "$(echo "$response" | jq 'length')" -eq 0 ]; then
    break
  fi
  
  echo "$response" >> woocommerce-export/products-page-${page}.json
  page=$((page + 1))
  
  # WooCommerce API rate limiting
  sleep 1
done

# Export customers
echo "Exporting customers..."
page=1
while true; do
  echo "Fetching customers page $page..."
  response=$(curl -s -u "${CONSUMER_KEY}:${CONSUMER_SECRET}" \
    "${SHOP_URL}/wp-json/wc/v3/customers?per_page=100&page=${page}")
  
  if [ "$(echo "$response" | jq 'length')" -eq 0 ]; then
    break
  fi
  
  echo "$response" >> woocommerce-export/customers-page-${page}.json
  page=$((page + 1))
  sleep 1
done

# Export orders (optional)
echo "Exporting orders..."
page=1
while true; do
  echo "Fetching orders page $page..."
  response=$(curl -s -u "${CONSUMER_KEY}:${CONSUMER_SECRET}" \
    "${SHOP_URL}/wp-json/wc/v3/orders?per_page=100&page=${page}")
  
  if [ "$(echo "$response" | jq 'length')" -eq 0 ]; then
    break
  fi
  
  echo "$response" >> woocommerce-export/orders-page-${page}.json
  page=$((page + 1))
  sleep 1
done

# Export categories
echo "Exporting categories..."
curl -u "${CONSUMER_KEY}:${CONSUMER_SECRET}" \
  "${SHOP_URL}/wp-json/wc/v3/products/categories?per_page=100" \
  > woocommerce-export/categories.json

echo "Export completed!"
```

### Option 2: Using WP-CLI Export

```bash
#!/bin/bash
# export-woocommerce-wpcli.sh

# Export products to JSON
wp wc product list --user=1 --format=json > woocommerce-export/products.json

# Export customers
wp wc customer list --user=1 --format=json > woocommerce-export/customers.json

# Export orders
wp wc order list --user=1 --format=json --status=any > woocommerce-export/orders.json

# Export product categories
wp wc product_cat list --user=1 --format=json > woocommerce-export/categories.json

# Export product tags
wp wc product_tag list --user=1 --format=json > woocommerce-export/tags.json
```

### Option 3: CSV Export via WooCommerce Admin

1. Go to **WooCommerce > Products**
2. Click **Export** button
3. Select columns to export
4. Choose "Export to CSV"
5. Repeat for Orders (WooCommerce > Orders > Export)

## Data Mapping

### WordPress User to R commerce Customer

WooCommerce stores customers as WordPress users with additional metadata:

```php
<?php
// migrate-customers.php

class WooCommerceCustomerMigrator {
  public function transformCustomer($wpUser) {
    // Core WordPress data
    $customer = [
      'email' => $wpUser->user_email,
      'first_name' => get_user_meta($wpUser->ID, 'first_name', true),
      'last_name' => get_user_meta($wpUser->ID, 'last_name', true),
      'created_at' => $wpUser->user_registered,
      'meta_data' => [
        'wordpress_id' => $wpUser->ID,
        'username' => $wpUser->user_login,
        'nickname' => get_user_meta($wpUser->ID, 'nickname', true),
        'description' => get_user_meta($wpUser->ID, 'description', true),
        'rich_editing' => get_user_meta($wpUser->ID, 'rich_editing', true),
        'syntax_highlighting' => get_user_meta($wpUser->ID, 'syntax_highlighting', true),
        'comment_shortcuts' => get_user_meta($wpUser->ID, 'comment_shortcuts', true),
        'admin_color' => get_user_meta($wpUser->ID, 'admin_color', true),
        'use_ssl' => get_user_meta($wpUser->ID, 'use_ssl', true),
        'show_admin_bar_front' => get_user_meta($wpUser->ID, 'show_admin_bar_front', true),
        'locale' => get_user_meta($wpUser->ID, 'locale', true),
        'wp_capabilities' => get_user_meta($wpUser->ID, 'wp_capabilities', true),
        'wp_user_level' => get_user_meta($wpUser->ID, 'wp_user_level', true),
        'dismissed_wp_pointers' => get_user_meta($wpUser->ID, 'dismissed_wp_pointers', true),
        'session_tokens' => get_user_meta($wpUser->ID, 'session_tokens', true),
        'last_update' => get_user_meta($wpUser->ID, 'last_update', true)
      ]
    ];
    
    // WooCommerce specific data
    $customer['billing_address'] = $this->getBillingAddress($wpUser->ID);
    $customer['shipping_address'] = $this->getShippingAddress($wpUser->ID);
    $customer['accepts_marketing'] = $this->getMarketingConsent($wpUser->ID);
    
    // Additional WooCommerce data
    $customer['meta_data']['woocommerce'] = [
      'paying_customer' => get_user_meta($wpUser->ID, 'paying_customer', true),
      'billing_email' => get_user_meta($wpUser->ID, 'billing_email', true),
      'shipping_email' => get_user_meta($wpUser->ID, 'shipping_email', true),
      'last_update' => get_user_meta($wpUser->ID, 'last_update', true)
    ];
    
    return $customer;
  }
  
  private function getBillingAddress($userId) {
    return [
      'first_name' => get_user_meta($userId, 'billing_first_name', true),
      'last_name' => get_user_meta($userId, 'billing_last_name', true),
      'company' => get_user_meta($userId, 'billing_company', true),
      'address1' => get_user_meta($userId, 'billing_address_1', true),
      'address2' => get_user_meta($userId, 'billing_address_2', true),
      'city' => get_user_meta($userId, 'billing_city', true),
      'state' => get_user_meta($userId, 'billing_state', true),
      'postal_code' => get_user_meta($userId, 'billing_postcode', true),
      'country' => get_user_meta($userId, 'billing_country', true),
      'phone' => get_user_meta($userId, 'billing_phone', true)
    ];
  }
  
  private function getShippingAddress($userId) {
    return [
      'first_name' => get_user_meta($userId, 'shipping_first_name', true),
      'last_name' => get_user_meta($userId, 'shipping_last_name', true),
      'company' => get_user_meta($userId, 'shipping_company', true),
      'address1' => get_user_meta($userId, 'shipping_address_1', true),
      'address2' => get_user_meta($userId, 'shipping_address_2', true),
      'city' => get_user_meta($userId, 'shipping_city', true),
      'state' => get_user_meta($userId, 'shipping_state', true),
      'postal_code' => get_user_meta($userId, 'shipping_postcode', true),
      'country' => get_user_meta($userId, 'shipping_country', true)
    ];
  }
  
  private function getMarketingConsent($userId) {
    // WooCommerce stores marketing consent in various ways
    // Check for common plugins or built-in methods
    
    $marketing = get_user_meta($userId, 'marketing_opt_in', true);
    if ($marketing !== '') {
      return $marketing === 'yes';
    }
    
    // Check if user has purchased (indicates some consent)
    $orders = wc_get_orders([
      'customer_id' => $userId,
      'limit' => 1
    ]);
    
    return !empty($orders);
  }
}
```

### Product Variation Handling

WooCommerce uses product variations differently than Shopify:

```php
<?php
// migrate-variable-products.php

class WooCommerceVariableProductMigrator {
  public function transformProduct($productId) {
    $product = wc_get_product($productId);
    
    if ($product->get_type() === 'variable') {
      return $this->transformVariableProduct($product);
    } else {
      return $this->transformSimpleProduct($product);
    }
  }
  
  private function transformVariableProduct($product) {
    $baseProduct = [
      'name' => $product->get_name(),
      'slug' => $product->get_slug(),
      'description' => $product->get_description(),
      'short_description' => $product->get_short_description(),
      'status' => $product->get_status(),
      'category_id' => $this->getPrimaryCategory($product),
      'tags' => $this->getProductTags($product),
      'meta_data' => $this->getProductMeta($product),
      'images' => $this->getProductImages($product)
    ];
    
    // Handle attributes (becomes options in R commerce)
    $attributes = $product->get_attributes();
    $baseProduct['options'] = $this->transformAttributes($attributes);
    
    // Transform variations
    $variations = $product->get_children();
    $baseProduct['variants'] = [];
    
    foreach ($variations as $variationId) {
      $variation = wc_get_product($variationId);
      $baseProduct['variants'][] = $this->transformVariation($variation, $attributes);
    }
    
    return $baseProduct;
  }
  
  private function transformVariation($variation, $parentAttributes) {
    $variant = [
      'sku' => $variation->get_sku(),
      'price' => $variation->get_price(),
      'regular_price' => $variation->get_regular_price(),
      'sale_price' => $variation->get_sale_price(),
      'inventory_quantity' => $variation->get_stock_quantity(),
      'inventory_policy' => $variation->get_stock_status(),
      'weight' => $variation->get_weight(),
      'dimensions' => [
        'length' => $variation->get_length(),
        'width' => $variation->get_width(),
        'height' => $variation->get_height()
      ],
      'meta_data' => [
        'woocommerce' => [
          'variation_id' => $variation->get_id(),
          'virtual' => $variation->is_virtual(),
          'downloadable' => $variation->is_downloadable()
        ]
      ]
    ];
    
    // Map variation attributes to R commerce options
    foreach ($variation->get_variation_attributes() as $attr => $value) {
      // Transform from 'attribute_pa_color' to 'Color'
      $attrName = str_replace('attribute_', '', $attr);
      $variant['options'] = $variant['options'] || [];
      $variant['options'][$attrName] = $value;
    }
    
    return $variant;
  }
  
  private function transformAttributes($attributes) {
    $options = [];
    
    foreach ($attributes as $attribute) {
      $options[] = [
        'name' => $attribute->get_name(),
        'position' => $attribute->get_position(),
        'values' => $attribute->get_options()
      ];
    }
    
    return $options;
  }
  
  // ... additional helper methods
}
```

## Migration Script (Python)

```python
#!/usr/bin/env python3
# migrate-woocommerce.py

import requests
import json
import os
import sys
import time
from datetime import datetime
from typing import List, Dict

class WooCommerceMigrator:
    def __init__(self, wc_config, rcommerce_config):
        self.wc_url = wc_config['url']
        self.wc_key = wc_config['consumer_key']
        self.wc_secret = wc_config['consumer_secret']
        self.rcommerce_url = rcommerce_config['url']
        self.rcommerce_key = rcommerce_config['api_key']
        self.migration_log = []
        
    def migrate_all(self):
        """Execute complete migration"""
        print("Starting WooCommerce to R commerce migration...")
        
        try:
            # Phase 1: Categories
            print("\n=== Phase 1: Migrating Categories ===")
            self.migrate_categories()
            
            # Phase 2: Products
            print("\n=== Phase 2: Migrating Products ===")
            self.migrate_products()
            
            # Phase 3: Customers
            print("\n=== Phase 3: Migrating Customers ===")
            self.migrate_customers()
            
            # Phase 4: Orders (optional)
            if os.environ.get('MIGRATE_ORDERS'):
                print("\n=== Phase 4: Migrating Orders ===")
                self.migrate_orders()
            
            print("\n Migration completed successfully!")
            self.save_migration_log()
            
        except Exception as e:
            print(f"\n Migration failed: {e}")
            sys.exit(1)
    
    def _wc_get(self, endpoint: str, params: dict = None) -> dict:
        """Make authenticated GET request to WooCommerce API"""
        url = f"{self.wc_url}/wp-json/wc/v3/{endpoint}"
        response = requests.get(url, auth=(self.wc_key, self.wc_secret), params=params)
        response.raise_for_status()
        return response.json()
    
    def migrate_categories(self):
        """Migrate WooCommerce product categories to R commerce"""
        page = 1
        while True:
            categories = self._wc_get('products/categories', {'per_page': 100, 'page': page})
            if not categories:
                break
            
            for category in categories:
                try:
                    category_data = {
                        'name': category['name'],
                        'slug': category['slug'],
                        'description': category.get('description', ''),
                        'meta_data': {
                            'woocommerce': {
                                'category_id': category['id'],
                                'parent': category.get('parent', 0),
                                'display': category.get('display', 'default'),
                                'image': category.get('image', {})
                            }
                        }
                    }
                    
                    # Create in R commerce
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
                    
                    # Rate limiting
                    time.sleep(0.5)
                    
                except Exception as e:
                    print(f" Error migrating category {category['name']}: {e}")
                    self.migration_log.append({
                        'type': 'category',
                        'operation': 'create',
                        'status': 'error',
                        'source_id': category['id'],
                        'name': category['name'],
                        'error': str(e)
                    })
            
            page += 1
    
    def migrate_products(self):
        """Migrate WooCommerce products to R commerce"""
        page = 1
        while True:
            products = self._wc_get('products', {'per_page': 100, 'page': page})
            if not products:
                break
            
            for product in products:
                try:
                    product_data = self.transform_product(product)
                    
                    # Create in R commerce
                    response = requests.post(
                        f"{self.rcommerce_url}/v1/products",
                        json=product_data,
                        headers={'Authorization': f'Bearer {self.rcommerce_key}'}
                    )
                    
                    if response.status_code == 201:
                        print(f" Migrated product: {product['name']}")
                        self.migration_log.append({
                            'type': 'product',
                            'operation': 'create',
                            'status': 'success',
                            'source_id': product['id'],
                            'target_id': response.json()['data']['id'],
                            'name': product['name']
                        })
                    else:
                        print(f" Failed to migrate product {product['name']}: {response.text}")
                        self.migration_log.append({
                            'type': 'product',
                            'operation': 'create',
                            'status': 'failed',
                            'source_id': product['id'],
                            'name': product['name'],
                            'error': response.text
                        })
                    
                    # Rate limiting
                    time.sleep(0.5)
                    
                except Exception as e:
                    print(f" Error migrating product {product.get('name', 'unknown')}: {e}")
                    self.migration_log.append({
                        'type': 'product',
                        'operation': 'create',
                        'status': 'error',
                        'source_id': product.get('id'),
                        'name': product.get('name', 'unknown'),
                        'error': str(e)
                    })
            
            page += 1
    
    def transform_product(self, product: dict) -> dict:
        """Transform WooCommerce product to R commerce format"""
        product_data = {
            'name': product['name'],
            'slug': product['slug'],
            'description': product.get('description', ''),
            'short_description': product.get('short_description', ''),
            'sku': product.get('sku', ''),
            'price': float(product.get('price', 0)),
            'compare_at_price': float(product.get('regular_price', 0)) if product.get('sale_price') else None,
            'inventory_quantity': product.get('stock_quantity', 0) or 0,
            'inventory_policy': 'deny' if product.get('stock_status') == 'outofstock' else 'continue',
            'weight': float(product.get('weight', 0)) if product.get('weight') else None,
            'status': self.map_product_status(product['status']),
            'tags': [tag['name'] for tag in product.get('tags', [])],
            'images': [{'url': img['src'], 'alt': img.get('alt', '')} for img in product.get('images', [])],
            'is_taxable': product.get('tax_status') != 'none',
            'requires_shipping': not product.get('virtual', False),
            'meta_data': {
                'woocommerce': {
                    'product_id': product['id'],
                    'type': product.get('type', 'simple'),
                    'tax_class': product.get('tax_class', ''),
                    'stock_status': product.get('stock_status', ''),
                    'manage_stock': product.get('manage_stock', False),
                    'backorders': product.get('backorders', 'no'),
                    'sold_individually': product.get('sold_individually', False)
                }
            }
        }
        
        # Handle variable products
        if product.get('type') == 'variable':
            product_data['options'] = self.transform_attributes(product.get('attributes', []))
            product_data['variants'] = self.migrate_variations(product['id'])
        
        return product_data
    
    def transform_attributes(self, attributes: list) -> list:
        """Transform WooCommerce attributes to R commerce options"""
        options = []
        for attr in attributes:
            if attr.get('variation', False):
                options.append({
                    'name': attr['name'],
                    'position': attr.get('position', 0),
                    'values': attr.get('options', [])
                })
        return options
    
    def migrate_variations(self, product_id: int) -> list:
        """Fetch and transform product variations"""
        variations = self._wc_get(f'products/{product_id}/variations', {'per_page': 100})
        variants = []
        
        for variation in variations:
            variant = {
                'sku': variation.get('sku', ''),
                'price': float(variation.get('price', 0)),
                'regular_price': float(variation.get('regular_price', 0)),
                'sale_price': float(variation.get('sale_price', 0)) if variation.get('sale_price') else None,
                'inventory_quantity': variation.get('stock_quantity', 0) or 0,
                'inventory_policy': 'deny' if variation.get('stock_status') == 'outofstock' else 'continue',
                'weight': float(variation.get('weight', 0)) if variation.get('weight') else None,
                'options': {attr['name']: attr['option'] for attr in variation.get('attributes', [])},
                'meta_data': {
                    'woocommerce': {
                        'variation_id': variation['id'],
                        'virtual': variation.get('virtual', False),
                        'downloadable': variation.get('downloadable', False)
                    }
                }
            }
            variants.append(variant)
        
        return variants
    
    def migrate_customers(self):
        """Migrate WooCommerce customers to R commerce"""
        page = 1
        while True:
            customers = self._wc_get('customers', {'per_page': 100, 'page': page})
            if not customers:
                break
            
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
            
            page += 1
    
    def transform_customer(self, customer: dict) -> dict:
        """Transform WooCommerce customer to R commerce format"""
        billing = customer.get('billing', {})
        shipping = customer.get('shipping', {})
        
        return {
            'email': customer['email'],
            'first_name': customer.get('first_name', ''),
            'last_name': customer.get('last_name', ''),
            'phone': billing.get('phone', ''),
            'accepts_marketing': customer.get('is_paying_customer', False),
            'billing_address': {
                'first_name': billing.get('first_name', ''),
                'last_name': billing.get('last_name', ''),
                'company': billing.get('company', ''),
                'address1': billing.get('address_1', ''),
                'address2': billing.get('address_2', ''),
                'city': billing.get('city', ''),
                'state': billing.get('state', ''),
                'postal_code': billing.get('postcode', ''),
                'country': billing.get('country', ''),
                'phone': billing.get('phone', '')
            },
            'shipping_address': {
                'first_name': shipping.get('first_name', ''),
                'last_name': shipping.get('last_name', ''),
                'company': shipping.get('company', ''),
                'address1': shipping.get('address_1', ''),
                'address2': shipping.get('address_2', ''),
                'city': shipping.get('city', ''),
                'state': shipping.get('state', ''),
                'postal_code': shipping.get('postcode', ''),
                'country': shipping.get('country', '')
            },
            'meta_data': {
                'woocommerce': {
                    'customer_id': customer['id'],
                    'username': customer.get('username', ''),
                    'role': customer.get('role', ''),
                    'is_paying_customer': customer.get('is_paying_customer', False)
                }
            }
        }
    
    def migrate_orders(self):
        """Migrate WooCommerce orders to R commerce"""
        page = 1
        while True:
            orders = self._wc_get('orders', {'per_page': 100, 'page': page})
            if not orders:
                break
            
            for order in orders:
                try:
                    order_data = self.transform_order(order)
                    
                    response = requests.post(
                        f"{self.rcommerce_url}/v1/orders",
                        json=order_data,
                        headers={'Authorization': f'Bearer {self.rcommerce_key}'}
                    )
                    
                    if response.status_code == 201:
                        print(f" Migrated order: {order['id']}")
                        self.migration_log.append({
                            'type': 'order',
                            'operation': 'create',
                            'status': 'success',
                            'source_id': order['id'],
                            'target_id': response.json()['data']['id']
                        })
                    else:
                        print(f" Failed to migrate order {order['id']}: {response.text}")
                    
                    time.sleep(0.5)
                    
                except Exception as e:
                    print(f" Error migrating order {order.get('id', 'unknown')}: {e}")
            
            page += 1
    
    def transform_order(self, order: dict) -> dict:
        """Transform WooCommerce order to R commerce format"""
        return {
            'order_number': str(order['id']),
            'customer_email': order.get('billing', {}).get('email', ''),
            'customer_first_name': order.get('billing', {}).get('first_name', ''),
            'customer_last_name': order.get('billing', {}).get('last_name', ''),
            'status': self.map_order_status(order['status']),
            'subtotal': float(order.get('subtotal', 0)),
            'tax_amount': float(order.get('total_tax', 0)),
            'shipping_amount': float(order.get('shipping_total', 0)),
            'discount_amount': float(order.get('discount_total', 0)),
            'total': float(order.get('total', 0)),
            'billing_address': {
                'first_name': order.get('billing', {}).get('first_name', ''),
                'last_name': order.get('billing', {}).get('last_name', ''),
                'company': order.get('billing', {}).get('company', ''),
                'address1': order.get('billing', {}).get('address_1', ''),
                'address2': order.get('billing', {}).get('address_2', ''),
                'city': order.get('billing', {}).get('city', ''),
                'state': order.get('billing', {}).get('state', ''),
                'postal_code': order.get('billing', {}).get('postcode', ''),
                'country': order.get('billing', {}).get('country', ''),
                'phone': order.get('billing', {}).get('phone', '')
            },
            'shipping_address': {
                'first_name': order.get('shipping', {}).get('first_name', ''),
                'last_name': order.get('shipping', {}).get('last_name', ''),
                'company': order.get('shipping', {}).get('company', ''),
                'address1': order.get('shipping', {}).get('address_1', ''),
                'address2': order.get('shipping', {}).get('address_2', ''),
                'city': order.get('shipping', {}).get('city', ''),
                'state': order.get('shipping', {}).get('state', ''),
                'postal_code': order.get('shipping', {}).get('postcode', ''),
                'country': order.get('shipping', {}).get('country', '')
            },
            'line_items': [self.transform_line_item(item) for item in order.get('line_items', [])],
            'meta_data': {
                'woocommerce': {
                    'order_id': order['id'],
                    'order_key': order.get('order_key', ''),
                    'payment_method': order.get('payment_method', ''),
                    'payment_method_title': order.get('payment_method_title', ''),
                    'transaction_id': order.get('transaction_id', ''),
                    'date_created': order.get('date_created', ''),
                    'date_modified': order.get('date_modified', '')
                }
            }
        }
    
    def transform_line_item(self, item: dict) -> dict:
        """Transform order line item"""
        return {
            'product_id': str(item.get('product_id', '')),
            'name': item.get('name', ''),
            'sku': item.get('sku', ''),
            'quantity': item.get('quantity', 0),
            'unit_price': float(item.get('price', 0)),
            'total': float(item.get('total', 0)),
            'meta_data': {
                'woocommerce': {
                    'variation_id': item.get('variation_id'),
                    'tax_class': item.get('tax_class', '')
                }
            }
        }
    
    def map_product_status(self, wc_status: str) -> str:
        """Map WooCommerce product status to R commerce status"""
        status_map = {
            'publish': 'active',
            'draft': 'draft',
            'private': 'archived',
            'pending': 'draft'
        }
        return status_map.get(wc_status, 'draft')
    
    def map_order_status(self, wc_status: str) -> str:
        """Map WooCommerce order status to R commerce status"""
        status_map = {
            'pending': 'pending',
            'processing': 'processing',
            'on-hold': 'on_hold',
            'completed': 'completed',
            'cancelled': 'cancelled',
            'refunded': 'refunded',
            'failed': 'failed'
        }
        return status_map.get(wc_status, 'pending')
    
    def save_migration_log(self):
        """Save migration log to file"""
        timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
        filename = f'migration_log_{timestamp}.json'
        
        with open(filename, 'w') as f:
            json.dump({
                'timestamp': timestamp,
                'summary': {
                    'total_products': len([l for l in self.migration_log if l['type'] == 'product']),
                    'successful_products': len([l for l in self.migration_log if l['type'] == 'product' and l['status'] == 'success']),
                    'failed_products': len([l for l in self.migration_log if l['type'] == 'product' and l['status'] == 'failed']),
                    'total_categories': len([l for l in self.migration_log if l['type'] == 'category']),
                    'successful_categories': len([l for l in self.migration_log if l['type'] == 'category' and l['status'] == 'success']),
                },
                'details': self.migration_log
            }, f, indent=2)
        
        print(f"Migration log saved to {filename}")

# Usage
if __name__ == '__main__':
    wc_config = {
        'url': os.environ.get('WC_URL', 'https://your-store.com'),
        'consumer_key': os.environ.get('WC_CONSUMER_KEY', 'your_consumer_key'),
        'consumer_secret': os.environ.get('WC_CONSUMER_SECRET', 'your_consumer_secret')
    }
    
    rcommerce_config = {
        'url': os.environ.get('RCOMMERCE_URL', 'https://api.yourstore.com'),
        'api_key': os.environ.get('RCOMMERCE_API_KEY', 'your_api_key')
    }
    
    migrator = WooCommerceMigrator(wc_config, rcommerce_config)
    migrator.migrate_all()
```

## Plugin Data Migration

WooCommerce stores extension data in various locations. Access this data via the REST API or WP-CLI:

### Common Plugin Data Access

**WooCommerce Subscriptions:**
```bash
# Export subscriptions using WP-CLI
wp wc shop_subscription list --user=1 --format=json > subscriptions.json
```

**WooCommerce Bookings:**
```bash
# Export bookings
wp wc booking list --user=1 --format=json > bookings.json
```

**WooCommerce Memberships:**
```bash
# Export memberships
wp wc user_membership list --user=1 --format=json > memberships.json
```

**Yoast SEO:**
```php
<?php
// Access Yoast SEO data via WordPress functions
$yoast_title = get_post_meta($product_id, '_yoast_wpseo_title', true);
$yoast_description = get_post_meta($product_id, '_yoast_wpseo_metadesc', true);
$yoast_focus_keyword = get_post_meta($product_id, '_yoast_wpseo_focuskw', true);
```

**Advanced Custom Fields:**
```php
<?php
// Access ACF fields via WordPress functions
if (function_exists('get_fields')) {
    $acf_fields = get_fields($product_id);
    // Process ACF fields for migration
}
```

### Migration Strategy for Plugins

```php
<?php
// migrate-plugin-data.php

class WooCommercePluginDataMigrator {
  
  public function migrateSubscriptionData($rcommerceProductId, $woocommerceProductId) {
    // Check if WooCommerce Subscriptions is active
    if (!class_exists('WC_Subscriptions')) {
      return null;
    }
    
    // Get subscriptions for this product via API or WP-CLI export
    $subscriptions = $this->getProductSubscriptions($woocommerceProductId);
    
    foreach ($subscriptions as $subscription) {
      // Transform subscription data for R commerce
      $subscription_data = [
        'product_id' => $rcommerceProductId,
        'interval' => $subscription->billing_interval,
        'period' => $subscription->billing_period, // day, week, month, year
        'price' => $subscription->billing_amount,
        'trial_period' => $subscription->trial_period,
        'trial_interval' => $subscription->trial_interval,
        'meta_data' => [
          'woocommerce' => [
            'subscription_id' => $subscription->id,
            'status' => $subscription->status,
            'start_date' => $subscription->start_date,
            'expiry_date' => $subscription->expiry_date,
            'end_date' => $subscription->end_date,
            'requires_manual_renewal' => $subscription->requires_manual_renewal,
            'billing_period' => $subscription->billing_period,
            'billing_interval' => $subscription->billing_interval,
            'next_payment_date' => $subscription->next_payment_date
          ]
        ]
      ];
      
      // Store in R commerce meta_data for future subscription implementation
      update_post_meta($rcommerceProductId, '_subscription_data', $subscription_data);
    }
  }
  
  public function migrateBookingData($rcommerceProductId, $woocommerceProductId) {
    if (!class_exists('WC_Bookings')) {
      return null;
    }
    
    $bookable_product = new WC_Product_Booking($woocommerceProductId);
    
    $booking_data = [
      'duration_type' => $bookable_product->get_duration_type(),
      'duration' => $bookable_product->get_duration(),
      'duration_unit' => $bookable_product->get_duration_unit(),
      'calendar_display_mode' => $bookable_product->get_calendar_display_mode(),
      'requires_confirmation' => $bookable_product->get_requires_confirmation(),
      'can_be_cancelled' => $bookable_product->get_can_be_cancelled(),
      'cancel_limit' => $bookable_product->get_cancel_limit(),
      'cancel_limit_unit' => $bookable_product->get_cancel_limit_unit(),
      'min_date' => $bookable_product->get_min_date(),
      'max_date' => $bookable_product->get_max_date(),
      'max_date_value' => $bookable_product->get_max_date_value(),
      'max_date_unit' => $bookable_product->get_max_date_unit(),
      'buffer_period' => $bookable_product->get_buffer_period(),
      'availability_rules' => $bookable_product->get_availability_rules()
    ];
    
    // Store booking data for future implementation
    update_post_meta($rcommerceProductId, '_booking_data', $booking_data);
  }
  
  public function migrateACFData($rcommerceProductId, $woocommerceProductId) {
    // Advanced Custom Fields data
    if (!function_exists('get_field')) {
      return;
    }
    
    $acf_fields = get_fields($woocommerceProductId);
    
    if ($acf_fields) {
      update_post_meta($rcommerceProductId, '_acf_data', $acf_fields);
    }
  }
}
```

## Post-Migration Cleanup

After migration:

```bash
#!/bin/bash
# cleanup-woocommerce.sh

echo "Cleaning up WooCommerce installation..."

# Create backup
echo "Creating full backup..."
wp db export wordpress-pre-rcommerce-backup.sql

# Disable WooCommerce
echo "Deactivating WooCommerce..."
wp plugin deactivate woocommerce
wp plugin deactivate woocommerce-*

# Redirect all traffic
echo "Setting up redirects..."
cp wp-config.php wp-config.php.backup

# Add redirect to R commerce
cat >> wp-config.php << 'EOF'
// Redirect all WooCommerce pages to R commerce
if (strpos($_SERVER['REQUEST_URI'], '/shop') === 0 ||
    strpos($_SERVER['REQUEST_URI'], '/product') === 0 ||
    strpos($_SERVER['REQUEST_URI'], '/cart') === 0 ||
    strpos($_SERVER['REQUEST_URI'], '/checkout') === 0) {
    wp_redirect('https://your-new-store.com' . $_SERVER['REQUEST_URI'], 301);
    exit;
}
EOF

echo "Cleanup complete!"
```

## WordPress Integration Removal

To completely disconnect from WordPress:

```php
<?php
// wp-config-rcommerce-transition.php

// Keep WordPress for content only, remove all ecommerce functionality

// Disable WooCommerce completely
define('WC_ABSPATH', '');
define('WOOCOMMERCE_ABSPATH', '');

// Prevent WooCommerce from loading
add_action('plugins_loaded', function() {
    remove_action('plugins_loaded', 'woocommerce_init', 10);
}, 0);

// Redirect all WooCommerce endpoints
add_action('init', function() {
    $woocommerce_pages = array(
        'shop' => '/shop/',
        'cart' => '/cart/',
        'checkout' => '/checkout/',
        'myaccount' => '/my-account/',
        'terms' => '/terms-and-conditions/'
    );
    
    foreach ($woocommerce_pages as $page => $url) {
        $page_id = wc_get_page_id($page);
        if ($page_id > 0) {
            wp_delete_post($page_id, true);
        }
    }
});

// Add JavaScript to redirect immediately
add_action('wp_head', function() {
    ?>
    <script>
    if (window.location.pathname.indexOf('/shop') === 0 ||
        window.location.pathname.indexOf('/product') === 0 ||
        window.location.pathname.indexOf('/cart') === 0 ||
        window.location.pathname.indexOf('/checkout') === 0) {
        window.location.href = 'https://your-new-store.com' + window.location.pathname;
    }
    </script>
    <?php
});
```

## Testing Checklist

Post-migration verification:

### Data Integrity
- [ ] All products migrated (WC count matches R commerce)
- [ ] All customers migrated (WC count matches R commerce)
- [ ] Product relationships preserved
- [ ] Customer addresses accurate
- [ ] Order totals match

### Functionality
- [ ] Products searchable
- [ ] Categories browsable
- [ ] Cart functionality works
- [ ] Checkout process completes
- [ ] Payment gateways functional
- [ ] Shipping calculators accurate

### SEO
- [ ] Product URLs redirect correctly
- [ ] Category URLs redirect
- [ ] Page URLs redirect
- [ ] Sitemap submitted to Google
- [ ] Search Console monitoring active

### WordPress Content
- [ ] Blog posts unaffected
- [ ] Pages load correctly
- [ ] Media library accessible
- [ ] User logins functional
- [ ] Admin dashboard works

## Troubleshooting

### Common Issues

1. **API Rate Limiting**
   ```python
   # Add delays between requests
   import time
   time.sleep(1)  # 1 second delay
   ```

2. **Memory Limits**
   ```bash
   # Increase PHP memory limit for migration
   php -d memory_limit=512M migrate-woocommerce.php
   ```

3. **Large Catalog Timeouts**
   ```python
   # Process in smaller batches
   BATCH_SIZE = 50  # Reduce from 100
   ```

4. **WordPress User Role Issues**
   ```php
   // After migration, clean up user roles
   if (function_exists('wp_roles')) {
       wp_roles()->remove_role('customer');
       wp_roles()->remove_cap('shop_manager');
   }
   ```

This comprehensive guide covers the unique aspects of migrating from WooCommerce to R commerce, including WordPress integration, plugin compatibility, and data structure differences using API-based approaches.
