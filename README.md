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
- 💌 **Typed Commands** - Reusable messages with application state and configuration
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

### Initialized state and configuration

Use `Gotcha::from_state(value)` for state such as an initialized connection pool. To supply both
state and configuration, use `Gotcha::from_context(context)`. These values only need
`Clone + Send + Sync + 'static`; neither needs `Default`, and configuration needs no serde traits:

```rust,no_run
use gotcha::prelude::*;

#[state]
#[derive(Clone)]
struct AppState { started_at: std::time::Instant }

#[config]
#[derive(Clone)]
struct AppConfig { deployment: String }

let app = Gotcha::from_context(GotchaContext {
    state: AppState { started_at: std::time::Instant::now() },
    config: ConfigWrapper {
        server: ServerConfig::default(),
        app: AppConfig { deployment: "production".into() },
    },
})
.get("/deployment", |State(config): State<AppConfig>| async move { config.deployment });
```

`with_state::<T>()` and `with_types::<S, C>()` remain available when you want automatic default
construction. File loading requires `Deserialize`; serializing a configuration requires `Serialize`.

### Startup behavior

Both APIs load configuration before selecting a listener address. The priority is:

1. The explicit `listen(...)` / `listen_on(...)` argument.
2. Builder `.host(...)` / `.port(...)` overrides, applied independently.
3. `ConfigWrapper::server`, including settings loaded from files or environment variables.
4. Framework defaults: `127.0.0.1:3000` when the server section is absent.

Both APIs bind before state initialization, route assembly, and task registration. State, handlers,
and tasks see the effective address in `config.server`, including the actual port selected for `0`.
Binding failure therefore does not start background tasks.

Configuration errors stop startup by default. To explicitly allow a default fallback:

```rust,no_run
use gotcha::{ConfigErrorPolicy, Gotcha};

let app = Gotcha::new()
    .config_error_policy(ConfigErrorPolicy::fallback_to_default());
```

For the trait API, override `config_error_policy()` to return the same policy. A custom fallback
can use `ConfigErrorPolicy::Fallback(factory)`, where the factory returns a `ConfigWrapper<C>`.
Fallback applies only to startup configuration loading; eager `build_config()` and direct
`GotchaApp::config()` calls still return errors. See the [migration guide](MIGRATION.md#unreleased-shared-startup)
for behavior changes and the trait example.

### Shutdown

Both application APIs stop gracefully on Ctrl-C or Unix SIGTERM. HTTP stops accepting connections,
existing requests drain, and scheduled tasks stop starting new executions. To provide your own
shutdown signal:

```rust,no_run
use gotcha::Gotcha;

let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
let app = Gotcha::new()
    .shutdown_signal(async move { let _ = stopped.await; });
// Send through `stop` to request shutdown; await `app.run()` to wait for it to finish.
```

The trait API offers the same `shutdown_signal(&self)` hook. With the `task` feature, in-flight
scheduled executions get 30 seconds to finish before being aborted and joined. Override that period
with builder `.task_shutdown_timeout(duration)` or trait `task_shutdown_timeout(&self)`. This timeout
applies to scheduled tasks; HTTP requests drain independently. See the [migration guide](MIGRATION.md#unreleased-owned-scheduled-tasks).

### Advanced Trait API (For complex applications)

```rust,no_run
use gotcha::prelude::*;

#[config]
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
}

#[state]
#[derive(Clone)]
pub struct AppState {
    pub started_at: std::time::Instant,
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
        Ok(AppState { started_at: std::time::Instant::now() })
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

HTTP/1 serving is always enabled, including with `default-features = false`.
The builder's `run`/`listen`/`listen_on` and `GotchaApp::run` are always available.

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

`Schematic` lives in `gotcha_core`, so a library can implement or derive it without depending on
the web framework. Applications can use `#[derive(gotcha::Schematic)]` with just the `gotcha`
dependency and its `openapi` feature; a separate `gotcha_core` dependency and trait import are
unnecessary. The derive recognizes Cargo dependency renames. A custom facade can supply
`#[schematic(crate = "path::to::gotcha_core")]` to select its core re-export.

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

Generated documents use OpenAPI 3.2.0 and JSON Schema 2020-12 semantics. See the
[OpenAPI migration notes](MIGRATION.md#unreleased-openapi-32) for custom schemas and tooling compatibility.

Documentation HTTP endpoints are disabled by default. Enable them explicitly with
`Gotcha::with_openapi()`, or return `Some(OpenApiEndpoints::default())` from
`GotchaApp::openapi_endpoints()`. The default paths are:

- `/scalar` - Scalar documentation interface  
- `/openapi.json` - Raw OpenAPI specification

Use `openapi_endpoints(Some(config))` to customize paths, and `None` to disable all endpoints.
Set `scalar_path` to `None` to serve only JSON. The legacy `redoc_path` is disabled by default
because the bundled Redoc does not support OpenAPI 3.2.

```rust
# #[cfg(feature = "openapi")]
# {
use gotcha::{Gotcha, OpenApiEndpoints};
let app = Gotcha::new().openapi_endpoints(Some(OpenApiEndpoints {
    json_path: "/docs/schema.json".into(),
    redoc_path: None,
    scalar_path: Some("/docs/scalar".into()),
}));
# }
```

`layer` covers business routes already registered, following Axum's ordering. Use builder
`app_layer` or the trait's `finish_router` hook for authentication that must cover **every endpoint**,
including documentation and fallbacks. Application layers run after documentation is mounted,
regardless of when `with_openapi()` was called. Keeping authentication on business routes leaves
explicitly enabled documentation public.

Export the complete document with `Gotcha::into_openapi()`, `GotchaRouter::into_openapi()`, or
`GotchaApp::openapi_document()`. Export needs no listener or runtime and does not initialize
application state, load configuration, register tasks, or install application layers:

```rust
# #[cfg(feature = "openapi")]
# {
use gotcha::Gotcha;
let spec = Gotcha::new()
    .openapi(|mut spec| { spec.info.title = "My API".into(); spec })
    .into_openapi();
let json = serde_json::to_string_pretty(&spec).unwrap();
# }
```

The OpenAPI example supports `cargo run -p openapi -- --export-openapi` for a standalone JSON export.
See [the migration guide](MIGRATION.md#unreleased-explicit-openapi-endpoints) for both APIs and
middleware scope changes.

Customize the document with `.openapi(|mut spec| { /* edits */ spec })` on either `Gotcha` or
`GotchaRouter`. Repeated calls compose in registration order. Child subtrees run in `nest`/`merge`
insertion order, then the parent's own callbacks run, once per exported or served document. All callbacks edit the
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

### Messages

`Message<S, C>` groups a command's input, output, and handler. `Messager<S, C>` carries the
application context and can be extracted as `State<Messager<S, C>>`. This optional convention lets
HTTP handlers, scheduled tasks, and other commands reuse the same operations. Ordinary async
service methods remain equally valid; no message feature flag is required.

`messager.send(command).await` runs directly in the calling future. There is no queue, transport,
retry, or new task. Results and panics propagate to the caller; dropping the future cancels its
in-flight handler without undoing completed side effects.

`messager.spawn(command)` starts a Tokio task and returns `JoinHandle<Command::Output>`. Await it
to observe the result or a `JoinError` for panic/cancellation. If the command returns `Result<T, E>`,
the application error stays in the inner result. Call `abort()` and then await the handle to request
cancellation and wait for completion. Dropping the handle detaches the task, even when caused by
cancelling the future that owns it; application shutdown does not automatically wait for it.

To include a command in a scheduled execution's shutdown boundary, await `send` inside that
execution. See the [message example](examples/message/src/main.rs) for nested commands and joining
a spawned result, and the [migration guide](MIGRATION.md#unreleased-message-task-handles) for the
changed `spawn` return type.

### Task Scheduling

Requires the `task` feature. Registration collects work; execution starts after all application
initialization succeeds. The application retains ownership until shutdown completes. Standalone
users call `scheduler.start()`, retain the returned `RunningTasks`, and call
`running.shutdown(timeout).await` to stop and join their tasks. Dropping the owner aborts its tasks.

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
