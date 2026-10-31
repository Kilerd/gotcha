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

let app = Gotcha::from_context(GotchaContext::new(
    AppState { started_at: std::time::Instant::now() },
    ConfigWrapper {
        server: ServerConfig::default(),
        app: AppConfig { deployment: "production".into() },
    },
))
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

### Shutdown and lifecycle

Both application APIs stop gracefully on Ctrl-C or Unix SIGTERM. When the signal resolves, gotcha:

1. cancels the shutdown token in `GotchaContext` and stops accepting connections,
2. drains in-flight HTTP connections, bounded by `http_shutdown_timeout` (30 seconds by default).
   This also waits for upgraded connections such as WebSockets, which axum no longer tracks after
   the upgrade,
3. waits for running scheduled executions, bounded by `task_shutdown_timeout` (30 seconds by
   default with the `task` feature; this deadline runs concurrently with HTTP draining),
4. runs cleanup hooks in registration order.

At the HTTP drain deadline, remaining connections (including upgraded ones) are closed at the OS
socket level, so a handler that is not polling its socket is disconnected too. Gotcha then waits
up to one second for those handlers to exit before running cleanup; connections still alive after
that are left to the runtime and shutdown continues. A panicking cleanup hook does not skip later
hooks; the first panic is
re-raised from `run()` / `serve()` after every hook has run. A hook that never resolves blocks
shutdown, as there is no cleanup timeout. A server that is dropped or cancelled before the
deadline skips draining and cleanup, as before. Signal handling starts only when serving starts;
holding an unserved `BoundServer` installs no signal handler, and dropping it cancels the context
token so captured-token waiters wake. To provide your own shutdown signal:

```rust,no_run
use gotcha::Gotcha;

let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
let app = Gotcha::new()
    .shutdown_signal(async move { let _ = stopped.await; });
// Send through `stop` to request shutdown; await `app.run()` to wait for it to finish.
```

Handlers and tasks can observe the same moment through the context, which is cancelled before
HTTP draining starts. Long-lived responses such as WebSockets and server-sent events can select on
it instead of waiting to be reaped:

```rust,no_run
use gotcha::{GotchaContext, State};

async fn events(State(context): State<GotchaContext<(), ()>>) -> &'static str {
    let shutdown = context.shutdown();
    tokio::select! {
        _ = shutdown.cancelled() => "closing",
        _ = std::future::pending::<()>() => "still running",
    }
}
```

Cleanup hooks run after serving stops. Register them on the builder (in order) or override
`on_shutdown(&self, context)` on the trait API:

```rust,no_run
use gotcha::Gotcha;

let app = Gotcha::new()
    .on_shutdown(|context| async move {
        tracing::info!(port = context.config.server.port, "application stopped");
    })
    .on_shutdown(|_| async { /* tear down later resources */ });
```

Binding without serving exposes the effective address first, so port `0` works for integration
tests and in-process tools. `run()` and `listen()` are thin wrappers over `bind()`/`bind_on()` and
`serve()`:

```rust,no_run
use gotcha::Gotcha;

# async fn example() -> gotcha::GotchaResult<()> {
let server = Gotcha::new().port(0).bind().await?;
println!("listening on http://{}", server.local_addr());
server.serve().await?;
# Ok(()) }
```

`BoundServer::serve_with_shutdown(future)` actually replaces the application signal for one
server: the configured `shutdown_signal` is not polled, so Ctrl-C and SIGTERM do not stop it,
while cleanup hooks still run. The trait API offers `bind()`, `http_shutdown_timeout()`, and
`on_shutdown(&context)` as well.

Override the HTTP drain period with builder `.http_shutdown_timeout(duration)` or trait
`http_shutdown_timeout(&self)`; see the [migration guide](MIGRATION.md#unreleased-bound-servers-and-shutdown-lifecycle).
Scheduled executions keep their independent grace period (builder `.task_shutdown_timeout(duration)`
or trait `task_shutdown_timeout(&self)`); see the [task migration notes](MIGRATION.md#unreleased-owned-scheduled-tasks).

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

### Scoping Middleware and Mounting Services

`GotchaRouter::route_layer` (and the builder's `route_layer`) applies middleware only to routes
already registered. Requests that match no route bypass it entirely, including the fallback, and
documentation endpoints are not covered. To scope middleware to a documented prefix without
re-registering its routes, nest a child router and layer the child: `nest` prefixes the child's
documented paths, so `.nest("/api", api.layer(guard))` guards exactly `/api/*` and the generated
document still lists `/api/...`.

`nest_service` mounts a bare tower `Service` under a path prefix, for reverse proxies, file
handlers, or other services that are not Gotcha routes. Nested services are opaque to OpenAPI:
neither the prefix nor the service's responses appear in the document. They also do not receive the
application context, so a service that needs state at construction time is mounted from a
post-assembly hook:

```rust,no_run
use gotcha::{Gotcha, GotchaContext};

let app = Gotcha::from_state(())
    .get("/health", || async { "ok" })
    .finish_router(|router, context: &GotchaContext<(), gotcha::EmptyConfig>| {
        // Build the service from `context.state` here, then mount it:
        // router.nest_service("/proxy", proxy)
        let _ = context;
        router
    });
```

The builder's `finish_router` mirrors the trait's
`GotchaApp::finish_router(&self, router, context)` hook. Hooks run once at startup after business
routes, fallback, and documentation are mounted, and receive a clone of the application's
`GotchaContext` — configuration is duplicated, while state shared behind `Arc` is the same value
the routes see. `app_layer` middleware is installed after these hooks, so it covers whatever they
mount. `finish_router` is not called during document export.

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

#### Optional and nullable fields

`#[derive(Schematic)]` reads the serde attributes already on a field, so the document follows the
contract for the patterns below. `Option<T>` on its own is optional *and* nullable; add a serde
attribute to choose one axis:

| Field | Generated schema | Serde behavior | zod equivalent |
| --- | --- | --- | --- |
| `value: Option<T>` | optional, `anyOf: [T, {"type": "null"}]` | missing or `null` → `None`; `None` → `null` | `.optional().nullable()` |
| `#[serde(deserialize_with = "Option::deserialize")] value: Option<T>` | required, same `anyOf` | missing key → error; `null` → `None` | `.nullable()` |
| `#[serde(skip_serializing_if = "Option::is_none")] value: Option<T>` | optional, `T` (no null branch) | missing or `null` → `None`; `None` is omitted | `.optional()` |
| `deserialize_with` or `with` **and** `skip_serializing_if = "Option::is_none"`, no `default` | optional, same `anyOf` as plain `Option<T>` | input requires the key; `null` → `None`; output omits `None` | no single equivalent |
| `#[serde(default)]` on the struct | every direct field optional (a `#[serde(flatten)]` field keeps the inner type's requiredness) | missing direct keys use the field defaults | every direct field `.optional()` |

`deserialize_with` (or `with`) without a `default` requires the key, so the field stays required
even when its type is `Option<T>`. Only `Option::is_none`, `std::option::Option::is_none` and
`core::option::Option::is_none` are recognized for dropping the null branch; any other
`skip_serializing_if` predicate keeps today's behavior (optional, with the null branch).

Combining a custom deserializer with `Option::is_none` and no `default` is asymmetric: input
requires the key while output may omit it, and one schema cannot express both directions. That
field keeps the previous optional + nullable contract. Prefer adding a `default` to make a
missing key acceptable; for "absent, `null` or value" use the tri-state pattern below.

A PATCH-style tri-state distinguishes "absent", `null` and a value with `Option<Option<T>>`. The
document is optional and nullable with one null branch. The inner option needs a small helper to
turn `null` into `Some(None)`:

```rust
# #[cfg(feature = "openapi")]
# mod tri_state_example {
use gotcha::prelude::*;
use serde::Deserializer;

fn deserialize_some<'de, T: Deserialize<'de>, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Schematic, Serialize, Deserialize)]
struct Attributes {
    theme: String,
}

#[derive(Schematic, Serialize, Deserialize)]
struct Patch {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    attributes: Option<Option<Attributes>>,
}
# }
```

Missing `attributes` deserializes to `None`, `null` to `Some(None)`, and an object to
`Some(Some(...))`; serializing omits `None` and writes `null` for `Some(None)`.

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

`layer` covers every business route already registered on the router, following Axum's ordering;
`route_layer` covers only requests that match a registered route and leaves the fallback untouched.
Use builder `app_layer` or the trait's `finish_router` hook for authentication that must cover
**every endpoint**, including documentation and fallbacks. Application layers run after
documentation is mounted, regardless of when `with_openapi()` was called. Keeping authentication
on business routes leaves explicitly enabled documentation public.

#### Document metadata

Generated documents default to a neutral `info` block: `title: "API"`, `version: "0.1.0"`, and no
description. Set the application's identity with `.openapi_info(..)` on the builder, or by
returning it from `GotchaApp::openapi_info`. The builder setting applies to the served document
and every offline export. The trait hook applies to the served document and
`GotchaApp::openapi_document()`, the recommended export for trait applications; when exporting
straight from `routes(..)`, pass it through with
`app.routes(GotchaRouter::default()).openapi_info(app.openapi_info())`.
`gotcha::cargo_info!()` builds the block from the calling crate's Cargo metadata:
`CARGO_PKG_NAME` becomes the title, `CARGO_PKG_VERSION` the version, and a non-empty
`CARGO_PKG_DESCRIPTION` the description.

```rust,no_run
# #[cfg(feature = "openapi")]
# {
use gotcha::Gotcha;
let app = Gotcha::new().openapi_info(gotcha::cargo_info!());
# }
```

The info block is application-level: nested and merged routers do not contribute one.
`.openapi(..)` transforms run afterwards and can still override any field.

Export the complete document with `Gotcha::into_openapi()`, `GotchaRouter::into_openapi()`, or
`GotchaApp::openapi_document()`. Trait applications should prefer `openapi_document()`, which also
applies `extractor_configs()` and `openapi_info()`. Export needs no listener or runtime and does
not initialize application state, load configuration, register tasks, or install application
layers:

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

Each extractor provider has its own rejection configuration. The defaults keep Axum's own status
(`400`, `415`, `422`, ...) and send `{"error": "<message>"}` as JSON; a `Valid` validation failure
adds a `fields` map to that body, and `GotchaError` renders `500` with a generic message while
logging the details. Override one provider without changing the others:

```rust
use gotcha::prelude::*;
use gotcha::{ErrorBody, JsonConfig};

let app = Gotcha::new()
    .extractor_config(JsonConfig::default().error_handler(|rejection| {
        (
            gotcha::axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            ErrorBody::new(rejection.body_text()),
        )
    }))
    .post("/items", |Json(item): Json<CreateItem>| async move { Json(item) });
# #[derive(serde::Deserialize, serde::Serialize)] struct CreateItem { name: String }
```

The handler receives a reference to that provider's concrete rejection type and returns
`(StatusCode, E)`. With the `openapi` feature, `E` must also implement `Schematic`, and an
overridden body is registered as a component; defaults add no components, so existing documents
stay unchanged. The builder and the trait API (`GotchaApp::extractor_configs`) share the same
config types. Providers are `Json`, `Form`, `Query`, `Path`, `Multipart`, `TypedHeader`,
`Header`, `Cookie`, and `Valid` (validation failures only; a malformed body inside
`Valid<Json<T>>` uses the `Json` handler).

A `Valid` failure keeps `422 Unprocessable Entity` and also renders as `ErrorBody`:

```json
{"error": "validation failed", "fields": {"address.city": ["length must be at least 1"]}}
```

Nested struct fields use dotted paths (`address.city`), list elements are indexed
(`items[0].name`), and struct-level validations keep validator's `__all__` key. A directly
validated collection has no field name (`[0].name`). Values of a nested map are indexed by
validator's iteration order and lose their key. Field segments are Rust field names:
`#[serde(rename)]` / `#[serde(rename_all)]` are not applied (`first_name`, not `firstName`);
remap them in a `ValidConfig` handler if clients need the JSON names. Messages are the declared
ones or readable defaults. A `ValidConfig` handler still receives the original
`validator::ValidationErrors`; `gotcha::validation::flatten_errors` builds the same field map.

Extractor rejections are `Rejected<R>`: they carry both the original rejection and the rendered
response, so Axum's per-handler pattern still works:

```rust
use gotcha::prelude::*;
use gotcha::axum::extract::rejection::JsonRejection;

async fn create(body: Result<Json<CreateItem>, Rejected<JsonRejection>>) -> Result<String, HttpError> {
    let Json(item) = body.map_err(|rejected| HttpError::bad_request(rejected.rejection().body_text()))?;
    Ok(item.name)
}
# #[derive(serde::Deserialize)] struct CreateItem { name: String }
```

`Form`, `Multipart`, `Json`, `Path`, `Query`, and `TypedHeader` are Gotcha wrappers; `Header` and
`Cookie` are Gotcha's named-parameter extractors. Use `gotcha::axum::extract::*` or
`gotcha::axum_extra::TypedHeader` for Axum's originals, which keep Axum's own rejection responses.
The configs are installed as the outermost layer of the assembled application, so routes added
in `finish_router` and `app_layer` middleware also see them. An application that overrides
`GotchaApp::build_router` must call `ExtractorConfigs::layer(router)` last; otherwise extractors
fall back to the defaults, as they do in a plain Axum router. `Extension` is still Axum's
extractor.

`HttpError` remains an optional convenience type for application errors and `#[api(errors(...))]`:

```rust
use gotcha::prelude::*;

#[cfg_attr(feature = "openapi", gotcha::api(errors(
    response(status = 404, body = "HttpError", description = "Item not found")
)))]
async fn read_item(Path(id): Path<u32>) -> Result<Json<String>, HttpError> {
    if id == 0 {
        return Err(HttpError::not_found("item not found"));
    }
    Ok(Json(id.to_string()))
}
```

Use `responses(...)` to add or override individual statuses while keeping response inference;
`drop_default` explicitly removes an inferred default. Declarations only affect documentation.
See [the migration guide](MIGRATION.md#unreleased-http-response-contracts) for media types and
inference/override rules.

Request bodies are inferred from body extractors such as `Json<T>` and `Form<T>`. When the wire
shape differs from the extractor's type — most often a `multipart/form-data` upload parsed field
by field — declare the body with `request_body(...)`:

```rust
# #[cfg(feature = "openapi")]
# mod upload_example {
use gotcha::openapi::Binary;
use gotcha::prelude::*;

/// A file upload with a title.
#[derive(Schematic)]
struct UploadForm {
    file: Binary,
    title: String,
}

#[api(request_body(
    content_type = "multipart/form-data",
    body = "UploadForm",
    description = "File upload"
))]
async fn upload(_form: Multipart) -> String {
    // Read the fields with `Multipart`; the declaration documents the wire format only.
    String::new()
}
# }
```

The declaration replaces the body that `Json`, `Form`, or `Multipart` would have inferred; path,
query, header, and cookie parameters are still documented. `content_type` defaults to
`application/json`, `required` defaults to `true`, and `description` is optional. Repeat
`request_body(...)` with distinct `content_type` values to describe several representations of the
same body — the entries must agree on `description` and `required`. Body types become components
exactly like response bodies.
`Binary` describes a file field as the OpenAPI 3.2 form
`{"type": "string", "contentMediaType": "application/octet-stream"}`.

Handlers whose response cannot be inferred — WebSocket upgrades, SSE streams, or any opaque
return type — use `skip_response`. The nonempty `responses(...)` list is then the complete
contract and the return type is never referenced by the macro, so `impl Trait` is allowed:

```rust
# #[cfg(feature = "openapi")]
# mod streaming_example {
use std::convert::Infallible;

use gotcha::prelude::*;

/// A parsed server-sent event: its `data` plus the optional SSE fields.
#[derive(Schematic)]
struct ChatSseEvent {
    data: String,
    event: Option<String>,
    id: Option<String>,
    retry: Option<u32>,
}

#[api(
    skip_response,
    responses(response(
        status = 200,
        content_type = "text/event-stream",
        item = "ChatSseEvent"
    ))
)]
async fn events() -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    Sse::new(futures_util::stream::once(async {
        Ok(Event::default().event("message").data(r#"{"seq":1}"#))
    }))
}
# }
```

`item` emits OpenAPI 3.2's `itemSchema`, describing each parsed item of a sequential media type,
and requires an explicit `content_type`. For `text/event-stream` the item **must describe the
parsed event envelope** (`data` plus the optional `event`, `id`, and `retry` fields), not just the
JSON carried inside `data`; a hand-written `Schematic` can attach `contentMediaType` and
`contentSchema` to `data` when that JSON has its own schema. For `application/jsonl`, `item`
describes each JSON value instead. `body` remains the whole-body schema and the two fields are
independent.
`skip_response` cannot be combined with `errors(...)` (declare every status under `responses(...)`)
or `drop_default` (there is no inferred default to remove). A WebSocket route documents its
upgrade as an empty `101` response and needs no `Responsible` implementation:

```rust
# #[cfg(feature = "openapi")]
# mod websocket_example {
use gotcha::prelude::*;

#[api(
    skip_response,
    responses(response(status = 101, description = "Switching protocols"))
)]
async fn socket(upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(|socket| async move {
        drop(socket);
    })
}
# }
```

Extractors with no OpenAPI representation are excluded with `#[api(skip)]` on the parameter.
It accepts exactly that form; other parameter-level `#[api(...)]` contents are compile errors:

```rust
# #[cfg(feature = "openapi")]
# mod skip_parameter_example {
use gotcha::prelude::*;

struct RawConnection;

#[api]
async fn handler(#[api(skip)] connection: RawConnection) -> String {
    let _ = connection;
    String::new()
}
# }
```

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
- Environment variable resolution inside values: `${ENV_VAR}`. An unset variable resolves to
  an empty string.
- Path variable resolution: `${app.database.name}`. A missing path resolves to an empty string.
- Profile-based overrides via `GOTCHA_ACTIVE_PROFILE` environment variable
- Environment overrides with the `APP_` prefix, where `__` separates nested sections:

  | variable | overrides |
  |---|---|
  | `APP_APP_NAME=x` | the top-level `app_name` field |
  | `APP_SERVER__PORT=8080` | `port` inside `[server]` |

  A single underscore stays part of the field name, so snake_case fields are addressable, and
  typed fields (numbers, booleans) parse the value rather than rejecting it.

Variable substitution is enabled for automatic loading and for every `with_*` source helper,
including `with_default_config()`, `with_default_files()`, and `with_default_env()`.

Framework-managed loading installs typed defaults for the reserved `[server]` section beneath
every file and environment source. Environment-only configuration therefore works without a
configuration file:

```sh
APP_SERVER__PORT=9000 cargo run
```

Applications can register their own typed defaults at the same layer with
`ConfigBuilder::defaults`. Application defaults beat the framework's `127.0.0.1:3000`, while
files and environment variables still beat them:

```rust,no_run
use gotcha::prelude::*;
use gotcha::config::ConfigBuilder;

#[derive(Serialize)]
struct Defaults {
    database_url: String,
    server: ServerConfig,
}

let _app = Gotcha::new()
    .build_config(|builder| {
        builder
            .defaults(Defaults {
                database_url: "postgres://localhost/app".into(),
                server: ServerConfig { host: "0.0.0.0".into(), port: 8700 },
            })
            .expect("defaults serialize")
    })
    .expect("configuration loads");
```

The trait API can use the same builder in a custom `GotchaApp::config()`. Manual loaders should
start from `ConfigBuilder::framework_defaults()` rather than `ConfigBuilder::new()`; without the
typed `[server]` base, `APP_SERVER__PORT` is not coerced when no file supplies `server.port`:

```rust,no_run
use gotcha::config::{ConfigBuilder, ConfigWrapper};

let config: ConfigWrapper<gotcha::EmptyConfig> = ConfigBuilder::framework_defaults()
    .file_optional("configurations/application.toml")
    .env("APP")
    .enable_vars()
    .build()
    .expect("configuration loads");
```

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
