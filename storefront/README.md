# rCommerce Storefront

A whitelabel **Astro + React** storefront for the rCommerce headless platform, designed to be deployed to **Cloudflare Pages** (SSR via the `@astrojs/cloudflare` adapter). The same codebase serves multiple stores: branding is injected at build time through `PUBLIC_*` environment variables and optional store-private overlay files.

## Features

- Product catalog (home, category listing, product detail with variants)
- Product reviews (submit + display, feeds Google review snippets)
- Guest shopping cart with drawer + persistent guest carts
- Guest and account checkout with the **Airwallex Drop-in** (cards, Apple Pay, Google Pay, WeChat Pay, Alipay)
- Customer accounts: addresses, order history, order detail with status
- Coupon support at checkout
- SEO built in: `sitemap.xml`, `robots.txt`, Google Shopping feed (`google-feed.xml`), Google reviews feed (`google-reviews.xml`), product JSON-LD (`Product`, `Offer`, `aggregateRating`)
- Responsive Tailwind CSS design, brand-themed

## How branding works

Branding is resolved **at build time** (Astro inlines it — changing an env var requires a rebuild):

1. **Environment variables** (`PUBLIC_*`) — name, logo, colors, API base, menu, hero media. See the table below.
2. **Store-private overlays** (optional) — files in `src/lib/brand/private/` that are *not* part of the public template:
   - `<slug>.ts` — exports `content` (deep-merged over the generic defaults in `src/lib/brand/default.ts`: copy, nav, footer, SEO shipping/returns data, homepage, hidden categories, redirects) and optionally `configDefaults` (logo path, theme class, favicons, layout flags).
   - `<slug>-<page>.astro` — bespoke full-page replacements for generic routes (e.g. a custom home, about, or contact page).
3. **The generic template** (this public repo) ships only the defaults — with no overlays and no env vars it builds the neutral "rCommerce Store" demo.

The active store is selected by `PUBLIC_BRAND_SLUG`; without it the build uses the `default` brand. Explicit `PUBLIC_*` env vars always win over overlay `configDefaults` for the scalar fields.

## Environment variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `PUBLIC_BRAND_SLUG` | Store key; selects the overlay module | `my-store` |
| `PUBLIC_BRAND_NAME` | Store name | `My Store` |
| `PUBLIC_BRAND_LOGO` | Logo asset path (serve from `public/`) | `/images/logo.svg` |
| `PUBLIC_BRAND_PRIMARY` | Primary brand color | `#C1481A` |
| `PUBLIC_API_BASE` | rCommerce API base, inlined into the client bundle | `https://api.example.com/api/v1` |
| `PUBLIC_BRAND_MENU` | Comma-separated `slug:Label` category menu | `wheels:Wheels,bushings:Bushings` |
| `PUBLIC_HERO_VIDEO_URL` | Optional homepage hero video (Cloudflare Stream iframe src) | `https://customer-xxx.cloudflarestream.com/...` |
| `PUBLIC_HERO_PRODUCT_SLUG` | Optional featured product in the hero panel | `big-wheel-110mm` |
| `PUBLIC_AIRWALLEX_ENV` | Fallback Airwallex environment for checkout (`demo`/`prod`). The backend's checkout response takes precedence at runtime. | `prod` |
| `RCOMMERCE_API_URL` | Origin of the rCommerce API (used by the Pages worker proxy and SSR fetches) | `https://api.example.com` |
| `RCOMMERCE_API_KEY` | Optional API key for server-side fetches | — |

## Development

```bash
npm ci
npm run dev
```

Set the backend URL and brand via environment variables or `wrangler.toml`:

```toml
name = "my-storefront"
compatibility_date = "2025-06-01"
compatibility_flags = ["nodejs_compat"]
pages_build_output_dir = "dist"

[vars]
RCOMMERCE_API_URL = "https://api.example.com"
PUBLIC_API_BASE = "https://api.example.com/api/v1"
PUBLIC_AIRWALLEX_ENV = "prod"
```

## Build

```bash
npm run build
```

The output in `dist/` is an SSR Cloudflare Pages bundle (`_worker.js`) — there is no static `index.html`.

## Deploy to Cloudflare Pages

```bash
npm run build
npx wrangler pages deploy dist --project-name=my-storefront --branch=main
```

Or configure the project in the Cloudflare dashboard with build command `npm run build` and output dir `dist`. Set the `PUBLIC_*` variables in the Pages project's build environment.

### Running multiple stores

Create **one Pages project per store** and give each its own `PUBLIC_*` values (or its own private overlay + `PUBLIC_BRAND_SLUG`). All stores can share a single rCommerce backend or use one backend each — `PUBLIC_API_BASE`/`RCOMMERCE_API_URL` point the storefront at its API.

## SEO endpoints

Generated per build from the catalog and the brand content overlay:

- `/sitemap.xml` — pages, categories, products (driven by the overlay's nav/footer/redirect config)
- `/robots.txt`
- `/google-feed.xml` — Google Shopping / Merchant Center product feed
- `/google-reviews.xml` — review feed

The product JSON-LD `Offer` blocks (shipping destinations, return policy, price currency) come from `content.seo` in the brand overlay — update it there to match your shipping rates and returns terms.

## Backend requirements

This storefront expects the rCommerce API to expose:

- `GET /api/v1/products`
- `GET /api/v1/products/:id`
- `POST /api/v1/carts/guest`
- `GET /api/v1/carts/:id`
- `POST /api/v1/carts/:id/items`
- `PUT /api/v1/carts/:id/items/:item_id`
- `DELETE /api/v1/carts/:id/items/:item_id`
- `POST /api/v1/checkout/initiate`
- `POST /api/v1/checkout/shipping`
- `POST /api/v1/checkout/complete`
