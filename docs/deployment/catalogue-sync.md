# Catalogue sync

A network of stores can share one catalogue. A catalogue store holds the
products, categories, descriptions and images. Each merchant store copies the
products of its assortment and adds what it owns: prices, stock, its own
products and categories.

Each store keeps its own database. The catalogue and the stores can run in one
process (see [several stores](multi-store.md)) or in separate ones.

## Setting it up

In the catalogue store, log changes:

```toml
[catalogue]
publish_changes = true
```

In each merchant store, point at the catalogue database:

```toml
[catalogue]
source_url = "postgres://catalogue_reader:secret@db.internal:5432/rc_catalogue"
sync_interval_seconds = 300   # 0 = only on demand
```

The source account only needs `SELECT` on the catalogue tables. Then choose the
assortment and run a first sync:

```bash
curl -X PUT  https://shop.example/api/v1/admin/catalogue/assortment \
     -H "Authorization: Bearer $ADMIN_TOKEN" -H "Content-Type: application/json" \
     -d '{"categories": ["<catalogue category id>"]}'
curl -X POST https://shop.example/api/v1/admin/catalogue/sync -H "Authorization: Bearer $ADMIN_TOKEN"
```

## What a sync does

- **Change log:** the catalogue database logs every product and category change
  in `catalogue_changes`, through triggers. A change to a variant, image, option
  or category link logs its product. With `publish_changes = false` (the
  default) nothing is logged.
- **Incremental runs:** a store reads the log since its last run and copies each
  changed product whole: the product, variants, images, options and category
  links. All categories are copied when one changes.
- **Full runs:** the first run, every run after the assortment changes, and
  `POST /admin/catalogue/sync?full=true` copy the whole assortment.
- **Same ids:** copies keep the catalogue's ids, so variants, images and orders
  need no mapping. A slug already used by one of the store's own rows gets the
  start of the id appended.

| | Set by the catalogue | Kept by the store |
|---|---|---|
| Product | title, slug, description, type, weight, shipping, SEO, GTIN, publication date | price, compare-at and cost price, currency, SKU, stock, active and featured flags |
| Variant | title, weight, shipping, GTIN | price, SKU, stock, active flag |
| Images, options | the whole set | — |
| Category links | links to catalogue categories | links to the store's own categories |

- **New products arrive inactive**, with the catalogue's price as a starting
  point: the store activates what it has priced.
- **Products deleted** from the catalogue, or no longer in the assortment, are
  deactivated, not deleted, because orders refer to them. So are variants
  removed from the catalogue.
- **The catalogue can deactivate** a product or variant everywhere. It cannot
  activate one in a store.

## Assortment

`PUT /admin/catalogue/assortment` replaces it:

```json
{ "all": false, "categories": ["…"], "products": ["…"] }
```

A category includes its sub-categories. The next sync copies the new assortment
and deactivates the catalogue products that left it.

## Admin API

| Route | Purpose |
|---|---|
| `GET /admin/catalogue` | Last position, last run and error, number of catalogue products, assortment |
| `POST /admin/catalogue/sync` | Run a sync now (`?full=true` for a full one); 409 while one is running |
| `PUT /admin/catalogue/assortment` | Replace the assortment |

All three answer 404 when `source_url` is not set.

## Operations

- **One sync at a time** per store database (PostgreSQL advisory lock), also
  across several API instances.
- **Overlap:** changes from the last five minutes are applied again on the next
  run, in case a slower catalogue transaction committed after them. Copies are
  idempotent.
- **Log retention:** entries older than 30 days are pruned when the catalogue
  store starts. A store that has not synced since then copies its whole
  assortment again.
