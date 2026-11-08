# Inventory API (stock management)

Back-office routes for stock per location. All of them sit behind the admin
middleware and also accept callers holding the `inventory:read` /
`inventory:write` scope.

Stock is tracked in two places that always move together:

- `inventory_levels`: one row per product (or variant) and location;
  `available` is the free stock, `reserved` the stock committed to orders and
  not yet shipped.
- `products.inventory_quantity` (and `product_variants.inventory_quantity`):
  the total the cart checks.

Every change below updates both and writes a `stock_movements` row, in one
transaction. Free stock never goes below zero.

## Stock levels

`GET /api/v1/admin/inventory/levels`

| Query | Meaning |
|---|---|
| `location_id` | Only this location |
| `product_id` | Only this product (all its variants) |
| `q` | Title, product SKU or variant SKU contains (case-insensitive) |
| `low_only=true` | Only rows at or below their reorder point |
| `page`, `per_page` | 50 per page by default, at most 200 |

```json
{
  "levels": [{
    "id": "…", "product_id": "…", "variant_id": null, "sku": "BLK-39", "title": "Concrete block 39x19x14",
    "location_id": "…", "location_code": "MAIN", "location_name": "Main warehouse",
    "available": 2400, "reserved": 120, "incoming": 0,
    "reorder_point": 200, "reorder_quantity": 1000,
    "last_counted_at": null, "updated_at": "2026-11-09T08:00:00Z"
  }],
  "pagination": {"page": 1, "per_page": 50, "total": 1, "total_pages": 1}
}
```

`PATCH /api/v1/admin/inventory/levels/:id` sets `{"reorder_point": 200, "reorder_quantity": 1000}`
(both ≥ 0) and returns `{"level": …}`.

## Adjustments

`POST /api/v1/admin/inventory/adjustments`

```json
{"product_id": "…", "variant_id": null, "location_id": "…", "quantity": -3, "reason": "damage", "notes": "pallet dropped"}
```

`quantity` is the change (never 0). `reason` is one of:

| Reason | Direction | Movement type |
|---|---|---|
| `correction` | either | `in` / `out` |
| `count` | either | `in` / `out` |
| `damage` | remove only | `lost` |
| `found` | add only | `found` |
| `return` | add only | `return` |

Returns `201 {"level": …}`. `400` when the reason does not allow the direction
or free stock would go below zero; `404` for an unknown product, variant (of
that product) or location.

## Goods receipts

`POST /api/v1/admin/inventory/receipts`

```json
{
  "location_id": "…",
  "reference": "Delivery note 4711",
  "notes": null,
  "lines": [{"product_id": "…", "variant_id": null, "quantity": 40, "cost_per_unit": "2.10"}]
}
```

1 to 500 lines, each with a positive quantity. All lines are booked or none.
The first receipt of a product at a location creates its stock level; a
`cost_per_unit` updates the level's unit cost. Returns
`201 {"levels": [...]}` in line order.

## Movement history

`GET /api/v1/admin/inventory/movements?location_id&product_id&from&to&page&per_page`

Newest first. `from` / `to` are RFC 3339 timestamps (`to` exclusive).

```json
{
  "movements": [{
    "id": "…", "product_id": "…", "variant_id": null, "sku": "BLK-39", "title": "Concrete block 39x19x14",
    "location_id": "…", "location_code": "MAIN", "quantity": -3, "movement_type": "lost",
    "reason": "damage", "reference": null, "notes": "pallet dropped", "cost_per_unit": null,
    "created_by": "…", "created_at": "2026-11-09T08:05:00Z"
  }],
  "pagination": {"page": 1, "per_page": 50, "total": 2, "total_pages": 1}
}
```

Movements written by orders (refund restock, for example) have `reason` and
`created_by` set to `null`.
