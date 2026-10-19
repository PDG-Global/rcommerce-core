# R Commerce Demo Frontend

A lightweight, vanilla HTML/JavaScript frontend for demonstrating R Commerce functionality.

## Two Ways to Run

### Option 1: Demo Server (Recommended) 🔒

The **demo server** is a Rust binary that serves the frontend and proxies API requests. This keeps your API key secure (never exposed to the browser).

```bash
# 1. Build the demo server
cargo build --release -p rcommerce-demo-server

# 2. Create an API key
rcommerce api-key create --name "Demo" --scopes "products:read,orders:write,carts:write"

# 3. Run the demo server
./target/release/rcommerce-demo \
  --api-url http://localhost:8080 \
  --api-key ak_yourprefix.yoursecret

# 4. Open http://localhost:3000
```

**Benefits:**
- ✅ API key hidden from browser
- ✅ No CORS issues
- ✅ Single binary deployment
- ✅ Works behind Caddy/Nginx

See [Demo Server README](../crates/rcommerce-demo-server/README.md) for full documentation.

---

### Option 2: Static Files (Development Only) ⚠️

Serve the HTML/JS files directly with any static file server. **Warning: API key will be exposed in browser!**

```bash
# Using Python
python -m http.server 3000

# Using PHP
php -S localhost:3000

# Using Node.js
npx serve .
```

**Configure API key:** Edit `api.js` and set your API key:
```javascript
const API_KEY = 'ak_yourprefix.yoursecret';  // ⚠️ Exposed to browser!
```

---

## Features

- Product listing with demo products
- Product detail pages
- Shopping cart (localStorage for guests, API for logged-in users)
- Checkout flow
- Customer authentication (login/register)
- Responsive design

## Demo Products

1. **Premium Wireless Headphones** - $299.99
2. **Smart Watch Pro** - $399.99
3. **Portable Bluetooth Speaker** - $149.99

## Architecture

### With Demo Server (Secure)
```
Browser ──▶ Demo Server ──▶ R Commerce API
            │                    │
            │ API key injected   │
            │ (server-side only) │
```

Uses `api-proxy.js` - makes relative requests to `/api/*`

### Static Files (Insecure)
```
Browser ──────────────▶ R Commerce API
   │                          │
   │ API key in JS            │
   │ (visible to users)       │
```

Uses `api.js` - makes direct requests to API with exposed key

## File Structure

```
demo-frontend/
├── index.html          # Product listing
├── product.html        # Product detail
├── cart.html           # Shopping cart
├── checkout.html       # Checkout flow
├── confirmation.html   # Order confirmation
├── styles.css          # Styles
├── api.js              # Direct API client (static files mode)
├── api-proxy.js        # Proxy API client (demo server mode)
├── app.js              # Main application logic
├── auth.js             # Authentication handling
└── checkout.js         # Checkout logic
```

## Browser Compatibility

- Chrome/Edge (latest)
- Firefox (latest)
- Safari (latest)

## Security Warning

**Never use the static files mode (`api.js`) in production!** The API key will be visible to anyone who views the page source. Always use the demo server or implement your own backend proxy.

## License

AGPL-3.0
