# rCommerce Storefront

A fresh Astro + Cloudflare Pages demo storefront for the rCommerce headless e-commerce platform.

## Features

- Product catalog (home + product listing + product detail)
- Guest shopping cart with drawer
- Guest checkout with shipping selection
- Cloudflare Pages API proxy to rCommerce backend
- Responsive Tailwind CSS design

## Development

```bash
npm install
npm run dev
```

Set the backend URL in `wrangler.toml` or via environment variables:

```toml
[vars]
RCOMMERCE_API_URL = "http://localhost:8080"
RCOMMERCE_API_KEY = "your-api-key"
```

## Build

```bash
npm run build
```

## Deploy

```bash
npm run deploy
```

## Backend Requirements

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
