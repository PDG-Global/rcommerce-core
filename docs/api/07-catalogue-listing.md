# Catalogue listing and search

`GET /api/v1/products` returns active products, newest first, with `meta.total`,
`meta.page`, `meta.per_page` and `meta.total_pages`.

| Parameter | Meaning |
|---|---|
| `category` | category slug |
| `q` | search (below) |
| `sort` | `newest` (default), `price_asc`, `price_desc`, `name_asc`, `name_desc` |
| `min_price`, `max_price` | price range, excluding tax (inclusive) |
| `page`, `per_page` | 1-based page, 1 to 100 per page (default 20) |

An unknown `sort`, or `min_price` above `max_price`, is a `400` validation error.
`meta.total` counts with the same filters as the page.

## Search

`q` is split into words (at most 8). Every word must appear in the title, the
description, the SKU or a variant SKU. Both the words and the product texts are
compared in a normalised form (SQL function `catalog_search_text`, migration 030):

- lower case, accents removed: `beton` finds "Béton";
- spaces removed: `25kg` finds "25 kg", and `ciment portland` still matches;
- a comma between digits read as a decimal point: `1.2mm` finds "1,2 mm";
- `%` and `_` are taken literally.

When `q` equals a product's SKU (ignoring case), that product comes first, before
the requested `sort`.

```bash
GET /api/v1/products?q=st-0005
GET /api/v1/products?q=ciment+25kg&sort=price_asc&min_price=5&max_price=20
```
