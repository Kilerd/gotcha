# Gotcha

An enhanced web framework built on top of Axum, providing additional features and conveniences for building robust web applications in Rust.

[![Crates.io](https://img.shields.io/crates/v/gotcha.svg)](https://crates.io/crates/gotcha)
[![Documentation](https://docs.rs/gotcha/badge.svg)](https://docs.rs/gotcha)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## ✨ Features

- 🚀 **Built on Axum** - High performance and reliability
- 📚 **Automatic OpenAPI** - Generate documentation from your code
- 📊 **Prometheus Metrics** - Built-in metrics collection
- 🌐 **CORS Support** - Cross-origin resource sharing
- 🔌 **WebSocket & SSE** - Real-time endpoints, re-exported and ready
- 📁 **Static Files** - Serve static content effortlessly
- ⏰ **Task Scheduling** - Cron and interval-based background tasks
- 💌 **Message System** - Built-in inter-service communication
- ⚙️ **Smart Configuration** - Environment-based config with variable resolution
- 🏗️ **Two APIs** - Choose between simple builder API or advanced trait-based API

## 🚀 Quick Start

### Simple Builder API (Recommended for new projects)

```rust,no_run
use gotcha::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::new()
        .get("/", || async { "Hello World" })
        .get("/hello/{name}", |Path(name): Path<String>| async move {
            format!("Hello, {}!", name)
        })
        .post("/users", |Json(user): Json<User>| async move {
            Json(user) // Echo the user back
        })
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct User {
    name: String,
    email: String,
}
```

### Advanced Trait API (For complex applications)

```rust,no_run
use gotcha::prelude::*;

#[config]
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
}

#[state]
#[derive(Clone, Default)]
pub struct AppState {
    pub started_at: u64,
}

pub struct App {}

impl GotchaApp for App {
    type State = AppState;
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>)
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
            .get("/", hello_world)
            .get("/users/{id}", get_user)
    }

    async fn state(&self, config: &ConfigWrapper<Self::Config>) -> GotchaResult<Self::State> {
        // Open database connections here; `config` is already loaded.
        let _ = &config.database_url;
        Ok(AppState::default())
    }
}

// The application's own config and state extract directly, thanks to `#[config]` / `#[state]`.
async fn hello_world(State(config): State<Config>) -> impl Responder {
    config.redis_url.clone()
}

async fn get_user(Path(id): Path<u32>, State(_state): State<AppState>) -> impl Responder {
    format!("user {id}")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}
```

## 📦 Installation

Add Gotcha to your `Cargo.toml`:

```toml
[dependencies]
gotcha = "0.4"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
```

### Optional Features

Enable additional features as needed:

```toml
[dependencies]
gotcha = { version = "0.4", features = ["openapi", "prometheus", "cors", "static_files", "task"] }
```

Available features:
- `openapi` - Automatic OpenAPI/Swagger documentation
- `prometheus` - Metrics collection and exposition
- `cors` - Cross-Origin Resource Sharing support
- `static_files` - Static file serving capabilities
- `task` - Background task scheduling with cron support

## 📖 Documentation & Examples

### Composing HTTP Methods

Use Gotcha's method constructors to keep HTTP routing and OpenAPI metadata together:

```rust
use gotcha::prelude::*;

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn list() -> String { "items".into() }
#[cfg_attr(feature = "openapi", gotcha::api)]
async fn create() -> String { "created".into() }

let app = Gotcha::new().route("/items", get(list).post(create));
```

These constructors return `MethodRouter`, which supports `on`, `merge`, `clone`, and
`layer` while retaining annotated handlers' metadata. The `.get(path, handler)` shortcuts use
the same registration logic. With `openapi` enabled, `#[api]` handlers generate operations;
unannotated handlers remain executable without generated operations.

Use `.route_raw(path, axum::routing::get(handler))` for native Axum method routers. Raw routes
do not generate OpenAPI operations, even for `#[api]` handlers. See
[the migration guide](MIGRATION.md#unreleased-documented-method-routers) for existing `.route()`
calls and the changed root/prelude exports.

### Composing Route Modules

`Gotcha::nest` and `Gotcha::merge` accept `GotchaRouter` modules. Configure state, configuration
sources, listening addresses, and tasks on the top-level application; all modules receive its
context. Routes, middleware, and OpenAPI metadata stay with the module.

```rust
use gotcha::prelude::*;

let api = GotchaRouter::default().get("/items", || async { "items" });
let health = GotchaRouter::default().get("/health", || async { "ok" });
let app = Gotcha::new().nest("/api", api).merge(health);
```

Passing a complete `Gotcha` application is a compile error. For independently configured
applications, run each separately. See [the migration guide](MIGRATION.md#unreleased-application-and-route-composition)
for moving existing child settings and task registrations to the top level.

### OpenAPI Documentation

With the `openapi` feature enabled, use the `#[api]` macro for automatic documentation:

```rust,ignore
use gotcha::prelude::*;

#[derive(Schematic, Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

/// Get user by ID
#[api(id = "get_user", group = "users")]
async fn get_user(Path(id): Path<u32>) -> Json<User> {
    Json(User { id, name: "Ada".into(), email: "ada@example.com".into() })
}
```

Component names retain short names when unique. Generic instances use names such as
`Envelope_String` and `Envelope_u32`. If different modules define `SendResult`, assembly uses
PascalCase module prefixes, such as `ScreensSendResult` and `TerminalsSendResult`, and emits a
`tracing::warn!` with the conflicting Rust types and their final names.

Use an explicit name to keep a public component stable across module moves:

```rust
# #[cfg(feature = "openapi")]
# mod schema_name_example {
use gotcha::Schematic;

#[derive(Schematic)]
#[schematic(name = "ScreenSendResult")]
struct SendResult {
    screen_id: String,
}
# }
```

Names must be nonempty and use only ASCII letters, digits, `.`, `-`, or `_`. Choose a unique
explicit name: duplicate overrides also warn and receive distinct module-prefixed names.
See [the migration guide](MIGRATION.md#unreleased-schema-identity-and-component-names) for details.

Visit these endpoints when running:
- `/redoc` - ReDoc documentation interface
- `/scalar` - Scalar documentation interface  
- `/openapi.json` - Raw OpenAPI specification

Customize the document with `.openapi(|mut spec| { /* edits */ spec })` on either `Gotcha` or
`GotchaRouter`. Repeated calls compose in registration order. Child subtrees run in `nest`/`merge`
insertion order, then the parent's own callbacks run, once at assembly. All callbacks edit the
complete document: top-level `security` is global even when set by a child. Use operation-level
security for individual routes. Later writes win; maps and lists are not implicitly merged.
See [the migration guide](MIGRATION.md#unreleased-openapi-transform-composition) for details.

Response contracts come from HTTP return types: `Json<T>` documents JSON, `String` documents
plain text, `Html<T>` documents HTML, and byte bodies document binary content. `Schematic` only
describes data. Use `WithStatus<T, STATUS>` for a status shared by the response and its document:

```rust
use gotcha::prelude::*;

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn create() -> WithStatus<Json<String>, 201> {
    WithStatus::new(Json("created".into()))
}
```

`Result<T, E>` combines both HTTP response contracts by default. Custom response/error types can
implement `Responsible` using the helpers in `gotcha::response`.

To document errors on each endpoint, use `#[api(errors(...))]`. Only the success response is inferred;
your error enum needs `IntoResponse` for HTTP behavior, with no `Responsible` implementation:

```rust
use gotcha::{Json, axum::{http::StatusCode, response::{IntoResponse, Response}}};

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("User not found")]
    NotFound,
    #[error("User already exists")]
    Conflict,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
        };
        (status, Json(self.to_string())).into_response()
    }
}

#[cfg_attr(feature = "openapi", gotcha::api(errors(
    response(status = 404, body = "String", description = "User not found"),
    response(status = 409, body = "String", description = "User already exists")
)))]
async fn create_user() -> Result<Json<String>, ApiError> {
    Err(ApiError::Conflict)
}
```

Each endpoint declares its complete error set; the same enum can be shared by endpoints with
different errors. `body` describes the actual wire data, which may be separate from the error enum.
`errors(...)` also supports return type aliases such as `ApiResult<T>`.

Use `responses(...)` to add or override individual statuses while keeping response inference;
`drop_default` explicitly removes an inferred default. Declarations only affect documentation.
See [the migration guide](MIGRATION.md#unreleased-http-response-contracts) for media types and
inference/override rules.

### Configuration System

Create a `configurations/application.toml` file. Your application's own settings live at the top
level; the framework's are in the reserved `[server]` section:

```toml
database_url = "${DATABASE_URL}"
api_key = "${API_KEY}"
app_name = "My Gotcha App"

[server]
host = "127.0.0.1"
port = 3000
```

Mark your config type with `#[config]` to extract it directly in handlers:

```rust
use gotcha::prelude::*;

#[config]
#[derive(Clone, Default, Serialize, Deserialize)]
struct Config {
    app_name: String,
}

async fn handler(State(config): State<Config>) -> impl Responder {
    config.app_name.clone()
}
```

The server settings are their own extractor, `State<ServerConfig>`; `State<ConfigWrapper<Config>>`
still gives you both at once and derefs to your config.

Configuration supports:
- Environment variable resolution inside values: `${ENV_VAR}`
- Path variable resolution: `${app.database.name}`
- Profile-based overrides via `GOTCHA_ACTIVE_PROFILE` environment variable
- Environment overrides with the `APP_` prefix, where `__` separates nested sections:

  | variable | overrides |
  |---|---|
  | `APP_APP_NAME=x` | the top-level `app_name` field |
  | `APP_SERVER__PORT=8080` | `port` inside `[server]` |

  A single underscore stays part of the field name, so snake_case fields are addressable, and
  typed fields (numbers, booleans) parse the value rather than rejecting it.

### Task Scheduling

Requires the `task` feature.

```rust,ignore
use gotcha::prelude::*;
use std::time::Duration;

# pub struct App {}
impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>)
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
    }

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) -> GotchaResult<Self::State> {
        Ok(())
    }

    async fn tasks(&self, scheduler: &mut TaskScheduler<Self::State, Self::Config>) -> GotchaResult<()> {
        // Daily cleanup at 2 AM (cron fields: sec min hour day month weekday)
        scheduler.cron("cleanup", "0 0 2 * * *".to_string(), |_ctx| async {
            println!("Running cleanup task");
        });
        // Every 30 seconds
        scheduler.interval("heartbeat", Duration::from_secs(30), |_ctx| async {
            println!("Heartbeat");
        });
        Ok(())
    }
}
```

## 🏗️ Architecture

Gotcha is organized as a Rust workspace with the following structure:

```text
gotcha/
├── gotcha/           # Main framework crate
├── gotcha_macro/     # Procedural macros
└── examples/         # Example applications
    ├── basic/        # Basic usage example
    ├── openapi/      # OpenAPI documentation example
    ├── configuration/# Configuration management example
    ├── task/         # Background tasks example
    ├── message/      # Message system example
    └── simple/       # Builder API example
```

### Core Concepts

- **GotchaApp trait** - Main application interface for complex apps
- **Gotcha builder** - Simple API for straightforward applications  
- **GotchaRouter** - Enhanced Axum router with OpenAPI integration
- **GotchaContext** - Application context combining state and configuration
- **ConfigWrapper** - Configuration management with environment resolution

## 🔧 Development

### Building

```bash
# Build main crate
cargo build --package gotcha

# Build with all features
cargo build --all-features

# Test all feature combinations
python3 test-feature-matrix.py
```

### Testing

```bash
# Run tests
cargo test --package gotcha

# Test with specific features
cargo test --package gotcha --features "openapi prometheus"
```

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy --all-targets

# Generate documentation
cargo doc --open
```

## 📚 Examples

Run any example to see Gotcha in action:

```bash
cd examples/simple && cargo run    # Builder API showcase
cd examples/openapi && cargo run   # OpenAPI documentation
cd examples/task && cargo run      # Background tasks
cd examples/message && cargo run   # Message system
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `python3 test-feature-matrix.py`
5. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](https://github.com/Kilerd/gotcha/blob/main/LICENSE) file for details.

## 🔗 Related Projects

- [Axum](https://github.com/tokio-rs/axum) - The underlying web framework
- [mofa](https://crates.io/crates/mofa) - Configuration management
- [oas](https://crates.io/crates/oas) - OpenAPI schema generation
