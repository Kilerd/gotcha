# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Gotcha is an enhanced web framework built on top of Axum, providing additional features for building web applications in Rust. It's structured as a Rust workspace with multiple crates:

- `gotcha/` - Main framework crate with modular features
- `gotcha_macro/` - Procedural macros for OpenAPI generation and route handling
- `examples/` - Various example applications demonstrating different features

## Commands

### Building and Testing
```bash
# Build the main gotcha crate
cargo build --package gotcha

# Run tests for the main gotcha crate
cargo test --package gotcha

# Run tests with specific features (example)
cargo test --package gotcha --features "openapi prometheus cors"

# Test feature combinations (comprehensive testing)
python3 test-feature-matrix.py

# Test Cloudflare Worker features specifically
python3 test-feature-matrix.py echo-cf-worker

# Build and run examples
cargo run --package basic
cargo run --package openapi
cargo run --package task
```

### Development Commands
```bash
# Check code formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy linter
cargo clippy

# Run clippy on all targets
cargo clippy --all-targets

# Build documentation
cargo doc --open

# Build with all features enabled
cargo build --all-features
```

## Architecture

### Core Framework Structure
- **GotchaApp trait**: Main application interface that defines routes, state, configuration, and optional tasks
- **GotchaRouter**: Router wrapper that extends Axum's router with OpenAPI operation tracking
- **GotchaContext**: Application context combining configuration and state
- **ConfigWrapper**: Configuration management with environment-based profiles

### Feature-Based Architecture
The framework uses Cargo features for modular functionality:
- `openapi` - Automatic OpenAPI documentation generation via `oas` crate
- `prometheus` - Metrics integration with `axum-prometheus`
- `cors` - CORS support via `tower-http`
- `static_files` - Static file serving capabilities
- `task` - Background task scheduling with cron support
- `message` - Built-in message passing system
- `cloudflare_worker` - Cloudflare Worker runtime support

### Configuration System
- Uses `mofa` crate for advanced configuration loading
- Supports environment variable resolution (`${ENV_VAR}`)
- Supports path variable resolution (`${app.database.name}`)
- Profile-based configuration via `GOTCHA_ACTIVE_PROFILE` environment variable
- Configuration files located in `configurations/` directory:
  - `application.toml` - Base configuration
  - `application_{profile}.toml` - Profile-specific overrides

### Macro System
The `gotcha_macro` crate provides:
- `#[api]` attribute for automatic OpenAPI schema generation
- `#[derive(Schematic)]` for parameter schema generation
- Route handler macros with type safety

### OpenAPI Integration
When `openapi` feature is enabled:
- Automatic schema generation from Rust types
- Built-in endpoints: `/openapi.json`, `/redoc`, `/scalar`
- Tagged union support with discriminator fields
- Complex type mapping including generics and HashMaps

### Examples Architecture
Each example demonstrates specific features:
- `basic/` - Minimal web server setup
- `openapi/` - OpenAPI documentation generation
- `configuration/` - Environment-based configuration
- `task/` - Background task scheduling
- `message/` - Message passing system
- `cloudflare-worker/` - Cloudflare Worker deployment

## Development Guidelines

### Testing Strategy
- Use `python3 test-feature-matrix.py` to test all feature combinations
- Feature combinations are automatically generated and tested
- Cloudflare Worker features require separate testing with `echo-cf-worker` option
- Tests are located in `gotcha/tests/pass/` directory

### Configuration Management  
- Configuration files must be in `configurations/` directory
- Use TOML format with `[basic]` section for server settings and `[application]` for custom config
- Environment variables can be injected using `${VAR_NAME}` syntax
- Profile switching via `GOTCHA_ACTIVE_PROFILE` environment variable

### OpenAPI Development
- Use `#[api]` macro on handler functions for automatic schema generation
- Implement `Schematic` derive on request/response types
- Test generated schemas against expected JSON in `tests/pass/openapi/`
- OpenAPI endpoints are automatically mounted when feature is enabled