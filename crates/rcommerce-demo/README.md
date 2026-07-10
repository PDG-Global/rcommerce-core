# R Commerce Demo Server

A standalone CLI tool that serves the R Commerce demo frontend with secure API proxying. This tool keeps your API keys safe by handling them server-side, never exposing them to the browser.

## Features

- 🔒 **Secure API Proxy** - API keys are handled server-side, never exposed to JavaScript
- 📁 **Static File Serving** - Serves the demo frontend HTML/CSS/JS files
- ⚙️ **Configurable** - Via CLI args, environment variables, or config file
- 🎨 **Custom Frontend Support** - Use your own frontend files if desired
- 🚀 **Easy Testing** - Quick way to test your R Commerce installation

## Installation

```bash
# Build from source
cargo build --release -p rcommerce-demo

# The binary will be at:
./target/release/rcommerce-demo
```

## Usage

### Quick Start

```bash
# Start with default settings (connects to localhost:8080)
rcommerce-demo

# Specify API URL and key
rcommerce-demo --api-url http://localhost:8080 --api-key ak_yourprefix.yoursecret

# Or use environment variables
export RCOMMERCE_API_URL=http://localhost:8080
export RCOMMERCE_API_KEY=ak_yourprefix.yoursecret
rcommerce-demo
```

### Getting an API Key

1. Start your R Commerce server
2. Create an API key using the CLI:
   ```bash
   rcommerce api-key create --name "Demo Frontend" --scopes "products:read,orders:write,carts:write,customers:write"
   ```
3. Copy the generated key for use with `rcommerce-demo`

### Command Line Options

```
rcommerce-demo [OPTIONS]

Options:
  -a, --api-url <API_URL>      R Commerce API base URL [env: RCOMMERCE_API_URL=] [default: http://localhost:8080]
  -k, --api-key <API_KEY>      API Key for service-to-service authentication [env: RCOMMERCE_API_KEY=]
  -c, --config <CONFIG>        Configuration file path
  -H, --host <HOST>            Host to bind the server to [default: 127.0.0.1]
  -P, --port <PORT>            Port to listen on [default: 3000]
  -f, --frontend-dir <DIR>     Directory containing custom frontend files
      --no-proxy               Disable API proxy (serve frontend only)
  -l, --log-level <LEVEL>      Log level [default: info]
  -h, --help                   Print help
  -V, --version                Print version
```

### Configuration File

Create a `demo-config.toml`:

```toml
api_url = "http://localhost:8080"
api_key = "ak_yourprefix.yoursecret"
host = "127.0.0.1"
port = 3000
no_proxy = false
```

Then run:
```bash
rcommerce-demo -c demo-config.toml
```

### Using Custom Frontend Files

You can override the default frontend with your own files:

```bash
rcommerce-demo --frontend-dir /path/to/custom/frontend
```

Required files in the directory:
- `index.html` - Homepage with product listing
- `product.html` - Product detail page
- `cart.html` - Shopping cart page
- `checkout.html` - Checkout page
- `confirmation.html` - Order confirmation page
- `styles.css` - Stylesheet
- `app.js` - Main application logic
- `api.js` - API client (will be modified for proxy mode)
- `auth.js` - Authentication handling
- `checkout.js` - Checkout logic
- `checkout_v2.js` - Alternative checkout implementation

## How It Works

### Security Model

```
┌─────────────────┐      ┌──────────────────┐      ┌─────────────────┐
│   Browser       │──────▶│  rcommerce-demo  │──────▶│  R Commerce     │
│   (JavaScript)  │       │  (Proxy Server)  │      │  API Server     │
└─────────────────┘       └──────────────────┘      └─────────────────┘
       │                           │                         │
       │  No API key               │  API key added          │
       │  in browser               │  server-side            │
```

1. Browser makes API requests to `http://localhost:3000/api/v1/...`
2. `rcommerce-demo` receives the request and adds the API key
3. Request is forwarded to the R Commerce backend
4. Response is returned to the browser

### API Key Handling

- The API key is **never** sent to the browser
- JavaScript uses relative URLs (`/api/v1/products`)
- The proxy adds the `Authorization: Bearer <api_key>` header
- This prevents API key theft via browser DevTools

### JavaScript Configuration

The server injects a configuration object into each HTML page:

```javascript
window.RCOMMERCE_CONFIG = {
    API_BASE_URL: '/api/v1',  // Relative URL (proxied)
    PROXY_ENABLED: true,
    API_URL: 'http://localhost:8080'  // Actual backend (for info)
};
```

The `api.js` file is automatically modified to:
- Use `window.RCOMMERCE_CONFIG.API_BASE_URL` instead of hardcoded URL
- Remove the API key constant (handled by proxy)

## Pages

The demo frontend includes:

- **/** (Home) - Product listing
- **/product** - Product detail page
- **/cart** - Shopping cart
- **/checkout** - Checkout flow
- **/confirmation** - Order confirmation

## Development

### Building

```bash
# Debug build
cargo build -p rcommerce-demo

# Release build
cargo build --release -p rcommerce-demo
```

### Running Tests

```bash
cargo test -p rcommerce-demo
```

## Troubleshooting

### "API proxy is disabled" error

You started with `--no-proxy` flag. Remove it to enable API proxying:
```bash
rcommerce-demo --api-url http://localhost:8080 --api-key <your-key>
```

### "Failed to connect to backend" error

The R Commerce server is not running or not accessible:
```bash
# Start the R Commerce server first
rcommerce server

# Then in another terminal
rcommerce-demo
```

### "API key required" error

You need to create an API key:
```bash
rcommerce api-key create --name "Demo" --scopes "products:read,orders:write,carts:write"
```

### CORS errors in browser

The proxy handles CORS automatically. If you see CORS errors, ensure you're accessing the demo through the proxy URL (`http://localhost:3000`) not directly from the file system.

## License

MIT - See LICENSE file for details
