# Development Documentation

This section contains resources for developers working with or contributing to R Commerce.

## Developer Resources

| Document | Description |
|----------|-------------|
| [developer-guide.md](developer-guide.md) | Complete development setup guide |
| [development-roadmap.md](development-roadmap.md) | Project roadmap and timeline |
| [cli-reference.md](cli-reference.md) | Complete CLI command reference |
| [configuration-reference.md](configuration-reference.md) | Configuration options |
| [contributing.md](../CONTRIBUTING.md) | Contribution guidelines |

## Quick Start for Developers

### 1. Clone Repository

```bash
git clone https://github.com/creativebastard/rcommerce.git
cd rcommerce
```

### 2. Install Dependencies

```bash
# Rust (latest stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# PostgreSQL
# macOS
brew install postgresql

# Ubuntu/Debian
sudo apt-get install postgresql
```

### 3. Setup Database

```bash
# Create database
psql -U postgres -c "CREATE DATABASE rcommerce;"
psql -U postgres -c "CREATE USER rcommerce WITH PASSWORD 'password';"
psql -U postgres -c "GRANT ALL PRIVILEGES ON DATABASE rcommerce TO rcommerce;"
```

### 4. Configure

```bash
cp config.example.toml config.toml
# Edit config.toml with your settings
```

### 5. Run

```bash
cargo run --bin rcommerce -- server
```

## Testing

```bash
# Run all tests
cargo test --workspace

# Run with output
cargo test -- --nocapture

# Check formatting
cargo fmt --check

# Run linter
cargo clippy
```

## Project Structure

```
rcommerce/
├── crates/
│   ├── rcommerce-core/     # Core library
│   ├── rcommerce-api/      # HTTP API server
│   └── rcommerce-cli/      # CLI tool
├── docs/                    # Documentation
├── migrations/              # Database migrations
└── scripts/                 # Utility scripts
```

## Related Documentation

- [Architecture](../architecture/index.md) - System architecture
- [API Docs](../api/index.md) - API reference
- [Deployment](../deployment/index.md) - Deployment guides
