# Contributing to R Commerce

Thank you for your interest in contributing to R Commerce! This document
provides guidelines for contributing to the project.

## Quick Start

1. **Fork** the repository
2. **Clone** your fork: `git clone https://github.com/yourusername/rcommerce.git`
3. **Create** a branch: `git checkout -b feature/your-feature`
4. **Make** your changes
5. **Test** your changes: `cargo test --workspace`
6. **Commit** with a clear message
7. **Push** to your fork
8. **Submit** a pull request

## Prerequisites

- Rust 1.70.0 or later
- PostgreSQL 14+
- Git

## Development Setup

```bash
# Clone the repository
git clone https://github.com/pdgglobal/rcommerce.git
cd rcommerce

# Install dependencies
cargo build --workspace

# Run tests
cargo test --workspace

# Run the API server
cargo run -p rcommerce-api
```

## Contributor License Agreement

Before we can accept your contributions, you must sign our
[Contributor License Agreement (CLA)](CLA.md).

The CLA ensures that:
- You retain copyright to your contributions
- We can license your contributions under both AGPL-3.0 and Commercial licenses
- The project remains legally sound for all users

### Signing the CLA

The CLA is automatically enforced via [cla-assistant.io](https://cla-assistant.io).
When you submit your first pull request, a bot will ask you to sign the CLA.

## Code Standards

### Rust Style

- Follow the [Rust Style Guide](https://doc.rust-lang.org/style-guide/)
- Run `cargo fmt` before committing
- Run `cargo clippy` and fix any warnings
- Keep functions focused and under 50 lines when possible

### Documentation

- Add doc comments (`///`) for public APIs
- Update relevant documentation in `docs/`
- Include examples in doc comments where helpful

### Testing

- Write unit tests for new functionality
- Ensure all tests pass: `cargo test --workspace`
- Aim for good test coverage

### Commit Messages

Use clear, descriptive commit messages:

```
feat: add cart merge functionality

- Implement cart merging for guest to customer conversion
- Add database transaction for atomicity
- Include integration tests

Fixes #123
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting changes
- `refactor`: Code restructuring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

## Pull Request Process

1. **Update documentation** if your changes affect the API or usage
2. **Add tests** for new functionality
3. **Ensure CI passes** (tests, formatting, clippy)
4. **Request review** from maintainers
5. **Address feedback** promptly

## Areas for Contribution

We welcome contributions in these areas:

### High Priority

- Payment gateway integrations (PayPal, Square, Adyen)
- Additional database support (MongoDB, DynamoDB)
- Performance optimizations
- Security enhancements

### Documentation

- API documentation improvements
- Tutorial content
- Translation to other languages

### Testing

- Additional test coverage
- Performance benchmarks
- Integration tests for edge cases

### Features

- Multi-currency support enhancements
- Advanced inventory management
- Marketing automation integrations
- Analytics and reporting

## Code Review

All submissions require review before merging. Reviewers will check:

- Code quality and style
- Test coverage
- Documentation completeness
- Backward compatibility
- Security implications

## Community

- **Discussions**: Use GitHub Discussions for questions
- **Issues**: Report bugs and request features via GitHub Issues
- **Discord**: Join our community chat (coming soon)

## License

By contributing, you agree that your contributions will be licensed under the
same dual-license as the project (AGPL-3.0 and Commercial).

## Questions?

- Email: dev@rcommerce.app
- GitHub Issues: [github.com/pdgglobal/rcommerce/issues](https://github.com/pdgglobal/rcommerce/issues)

---

Thank you for contributing to R Commerce!
