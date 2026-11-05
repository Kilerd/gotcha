# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Gotcha is an enhanced web framework built on top of Axum, providing additional features for building web applications in Rust. It's structured as a Rust workspace with multiple crates:

- `gotcha/` - Main framework crate with modular features
- `gotcha_core/` - `Schematic`, parameter providers, and response descriptors
- `gotcha_macro/` - Procedural macros: `#[api]`, `#[derive(Schematic)]`, `#[config]`, `#[state]`
- `examples/` - Example applications demonstrating different features (`publish = false`)

The three library crates are versioned and released together and declare `rust-version = "1.89"`.

## Commands

### Building and Testing
```bash
# Build the main gotcha crate
cargo build --package gotcha

# Run tests for the main gotcha crate. Use --all-features: the default features exclude
# `openapi`, so about half of the integration tests and the trybuild pass fixtures do not compile
# without it and a default run passes without them
cargo test --package gotcha --all-features

# Run tests with specific features (example)
cargo test --package gotcha --features "openapi prometheus cors"

# Without default features, as CI does
cargo test --package gotcha --no-default-features

# The other crates
cargo test -p gotcha_core --all-features
cargo test -p gotcha_macro --all-features

# Test feature combinations (comprehensive testing)
python3 test-feature-matrix.py

# Standalone consumers of the macros, outside workspace feature unification
python3 test-downstream.py

# Build and run examples
cargo run --package basic
cargo run --package openapi
cargo run --package task
```

`#[api]`, `ParameterProvider`, `OpenApiEndpoints` and the rest of the OpenAPI layer exist only with
the `openapi` feature, so a test file using them starts with `#![cfg(feature = "openapi")]`.

### Development Commands
```bash
# Check code formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy as CI does
cargo clippy --all-features --workspace -- -D warnings

# Run clippy on all targets (the three crates' test code still has warnings; CI does not run this)
cargo clippy --all-targets

# Build documentation (docs.rs builds with all features)
cargo doc --open

# Build with all features enabled
cargo build --all-features
```

## Architecture

### Core Framework Structure
- **`Gotcha` builder**: The application API: routes, state, configuration, tasks, middleware, documentation endpoints, and startup (`run`, `bind`, `bind_on`, `into_router`)
- **`GotchaApp` trait**: Deprecated since 0.5 in favour of the builder, which covers every trait method; it stays until a later release removes it. New code, examples and docs use the builder; tests that still cover the trait carry `#[allow(deprecated)]`
- **GotchaRouter**: Router wrapper that extends Axum's router with OpenAPI operation tracking
- **GotchaContext**: Application context combining configuration and state
- **ConfigWrapper**: Configuration management with environment-based profiles

### Feature-Based Architecture
The framework uses Cargo features for modular functionality:
- `bigdecimal`, `chrono`, `rust_decimal`, `uuid` (default) - `Schematic` impls for those data types
- `openapi` - OpenAPI 3.2 document generation via `oas` crate
- `prometheus` - Metrics via `axum-prometheus`; nothing is served until `.metrics(MetricsEndpoint::default())`
- `cors` - CORS layer via `tower-http` (no extra dependencies)
- `static_files` - Static file serving via `tower-http`
- `task` - Cron schedules; `TaskScheduler` with `spawn` and `interval` is always available

### Configuration System
- Uses `mofa` crate for advanced configuration loading
- Supports environment variable resolution (`${ENV_VAR}`)
- Supports path variable resolution: `${database.name}` reads `database.name` from the root of the merged configuration
- Profile-based configuration via `GOTCHA_ACTIVE_PROFILE` environment variable
- Configuration files located in `configurations/` directory:
  - `application.toml` - Base configuration
  - `application_{profile}.toml` - Profile-specific overrides
- Precedence, lowest to highest: framework `[server]` defaults, `application.toml`, the profile file, files added with `with_file_config`/`with_optional_config`, the `APP` environment prefix (`APP_SERVER__PORT`), prefixes added with `with_env_config`. The `with_*` methods add to these standard sources (`ConfigBuilder::standard()`); `build_config` takes full control. An exported `APP_SERVER__PORT` therefore changes the port an example binds; the configuration tests remove `APP_*` variables in their child processes

### Macro System
The `gotcha_macro` crate provides:
- `#[api]` attribute for automatic OpenAPI operation generation
- `#[derive(Schematic)]` for request, response and parameter schemas, driven by serde attributes plus `#[schematic(..)]`
- `#[config]` and `#[state]` attributes so handlers can extract the configuration or state type with `State<T>`

### OpenAPI Integration
When `openapi` feature is enabled:
- Automatic schema generation from Rust types, following serde's wire shapes (renames, tagging, flatten, skip, `deny_unknown_fields`)
- Documentation endpoints are opt-in: `.with_openapi()` serves `/openapi.json` and `/scalar`; `.openapi_endpoints(..)` sets custom paths. Redoc is off by default because the bundled Redoc rejects 3.2 documents
- `into_openapi()` exports the document without HTTP
- Operations document the rejection responses of Gotcha's extractors; each extractor names its provider with `gotcha::extract::kind::<P>()`
- Tagged union support with discriminator fields
- Complex type mapping including generics and HashMaps

### Examples Architecture
Each example demonstrates specific features:
- `basic/` - Minimal application with custom configuration
- `configuration/` - File, environment, and profile configuration
- `custom_types/` - Constructors for custom state and configuration types
- `custom_types_real/` - Initialized state and configuration in a runnable API
- `extension/` - Extension middleware with documented handlers
- `message/` - Typed commands in application code, with background work through `scheduler.spawn`
- `openapi/` - OpenAPI 3.2 document generation
- `simple/` - Builder API example
- `task/` - An interval background task
- `testing-guide/` - Integration tests with axum-test

## Development Guidelines

### Testing Strategy
- Use `python3 test-feature-matrix.py` to test all feature combinations
- Feature combinations are automatically generated and tested
- Integration tests are `gotcha/tests/*.rs`; trybuild fixtures are in `gotcha/tests/pass/` and `gotcha/tests/fail/`
- `gotcha/tests/schema_matches_serde.rs` checks generated schemas against what serde writes and reads; its known-mismatch list holds shapes the derive does not document exactly yet, many of which need separate request and response schemas
- `test-downstream.py` builds the standalone crates in `tests/fixtures/derive/` and `tests/fixtures/no-default-features/`; the script lists them by name, so a new fixture must be added there

### Configuration Management  
- Configuration files must be in `configurations/` directory
- Use TOML format: the application's own settings at the top level, framework settings in the reserved `[server]` section
- Environment variables can be injected using `${VAR_NAME}` syntax
- Profile switching via `GOTCHA_ACTIVE_PROFILE` environment variable

### OpenAPI Development
- Use `#[api]` macro on handler functions for automatic schema generation
- Implement `Schematic` derive on request/response types
- Test generated schemas against expected JSON in `gotcha/tests/pass/openapi/`
- Documentation endpoints are only mounted when the application calls `.with_openapi()` or `.openapi_endpoints(..)`

### Changes and Releases
- A breaking change adds a section at the top of `MIGRATION.md`, with a TOC entry, saying what to change. Until a release, sections are titled `Unreleased: <title>`; at release they become `<old> → <new>: <title>` (as `0.4 → 0.5: ...`). Retitling changes the anchors, so the links in README, CHANGELOG and the `GotchaApp` `#[deprecated]` note change in the same commit
- `CHANGELOG.md` has one entry per release, linking PRs and MIGRATION sections
- `README.md` is also the `gotcha` crate page on crates.io and docs.rs, where `MIGRATION.md` is not next to it: link to it with absolute `https://github.com/Kilerd/gotcha/blob/main/MIGRATION.md#...` URLs
- Publish with `cargo publish --workspace`, which releases `gotcha_macro`, `gotcha_core`, then `gotcha`, and skips the examples
