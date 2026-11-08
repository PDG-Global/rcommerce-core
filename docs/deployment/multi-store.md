# Several stores in one process

One rCommerce process can serve several independent stores, such as the members of a retail network. Each store has:

- its own **database**: catalogue, customers, orders, staff and API keys never mix;
- its own **token secret and audience**: a token from one store is refused by every other;
- its own **upload directory** and **payment credentials**;
- its own **host names**: a request reaches the store serving its `Host`.

With no `[stores]` table, rCommerce runs as a single store, exactly as before.

## Configuration

The base configuration holds what the stores share (server, database host and credentials, defaults). Each `[stores.<name>]` table describes one store and overrides any section it needs; tables are merged key by key.

```toml
[database]
host = "postgres"
username = "rcommerce"
password = "..."

[security.jwt]
secret = "used only when there are no stores"

[stores.shop-a]
hostnames = ["shop-a.example", "www.shop-a.example"]

[stores.shop-a.database]
database = "rc_shop_a"

[stores.shop-a.security.jwt]
secret = "a-different-secret-of-at-least-32-bytes"

[stores.shop-a.payment.stripe]
secret_key = "sk_live_..."

[stores.network]
hostnames = ["catalogue.example"]
role = "catalogue"

[stores.network.database]
database = "rc_network"

[stores.network.security.jwt]
secret = "yet-another-secret-of-at-least-32-bytes"
```

- **Name** (`shop-a`): lower-case letters and digits separated by hyphens.
- **`hostnames`**: required. Ports and a trailing dot are ignored; matching is case-insensitive.
- **`role`**: `merchant` (default) or `catalogue`, a store that publishes its catalogue to the others.

### Isolation rules

The server refuses to start when two stores share:

- a host name;
- a database (same host, port and database name);
- a `security.jwt.secret`.

Unless a store sets them itself, rCommerce gives each store:

| Setting | Default per store |
|---|---|
| `security.jwt.audience` | `rcommerce-api:<name>` |
| `media.local_path` | `<base path>/<name>` |
| `database.pool_size` | the base size, capped at 10 |

**No environment fallbacks.** A single store may take some secrets and URLs from environment variables (`STRIPE_API_KEY`, `STOREFRONT_URL`, `REDIS_URL`…). A store of a multi-store process never does: each store states its credentials in its own table, so one store can never use another's.

## Database connections

Every store has its own connection pool. Keep the sum of all `database.pool_size` values below PostgreSQL's `max_connections`, minus a reserve for maintenance (the PostgreSQL default is 100). Beyond about ten stores, put PgBouncer (session mode) in front of PostgreSQL.

## Migrations

`rcommerce server` migrates every store's database before starting. Create each database first (`CREATE DATABASE rc_shop_a;`).

## Behind a reverse proxy

Serve HTTP and terminate TLS at a reverse proxy (Caddy, nginx, a CDN), which holds the certificates for every host name. `tls.enabled = true` is refused when stores are configured.

The proxy must pass the original `Host` header (nginx: `proxy_set_header Host $host;`). If it can only set `X-Forwarded-Host`, enable:

```toml
[server]
trust_forwarded_host = true
```

Only do this when clients cannot reach rCommerce directly: clients control that header.

Per-IP rate limiting (`[rate_limiting]`) applies once across all stores. Configure it in the base configuration.
