# Third-Party Licenses

R Commerce is built on top of many excellent open source projects. This
document lists the third-party software included in or used by R Commerce.

## Core Dependencies

### Rust Standard Library
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: The Rust Project Developers
- **Repository**: https://github.com/rust-lang/rust

### Tokio
- **License**: MIT
- **Copyright**: Tokio Contributors
- **Repository**: https://github.com/tokio-rs/tokio

### Axum
- **License**: MIT
- **Copyright**: Axum Contributors
- **Repository**: https://github.com/tokio-rs/axum

### SQLx
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: SQLx Contributors
- **Repository**: https://github.com/launchbadge/sqlx

### Serde
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: Serde Contributors
- **Repository**: https://github.com/serde-rs/serde

## Database

### PostgreSQL (client)
- **License**: MIT
- **Copyright**: Steven Fackler
- **Repository**: https://github.com/sfackler/rust-postgres

### MySQL (client)
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: blackbeam
- **Repository**: https://github.com/blackbeam/rust-mysql-simple

### SQLite (client)
- **License**: MIT
- **Copyright**: The rusqlite authors
- **Repository**: https://github.com/rusqlite/rusqlite

## Authentication & Security

### jsonwebtoken
- **License**: MIT
- **Copyright**: Keats
- **Repository**: https://github.com/Keats/jsonwebtoken

### bcrypt
- **License**: MIT
- **Copyright**: Vincent Prouillet
- **Repository**: https://github.com/Keats/rust-bcrypt

### ring
- **License**: ISC-style
- **Copyright**: Brian Smith
- **Repository**: https://github.com/briansmith/ring

## Serialization & Validation

### serde_json
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: Serde Contributors
- **Repository**: https://github.com/serde-rs/json

### validator
- **License**: MIT
- **Copyright**: Keats
- **Repository**: https://github.com/Keats/validator

### chrono
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: chrono Contributors
- **Repository**: https://github.com/chronotope/chrono

### uuid
- **License**: Apache-2.0
- **Copyright**: uuid Contributors
- **Repository**: https://github.com/uuid-rs/uuid

## HTTP & Networking

### reqwest
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: reqwest Contributors
- **Repository**: https://github.com/seanmonstar/reqwest

### hyper
- **License**: MIT
- **Copyright**: hyper Contributors
- **Repository**: https://github.com/hyperium/hyper

### tower
- **License**: MIT
- **Copyright**: Tower Contributors
- **Repository**: https://github.com/tower-rs/tower

## Caching

### redis
- **License**: BSD-3-Clause
- **Copyright**: redis-rs Contributors
- **Repository**: https://github.com/redis-rs/redis-rs

### dashmap
- **License**: MIT
- **Copyright**: Jon Gjengset
- **Repository**: https://github.com/xacrimon/dashmap

## Financial

### rust_decimal
- **License**: MIT
- **Copyright**: Paul Mason
- **Repository**: https://github.com/paupino/rust-decimal

## Utilities

### anyhow
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: anyhow Contributors
- **Repository**: https://github.com/dtolnay/anyhow

### thiserror
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: David Tolnay
- **Repository**: https://github.com/dtolnay/thiserror

### tracing
- **License**: MIT
- **Copyright**: Tokio Contributors
- **Repository**: https://github.com/tokio-rs/tracing

### config
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: Ryan Leckey
- **Repository**: https://github.com/mehcode/config-rs

### dotenvy
- **License**: MIT
- **Copyright**: dotenvy Contributors
- **Repository**: https://github.com/allan2/dotenvy

### lazy_static
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: lazy_static Contributors
- **Repository**: https://github.com/rust-lang-nursery/lazy-static.rs

### async-trait
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: David Tolnay
- **Repository**: https://github.com/dtolnay/async-trait

### futures
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: futures-rs Contributors
- **Repository**: https://github.com/rust-lang/futures-rs

## CLI

### clap
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: clap Contributors
- **Repository**: https://github.com/clap-rs/clap

## Testing

### mockall
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: mockall Contributors
- **Repository**: https://github.com/asomers/mockall

### wiremock
- **License**: MIT/Apache-2.0 dual license
- **Copyright**: Luca Palmieri
- **Repository**: https://github.com/LukeMathWalker/wiremock-rs

## Documentation

### MkDocs
- **License**: BSD-2-Clause
- **Copyright**: Tom Christie
- **Repository**: https://github.com/mkdocs/mkdocs

### Material for MkDocs
- **License**: MIT
- **Copyright**: Martin Donath
- **Repository**: https://github.com/squidfunk/mkdocs-material

---

## Full License Texts

Full texts of the licenses mentioned above can be found at:

- **MIT License**: https://opensource.org/licenses/MIT
- **Apache License 2.0**: https://www.apache.org/licenses/LICENSE-2.0
- **BSD-2-Clause**: https://opensource.org/licenses/BSD-2-Clause
- **BSD-3-Clause**: https://opensource.org/licenses/BSD-3-Clause
- **ISC License**: https://opensource.org/licenses/ISC

## Updates

This list is updated periodically. For the most current list of dependencies,
please refer to `Cargo.lock` in the project root.

## Questions

If you have questions about third-party licenses, please contact:
- Email: legal@rcommerce.app
