# Statistics API Documentation

The Statistics API provides comprehensive analytics and reporting capabilities for administrators to monitor store performance, track sales metrics, analyze customer behavior, and make data-driven business decisions.

## Overview

The Statistics API aggregates data across orders, products, customers, and payments to provide:

- **Real-time Dashboard Metrics** - Key performance indicators at a glance
- **Sales Analytics** - Revenue trends, growth rates, and period comparisons
- **Order Insights** - Order volumes, fulfillment rates, and status breakdowns
- **Product Performance** - Top sellers, inventory turnover, and product analytics
- **Customer Metrics** - Acquisition, retention, and lifetime value analysis
- **Subscription Analytics** - MRR, churn rates, and recurring revenue metrics

## Base URL

```
/api/v1/admin/statistics
```

## Authentication

All statistics endpoints require **admin-level authentication** via JWT token or API key with `admin` scope.

```http
Authorization: Bearer YOUR_ADMIN_API_KEY
```

### Required Scopes

| Endpoint | Required Scope |
|----------|---------------|
| All statistics endpoints | `admin` or `statistics:read` |

## Common Query Parameters

Most statistics endpoints support the following query parameters for date filtering and data aggregation:

| Parameter | Type | Description |
|-----------|------|-------------|
| `date_from` | ISO 8601 date | Start date for the reporting period (e.g., `2024-01-01`) |
| `date_to` | ISO 8601 date | End date for the reporting period (e.g., `2024-01-31`) |
| `period` | string | Aggregation period: `day`, `week`, `month`, `quarter`, `year` |
| `currency` | string | ISO 4217 currency code for monetary values (default: store default) |
| `compare_with` | string | Previous period to compare: `previous_period`, `previous_year` |

### Date Range Presets

Instead of explicit dates, you can use preset values for `date_from` and `date_to`:

| Preset | Description |
|--------|-------------|
| `today` | Current day (00:00 to now) |
| `yesterday` | Previous full day |
| `this_week` | Current week (Monday-Sunday) |
| `last_week` | Previous full week |
| `this_month` | Current calendar month |
| `last_month` | Previous calendar month |
| `this_quarter` | Current fiscal quarter |
| `last_quarter` | Previous fiscal quarter |
| `this_year` | Current calendar year |
| `last_year` | Previous calendar year |
| `last_7_days` | Last 7 days including today |
| `last_30_days` | Last 30 days including today |
| `last_90_days` | Last 90 days including today |

## Endpoints

### Dashboard Overview

```http
GET /api/v1/admin/statistics/dashboard
```

Returns a high-level overview of key metrics for the dashboard. This is the primary endpoint for admin dashboard displays.

#### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `date_from` | date | `today` | Start of reporting period |
| `date_to` | date | `today` | End of reporting period |
| `compare_with` | string | `previous_period` | Comparison period |

#### Example Request

```http
GET /api/v1/admin/statistics/dashboard?date_from=last_30_days&compare_with=previous_period
Authorization: Bearer sk_live_admin_xxx
```

#### Example Response

```json
{
  "data": {
    "period": {
      "from": "2024-01-01",
      "to": "2024-01-30",
      "days": 30
    },
    "sales": {
      "total_revenue": {
        "amount": "45678.90",
        "currency": "USD",
        "change_percentage": 12.5,
        "change_direction": "up",
        "comparison_period": "2023-12-02 to 2023-12-31"
      },
      "total_orders": {
        "count": 342,
        "change_percentage": 8.2,
        "change_direction": "up"
      },
      "average_order_value": {
        "amount": "133.56",
        "currency": "USD",
        "change_percentage": 3.9,
        "change_direction": "up"
      }
    },
    "orders": {
      "by_status": {
        "pending": 12,
        "processing": 28,
        "completed": 285,
        "cancelled": 17
      },
      "fulfillment_rate": 83.3,
      "cancellation_rate": 5.0
    },
    "customers": {
      "new_customers": 89,
      "returning_customers": 253,
      "total_customers": 15420,
      "conversion_rate": 3.2
    },
    "products": {
      "top_selling": [
        {
          "product_id": "550e8400-e29b-41d4-a716-446655440001",
          "title": "Premium Wireless Headphones",
          "sku": "WH-001",
          "quantity_sold": 156,
          "revenue": "23400.00"
        },
        {
          "product_id": "550e8400-e29b-41d4-a716-446655440002",
          "title": "Organic Cotton T-Shirt",
          "sku": "TSH-001",
          "quantity_sold": 142,
          "revenue": "4258.00"
        }
      ],
      "low_stock_count": 8
    },
    "payments": {
      "successful": 325,
      "failed": 12,
      "refunded": 5,
      "refund_amount": "450.00"
    },
    "subscriptions": {
      "active_subscribers": 1245,
      "mrr": "12450.00",
      "churn_rate": 2.1,
      "new_subscriptions": 45
    }
  },
  "meta": {
    "request_id": "req_stats_abc123",
    "timestamp": "2024-01-30T14:30:00Z",
    "cache_ttl": 300
  }
}
```

#### Response Fields

| Field | Type | Description |
|-------|------|-------------|
| `period` | object | The reporting period covered |
| `sales.total_revenue` | object | Total revenue with comparison |
| `sales.total_orders` | object | Total order count with comparison |
| `sales.average_order_value` | object | AOV with comparison |
| `orders.by_status` | object | Order count breakdown by status |
| `orders.fulfillment_rate` | decimal | Percentage of orders fulfilled |
| `orders.cancellation_rate` | decimal | Percentage of orders cancelled |
| `customers.new_customers` | integer | New customers in period |
| `customers.returning_customers` | integer | Returning customers in period |
| `customers.conversion_rate` | decimal | Visitor to customer conversion % |
| `products.top_selling` | array | Top 5 selling products |
| `products.low_stock_count` | integer | Products below threshold |
| `payments.successful` | integer | Successful payment count |
| `payments.failed` | integer | Failed payment count |
| `payments.refunded` | integer | Refund count |
| `subscriptions.mrr` | string | Monthly Recurring Revenue |
| `subscriptions.churn_rate` | decimal | Monthly churn percentage |

---

### Sales Statistics

```http
GET /api/v1/admin/statistics/sales
```

Returns detailed sales metrics with time-series data for charting and analysis.

#### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `date_from` | date | `this_month` | Start date |
| `date_to` | date | `today` | End date |
| `period` | string | `day` | Aggregation: `day`, `week`, `month` |
| `group_by` | string | null | Additional grouping: `product`, `category`, `channel` |
| `currency` | string | store default | Currency for amounts |

#### Example Request

```http
GET /api/v1/admin/statistics/sales?date_from=2024-01-01&date_to=2024-01-31&period=day
Authorization: Bearer sk_live_admin_xxx
```

#### Example Response

```json
{
  "data": {
    "summary": {
      "total_revenue": "156789.50",
      "total_orders": 1247,
      "average_order_value": "125.73",
      "total_discounts": "8765.00",
      "total_tax": "12543.20",
      "total_shipping": "6234.50",
      "net_revenue": "135246.80"
    },
    "comparison": {
      "revenue_change": 15.3,
      "orders_change": 8.7,
      "aov_change": 6.1
    },
    "time_series": [
      {
        "date": "2024-01-01",
        "revenue": "5234.50",
        "orders": 42,
        "aov": "124.63",
        "discounts": "234.00"
      },
      {
        "date": "2024-01-02",
        "revenue": "4890.25",
        "orders": 38,
        "aov": "128.69",
        "discounts": "156.00"
      }
    ],
    "by_channel": [
      {
        "channel": "web",
        "revenue": "125000.00",
        "orders": 980,
        "percentage": 79.7
      },
      {
        "channel": "mobile_app",
        "revenue": "28789.50",
        "orders": 245,
        "percentage": 19.7
      },
      {
        "channel": "pos",
        "revenue": "3000.00",
        "orders": 22,
        "percentage": 0.6
      }
    ],
    "by_payment_method": [
      {
        "method": "credit_card",
        "revenue": "98765.00",
        "orders": 780,
        "percentage": 62.5
      },
      {
        "method": "paypal",
        "revenue": "34567.50",
        "orders": 289,
        "percentage": 23.2
      },
      {
        "method": "alipay",
        "revenue": "23457.00",
        "orders": 178,
        "percentage": 14.3
      }
    ]
  },
  "meta": {
    "request_id": "req_stats_sales_001",
    "timestamp": "2024-01-30T14:30:00Z"
  }
}
```

---

### Order Statistics

```http
GET /api/v1/admin/statistics/orders
```

Returns detailed order metrics including status breakdowns, fulfillment analytics, and geographic distribution.

#### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `date_from` | date | `this_month` | Start date |
| `date_to` | date | `today` | End date |
| `period` | string | `day` | Aggregation period |
| `include_geo` | boolean | false | Include geographic distribution |

#### Example Request

```http
GET /api/v1/admin/statistics/orders?date_from=last_90_days&include_geo=true
Authorization: Bearer sk_live_admin_xxx
```

#### Example Response

```json
{
  "data": {
    "summary": {
      "total_orders": 3456,
      "total_items": 8765,
      "average_items_per_order": 2.54,
      "average_processing_time_hours": 18.5
    },
    "by_status": {
      "pending": 45,
      "confirmed": 123,
      "processing": 234,
      "on_hold": 12,
      "completed": 2890,
      "cancelled": 128,
      "refunded": 24
    },
    "by_fulfillment_status": {
      "unfulfilled": 402,
      "partial": 156,
      "fulfilled": 2890,
      "returned": 8
    },
    "by_payment_status": {
      "pending": 45,
      "authorized": 89,
      "paid": 3194,
      "failed": 78,
      "refunded": 50
    },
    "time_series": [
      {
        "date": "2024-01-01",
        "total": 45,
        "completed": 38,
        "cancelled": 2,
        "refunded": 1
      }
    ],
    "geographic_distribution": [
      {
        "country": "US",
        "orders": 2156,
        "revenue": "98765.00",
        "percentage": 62.4
      },
      {
        "country": "CA",
        "orders": 456,
        "revenue": "23456.00",
        "percentage": 13.2
      },
      {
        "country": "GB",
        "orders": 234,
        "revenue": "15678.00",
        "percentage": 6.8
      }
    ],
    "fulfillment_metrics": {
      "average_fulfillment_time_hours": 24.5,
      "same_day_fulfillment_rate": 35.2,
      "within_24h_rate": 68.5,
      "within_48h_rate": 89.3
    }
  },
  "meta": {
    "request_id": "req_stats_orders_001",
    "timestamp": "2024-01-30T14:30:00Z"
  }
}
```

---

### Product Statistics

```http
GET /api/v1/admin/statistics/products
```

Returns product performance metrics including top sellers, inventory analytics, and product-specific revenue data.

#### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `date_from` | date | `this_month` | Start date |
| `date_to` | date | `today` | End date |
| `limit` | integer | 20 | Number of products to return |
| `sort_by` | string | `revenue` | Sort: `revenue`, `quantity`, `profit` |
| `category_id` | UUID | null | Filter by category |

#### Example Request

```http
GET /api/v1/admin/statistics/products?date_from=2024-01-01&date_to=2024-01-31&limit=10&sort_by=revenue
Authorization: Bearer sk_live_admin_xxx
```

#### Example Response

```json
{
  "data": {
    "summary": {
      "total_products_sold": 8765,
      "unique_products_sold": 234,
      "total_revenue": "156789.50",
      "total_cost": "78945.00",
      "gross_profit": "77844.50",
      "gross_margin": 49.6
    },
    "top_products": [
      {
        "product_id": "550e8400-e29b-41d4-a716-446655440001",
        "title": "Premium Wireless Headphones",
        "sku": "WH-001",
        "category": "Electronics",
        "quantity_sold": 456,
        "revenue": "68400.00",
        "cost": "34200.00",
        "profit": "34200.00",
        "margin": 50.0,
        "return_rate": 2.1
      },
      {
        "product_id": "550e8400-e29b-41d4-a716-446655440002",
        "title": "Organic Cotton T-Shirt",
        "sku": "TSH-001",
        "category": "Clothing",
        "quantity_sold": 389,
        "revenue": "11670.00",
        "cost": "4670.00",
        "profit": "7000.00",
        "margin": 60.0,
        "return_rate": 5.4
      }
    ],
    "by_category": [
      {
        "category_id": "550e8400-e29b-41d4-a716-446655440100",
        "name": "Electronics",
        "revenue": "85000.00",
        "quantity": 567,
        "percentage": 54.2
      },
      {
        "category_id": "550e8400-e29b-41d4-a716-446655440101",
        "name": "Clothing",
        "revenue": "45000.00",
        "quantity": 1234,
        "percentage": 28.7
      }
    ],
    "inventory_metrics": {
      "total_sku_count": 456,
      "low_stock_count": 23,
      "out_of_stock_count": 8,
      "average_inventory_turnover": 4.2,
      "inventory_value": "234567.00"
    }
  },
  "meta": {
    "request_id": "req_stats_products_001",
    "timestamp": "2024-01-30T14:30:00Z"
  }
}
```

---

### Customer Statistics

```http
GET /api/v1/admin/statistics/customers
```

Returns customer analytics including acquisition metrics, retention rates, lifetime value, and segmentation data.

#### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `date_from` | date | `this_month` | Start date |
| `date_to` | date | `today` | End date |
| `include_segments` | boolean | false | Include customer segment breakdown |

#### Example Request

```http
GET /api/v1/admin/statistics/customers?date_from=last_90_days&include_segments=true
Authorization: Bearer sk_live_admin_xxx
```

#### Example Response

```json
{
  "data": {
    "summary": {
      "total_customers": 15420,
      "new_customers": 456,
      "returning_customers": 789,
      "churned_customers": 23,
      "reactivated_customers": 45
    },
    "acquisition": {
      "total_new": 456,
      "by_source": [
        {
          "source": "organic_search",
          "count": 156,
          "percentage": 34.2
        },
        {
          "source": "paid_ads",
          "count": 134,
          "percentage": 29.4
        },
        {
          "source": "social_media",
          "count": 89,
          "percentage": 19.5
        },
        {
          "source": "referral",
          "count": 45,
          "percentage": 9.9
        },
        {
          "source": "direct",
          "count": 32,
          "percentage": 7.0
        }
      ],
      "cost_per_acquisition": "12.50",
      "conversion_rate": 3.2
    },
    "retention": {
      "repeat_purchase_rate": 42.5,
      "average_time_between_orders_days": 45,
      "customer_lifetime_value": {
        "average": "456.78",
        "median": "234.50",
        "top_10_percent": "2345.00"
      },
      "cohort_analysis": [
        {
          "cohort": "2024-01",
          "customers": 156,
          "month_0": 100,
          "month_1": 45,
          "month_2": 38,
          "month_3": 32
        }
      ]
    },
    "segments": [
      {
        "segment": "vip",
        "count": 234,
        "average_order_value": "345.67",
        "total_revenue": "80956.78",
        "percentage": 1.5
      },
      {
        "segment": "loyal",
        "count": 1234,
        "average_order_value": "156.78",
        "total_revenue": "193456.52",
        "percentage": 8.0
      },
      {
        "segment": "at_risk",
        "count": 456,
        "average_order_value": "89.50",
        "total_revenue": "40812.00",
        "percentage": 3.0
      },
      {
        "segment": "new",
        "count": 456,
        "average_order_value": "78.90",
        "total_revenue": "35978.40",
        "percentage": 3.0
      }
    ],
    "geographic_distribution": [
      {
        "country": "US",
        "customers": 8765,
        "percentage": 56.8
      },
      {
        "country": "CA",
        "customers": 1234,
        "percentage": 8.0
      }
    ]
  },
  "meta": {
    "request_id": "req_stats_customers_001",
    "timestamp": "2024-01-30T14:30:00Z"
  }
}
```

---

### Subscription Statistics

```http
GET /api/v1/admin/statistics/subscriptions
```

Returns subscription and recurring revenue metrics including MRR, ARR, churn analysis, and growth rates.

#### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `date_from` | date | `this_month` | Start date |
| `date_to` | date | `today` | End date |
| `period` | string | `month` | Aggregation period |

#### Example Request

```http
GET /api/v1/admin/statistics/subscriptions?date_from=2024-01-01&period=month
Authorization: Bearer sk_live_admin_xxx
```

#### Example Response

```json
{
  "data": {
    "summary": {
      "active_subscribers": 1245,
      "active_subscriptions": 1345,
      "mrr": "12450.00",
      "arr": "149400.00",
      "arpu": "10.00",
      "arpu_annual": "120.00",
      "ltv": "480.00"
    },
    "changes": {
      "new_subscriptions": 45,
      "cancelled_subscriptions": 12,
      "net_change": 33,
      "churn_rate": 2.1,
      "growth_rate": 8.5
    },
    "mrr_breakdown": {
      "new_business": "450.00",
      "expansion": "120.00",
      "contraction": "-45.00",
      "churn": "-120.00",
      "net_new": "405.00"
    },
    "by_plan": [
      {
        "plan_id": "550e8400-e29b-41d4-a716-446655440200",
        "name": "Basic",
        "subscribers": 678,
        "mrr": "3390.00",
        "percentage": 27.2
      },
      {
        "plan_id": "550e8400-e29b-41d4-a716-446655440201",
        "name": "Pro",
        "subscribers": 456,
        "mrr": "6840.00",
        "percentage": 54.9
      },
      {
        "plan_id": "550e8400-e29b-41d4-a716-446655440202",
        "name": "Enterprise",
        "subscribers": 111,
        "mrr": "2220.00",
        "percentage": 17.9
      }
    ],
    "time_series": [
      {
        "date": "2024-01-01",
        "mrr": "12045.00",
        "subscribers": 1212,
        "new": 12,
        "churned": 3
      },
      {
        "date": "2024-01-02",
        "mrr": "12085.00",
        "subscribers": 1218,
        "new": 8,
        "churned": 2
      }
    ],
    "churn_analysis": {
      "overall_churn_rate": 2.1,
      "voluntary_churn_rate": 1.4,
      "involuntary_churn_rate": 0.7,
      "churn_reasons": [
        {
          "reason": "too_expensive",
          "count": 45,
          "percentage": 35.2
        },
        {
          "reason": "not_using",
          "count": 38,
          "percentage": 29.7
        },
        {
          "reason": "switched_competitor",
          "count": 23,
          "percentage": 18.0
        },
        {
          "reason": "missing_features",
          "count": 22,
          "percentage": 17.1
        }
      ]
    }
  },
  "meta": {
    "request_id": "req_stats_subs_001",
    "timestamp": "2024-01-30T14:30:00Z"
  }
}
```

---

## Error Responses

### Invalid Date Range

```json
{
  "error": {
    "code": "INVALID_DATE_RANGE",
    "message": "The date_from must be before date_to",
    "details": {
      "date_from": "2024-01-31",
      "date_to": "2024-01-01"
    }
  },
  "meta": {
    "request_id": "req_stats_err_001",
    "timestamp": "2024-01-30T14:30:00Z"
  }
}
```

### Insufficient Permissions

```json
{
  "error": {
    "code": "FORBIDDEN",
    "message": "Admin access required for statistics endpoints",
    "details": {
      "required_scope": "admin",
      "current_scopes": ["products:read", "orders:read"]
    }
  },
  "meta": {
    "request_id": "req_stats_err_002",
    "timestamp": "2024-01-30T14:30:00Z"
  }
}
```

### Date Range Too Large

```json
{
  "error": {
    "code": "DATE_RANGE_TOO_LARGE",
    "message": "Date range exceeds maximum of 365 days",
    "details": {
      "requested_days": 500,
      "max_days": 365
    }
  },
  "meta": {
    "request_id": "req_stats_err_003",
    "timestamp": "2024-01-30T14:30:00Z"
  }
}
```

## Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `INVALID_DATE_RANGE` | 400 | date_from is after date_to |
| `INVALID_PERIOD` | 400 | Unsupported period value |
| `DATE_RANGE_TOO_LARGE` | 400 | Date range exceeds maximum |
| `INVALID_CURRENCY` | 400 | Unsupported currency code |
| `FORBIDDEN` | 403 | Insufficient permissions |
| `STATS_UNAVAILABLE` | 503 | Statistics service temporarily unavailable |

## Rate Limiting

Statistics endpoints have specific rate limits due to data aggregation complexity:

| Endpoint | Limit |
|----------|-------|
| Dashboard | 60 requests/minute |
| Sales | 30 requests/minute |
| Orders | 30 requests/minute |
| Products | 30 requests/minute |
| Customers | 20 requests/minute |
| Subscriptions | 20 requests/minute |

## Caching

Statistics responses are cached to improve performance:

| Endpoint | Cache TTL |
|----------|-----------|
| Dashboard | 5 minutes |
| Sales | 10 minutes |
| Orders | 10 minutes |
| Products | 15 minutes |
| Customers | 15 minutes |
| Subscriptions | 5 minutes |

Use the `Cache-Control: no-cache` header to bypass cache for real-time data.

## Best Practices

1. **Use appropriate date ranges** - Avoid requesting more than 90 days of daily data
2. **Cache dashboard data** - Dashboard metrics change slowly; cache for 5+ minutes
3. **Use period aggregation** - Use `week` or `month` for longer date ranges
4. **Compare periods** - Use `compare_with` parameter for trend analysis
5. **Batch requests** - Request multiple metrics in a single dashboard call instead of multiple individual calls
