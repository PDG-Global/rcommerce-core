# Building R Commerce Binaries

This guide covers building R Commerce binaries for different platforms.

## Quick Start

### Build for Current Platform (Host)

```bash
# Build release binary
cargo build --release -p rcommerce-cli

# Binary location: target/release/rcommerce
```

### Using the Cross-Compilation Build Script

```bash
# Build for all supported platforms
./scripts/build-release.sh

# Build only macOS targets
./scripts/build-release.sh --macos-only

# Build only Linux targets (GNU and MUSL)
./scripts/build-release.sh --linux-only

# Build only static MUSL targets
./scripts/build-release.sh --musl-only

# Build only FreeBSD targets
./scripts/build-release.sh --freebsd-only

# Build specific target
./scripts/build-release.sh x86_64-unknown-linux-musl
```

## Supported Platforms

| Platform | Architecture | Target Triple | Binary Type | Notes |
|----------|--------------|---------------|-------------|-------|
| macOS | Intel (x86_64) | `x86_64-apple-darwin` | Native | ✅ Fully supported |
| macOS | Apple Silicon (ARM64) | `aarch64-apple-darwin` | Native | ✅ Fully supported |
| macOS | Universal | `universal` | Fat binary | ✅ Both archs |
| Linux | x86_64 (GNU) | `x86_64-unknown-linux-gnu` | Dynamic linking | ✅ Fully supported |
| Linux | x86_64 (MUSL) | `x86_64-unknown-linux-musl` | Static binary | ✅ Fully supported |
| Linux | ARM64 (GNU) | `aarch64-unknown-linux-gnu` | Dynamic linking | ✅ Fully supported |
| Linux | ARM64 (MUSL) | `aarch64-unknown-linux-musl` | Static binary | ✅ Fully supported |
| Linux | ARMv7 (GNU) | `armv7-unknown-linux-gnueabihf` | Dynamic linking | ✅ Fully supported |
| FreeBSD | x86_64 | `x86_64-unknown-freebsd` | Static-ish | ✅ Cross-compilation works |
| NetBSD | x86_64 | `x86_64-unknown-netbsd` | - | ⚠️ Build natively (see below) |
| OpenBSD | - | - | - | ❌ No Rust target available |
| DragonFlyBSD | - | - | - | ❌ No Rust target available |

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) 1.70.0 or later
- PostgreSQL 14+ (for runtime)

### macOS (Build Host for Cross-Compilation)

```bash
# Install Xcode command line tools
xcode-select --install

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Zig (required for Linux/FreeBSD cross-compilation)
brew install zig

# Install cargo-zigbuild (required for Linux/FreeBSD cross-compilation)
cargo install cargo-zigbuild

# Add all targets for cross-compilation
rustup target add \
  aarch64-apple-darwin \
  x86_64-apple-darwin \
  x86_64-unknown-linux-gnu \
  aarch64-unknown-linux-gnu \
  armv7-unknown-linux-gnueabihf \
  x86_64-unknown-linux-musl \
  aarch64-unknown-linux-musl \
  x86_64-unknown-freebsd
```

### Linux

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# For cross-compilation, install cargo-zigbuild
# Note: On Linux, you can also use 'cross' which uses Docker
cargo install cargo-zigbuild
```

### FreeBSD

```bash
# Install Rust
pkg install rust

# Or use rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Cross-Compilation

### Using `cargo-zigbuild` (Recommended from macOS)

`cargo-zigbuild` uses Zig as the linker, enabling cross-compilation without Docker:

```bash
# Build for Linux x86_64 (GNU)
cargo zigbuild --release --target x86_64-unknown-linux-gnu -p rcommerce-cli

# Build for Linux ARM64 (GNU)
cargo zigbuild --release --target aarch64-unknown-linux-gnu -p rcommerce-cli

# Build for Linux x86_64 (static MUSL)
cargo zigbuild --release --target x86_64-unknown-linux-musl -p rcommerce-cli

# Build for FreeBSD x86_64
cargo zigbuild --release --target x86_64-unknown-freebsd -p rcommerce-cli
```

### Using `cross` (Alternative for Linux hosts)

The `cross` tool uses Docker containers for cross-compilation:

```bash
# Install cross
cargo install cross --git https://github.com/cross-rs/cross

# Build for Linux ARM64
cross build --release --target aarch64-unknown-linux-gnu -p rcommerce-cli

# Build for Linux x86_64
cross build --release --target x86_64-unknown-linux-gnu -p rcommerce-cli
```

### Using Docker

```bash
# Build Linux binary using Docker
docker build -f Dockerfile.build --target export --output type=local,dest=./release .

# The binary will be in ./release/rcommerce
```

### macOS Cross-Compilation

macOS can cross-compile between Intel and Apple Silicon using native cargo:

```bash
# On Intel Mac, build for Apple Silicon
rustup target add aarch64-apple-darwin
cargo build --release --target aarch64-apple-darwin -p rcommerce-cli

# On Apple Silicon Mac, build for Intel
rustup target add x86_64-apple-darwin
cargo build --release --target x86_64-apple-darwin -p rcommerce-cli

# Create universal binary (fat binary)
lipo -create \
  target/x86_64-apple-darwin/release/rcommerce \
  target/aarch64-apple-darwin/release/rcommerce \
  -output target/release/rcommerce-universal
```

## Automated Builds with GitHub Actions

The repository includes a GitHub Actions workflow that automatically builds binaries for all platforms when you push a tag:

```bash
# Create a new version tag
git tag -a v0.1.0 -m "Release version 0.1.0"
git push origin v0.1.0
```

The workflow will:
1. Build for macOS (Intel and Apple Silicon)
2. Build for Linux (x86_64, ARM64, ARMv7 with both GNU and MUSL)
3. Build for FreeBSD (x86_64)
4. Create macOS universal binary
5. Create a GitHub Release with all binaries
6. Generate SHA256 checksums

## Binary Sizes

Typical release binary sizes:

| Platform | Size | Type |
|----------|------|------|
| macOS ARM64 | ~14 MB | Native |
| macOS x86_64 | ~16 MB | Native |
| macOS Universal | ~30 MB | Fat binary |
| Linux ARM64 GNU | ~13 MB | Dynamic linking |
| Linux ARM64 MUSL | ~12 MB | Static binary |
| Linux x86_64 GNU | ~15 MB | Dynamic linking |
| Linux x86_64 MUSL | ~14 MB | Static binary |
| Linux ARMv7 GNU | ~12 MB | Dynamic linking |
| FreeBSD x86_64 | ~15 MB | Static-ish |

## BSD Support Notes

### Native Build Instructions

All BSD systems should be built **natively** on the target system. Cross-compilation is not supported due to rustls/aws-lc-sys dependencies.

#### FreeBSD ✅

```bash
# Install dependencies
pkg install rust postgresql15-client git

# Clone and build
git clone https://github.com/creativebastard/rcommerce.git
cd rcommerce
cargo build --release -p rcommerce-cli

# Binary location: target/release/rcommerce
```

#### NetBSD ✅

```bash
# Install dependencies (using pkgin)
pkgin install rust postgresql15-client git

# Or from pkgsrc
cd /usr/pkgsrc/databases/postgresql15-client && make install

# Clone and build
git clone https://github.com/creativebastard/rcommerce.git
cd rcommerce
cargo build --release -p rcommerce-cli
```

#### OpenBSD ✅

```bash
# Install dependencies
pkg_add rust postgresql-client git

# Clone and build
git clone https://github.com/creativebastard/rcommerce.git
cd rcommerce
cargo build --release -p rcommerce-cli
```

#### DragonFlyBSD ✅

```bash
# Install dependencies
pkg install rust postgresql15-client git

# Clone and build
git clone https://github.com/creativebastard/rcommerce.git
cd rcommerce
cargo build --release -p rcommerce-cli
```

### Cross-Compilation Status

| BSD | Cross-Compile | Native Build | Notes |
|-----|---------------|--------------|-------|
| **FreeBSD** x86_64 | ⚠️ Experimental | ✅ Recommended | cargo-zigbuild may work |
| **NetBSD** x86_64 | ❌ Not supported | ✅ Supported | aws-lc-sys limitation |
| **OpenBSD** x86_64 | ❌ Not supported | ✅ Supported | No Rust target for cross-compile |
| **DragonFlyBSD** x86_64 | ❌ Not supported | ✅ Supported | No Rust target for cross-compile |

**Why no cross-compilation?**
- rustls → aws-lc-rs → aws-lc-sys dependency chain
- aws-lc-sys contains x86_64 assembly code that doesn't cross-compile to BSDs
- OpenBSD and DragonFlyBSD lack official Rust targets for cross-compilation

## Verification

After building, verify the binary:

```bash
# Check binary info
file rcommerce

# Check dynamic libraries (macOS/Linux)
ldd rcommerce        # Linux
otool -L rcommerce   # macOS

# Test the binary
./rcommerce --version
./rcommerce server --help
```

## Troubleshooting

### "linker 'cc' not found"

Install a C compiler:
- macOS: `xcode-select --install`
- Ubuntu/Debian: `sudo apt-get install build-essential`
- FreeBSD: `pkg install gcc`

### "target may not be installed"

Add the target to Rust:
```bash
rustup target add <target-triple>
```

### Cross-compilation failures with cargo-zigbuild

1. Ensure Zig is installed: `brew install zig`
2. Ensure cargo-zigbuild is installed: `cargo install cargo-zigbuild`
3. Set `SQLX_OFFLINE=true` for builds without database: `export SQLX_OFFLINE=true`

### Binary won't run on target system

**GNU targets**: The binary is dynamically linked and requires glibc. Use MUSL targets for fully static binaries.

**MUSL targets**: Fully static, should run on any Linux system.

## Advanced: Static Linking

For fully static Linux binaries, use the MUSL targets:

```bash
# Install musl target
rustup target add x86_64-unknown-linux-musl

# Build with musl (static linking) using cargo-zigbuild
cargo zigbuild --release --target x86_64-unknown-linux-musl -p rcommerce-cli
```

## Build Configuration

### TLS Implementation

R Commerce uses **rustls** (pure Rust TLS implementation) instead of OpenSSL for easier cross-compilation. The following crates are configured:

- `reqwest` - Uses `rustls-tls` feature
- `sqlx` - Uses `runtime-tokio-rustls` feature
- `lettre` - Uses `tokio1-rustls-tls` feature
- `redis` - Uses `tokio-rustls-comp` feature

This avoids OpenSSL/BoringSSL linking issues during cross-compilation.

### Environment Variables

```bash
# For builds without database connection
export SQLX_OFFLINE=true

# For verbose build output
export RUST_LOG=debug

# For faster builds (disable LTO)
export CARGO_PROFILE_RELEASE_LTO=false
```

## Distribution

### Creating a Release Archive

```bash
VERSION="0.1.0"
PLATFORM="x86_64-linux"

tar czf rcommerce-${VERSION}-${PLATFORM}.tar.gz \
    rcommerce \
    README.md \
    LICENSE \
    config/
```

### Docker Image

```bash
# Build Docker image
docker build -t rcommerce:latest .

# Run container
docker run -p 8080:8080 -e DATABASE_URL=postgres://... rcommerce:latest
```

## Next Steps

After building:
1. Test the binary: `./rcommerce server`
2. Set up configuration (see [Configuration Guide](CONFIGURATION.md))
3. Deploy to your infrastructure (see [Deployment Guide](DEPLOYMENT.md))
