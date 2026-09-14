# Migration Guide

- [Unreleased: explicit OpenAPI endpoints](#unreleased-explicit-openapi-endpoints)
- [Unreleased: message task handles](#unreleased-message-task-handles)
- [Unreleased: static OpenAPI descriptors](#unreleased-static-openapi-descriptors)
- [Unreleased: owned scheduled tasks](#unreleased-owned-scheduled-tasks)
- [Unreleased: shared startup](#unreleased-shared-startup)
- [Unreleased: runtime type bounds](#unreleased-runtime-type-bounds)
- [Unreleased: HTTP response contracts](#unreleased-http-response-contracts)
- [Unreleased: application and route composition](#unreleased-application-and-route-composition)
- [Unreleased: documented method routers](#unreleased-documented-method-routers)
- [Unreleased: OpenAPI transform composition](#unreleased-openapi-transform-composition)
- [Unreleased: nested OpenAPI paths](#unreleased-nested-openapi-paths)
- [Unreleased: schema identity and component names](#unreleased-schema-identity-and-component-names)
- [Unreleased: ordered configuration sources](#unreleased-ordered-configuration-sources)
- [0.3 → 0.4](#03--04) — **every application must edit its route paths and configuration file**
- [0.2 → 0.3: API simplification](#02--03-api-simplification)

---

# Unreleased: explicit OpenAPI endpoints

**The `openapi` feature no longer exposes HTTP documentation by itself.** It enables schema and
operation metadata. Applications choose whether and where to serve the resulting document.

For the builder, add `.with_openapi()` to retain the previous `/openapi.json`, `/redoc`, and
`/scalar` endpoints. This formerly empty method now enables the default endpoint configuration.
Use `.openapi_endpoints(Some(OpenApiEndpoints { ... }))` for custom paths, or
`.openapi_endpoints(None)` to disable all endpoints. The last configuration call wins;
`with_openapi()` resets custom paths to their defaults. `redoc_path` and `scalar_path` can each be
`None` to serve just the document or a single UI.

For the trait API, explicitly configure the application:

```rust,no_run
use gotcha::{ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, OpenApiEndpoints};

struct App;
impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    fn openapi_endpoints(&self) -> Option<OpenApiEndpoints> {
        Some(OpenApiEndpoints::default())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        router.get("/health", || async { "ok" })
    }

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> { Ok(()) }
}
```

Endpoint configuration belongs to the top-level application. Nesting or merging business routers
does not enable, relocate, or override documentation endpoints. Both UI pages use the configured
`json_path`. Paths must be distinct literal absolute URI paths, with no query, fragment, or dot
segments; percent-encode other characters. Invalid endpoint configuration fails assembly, while
collisions with existing business routes retain Axum's normal conflict behavior (panic).

## Middleware scope

`Gotcha::layer` and `GotchaRouter::layer` keep their existing Axum semantics: they cover business
routes already registered, not routes added afterwards or application documentation. If you want
private business routes and public documentation, keep authentication on those business routes.

Use `.app_layer(auth_layer)` on the builder for authentication, CORS, or other middleware that must
cover every application endpoint and fallback. It applies after routes and documentation are
assembled, even when registered before them. Repeated calls wrap in Axum order: the last registered
application layer sees incoming requests first. `with_cors()` retains its existing `layer` scope;
use `app_layer(CorsLayer::permissive())` when CORS should also cover documentation.

The corresponding trait hook is `fn finish_router(&self, router: axum::Router) -> axum::Router`.
Return `router.layer(auth_layer)` from it to cover the complete application. This hook runs after
documentation is mounted, without requiring an override of `build_router`. An existing custom
`build_router` still owns its whole assembly and must invoke any desired finalization itself.
Routes added through this native Axum hook do not contribute OpenAPI metadata.

## Export without HTTP

Use `Gotcha::into_openapi()` or `GotchaRouter::into_openapi()` to consume a route definition and
return its `oas::OpenAPIV3`. `GotchaApp::openapi_document()` creates a fresh definition through
`routes()` and consumes that. All three use the same generation and transform pipeline as the
HTTP document; no side channel or HTTP request is needed.

Export does not load configuration, initialize application state, bind a listener, register tasks,
or apply application middleware. User route registration and document transforms still run, so
any side effects written in those callbacks remain. Endpoint paths do not affect the document.
Transforms keep their `FnOnce` captures, nested ordering, and one execution per document.
When documentation is disabled and no export is requested, transforms are not executed at all.

The OpenAPI example demonstrates exporting with `cargo run -p openapi -- --export-openapi`.
It prints JSON and exits without starting the application.

---

# Unreleased: message task handles

`Message` and `Messager` remain available in the same module and root exports, without a feature
flag. They provide a typed command convention and application context, rather than a queue or
transport. `send` still executes directly in the caller's future; its output (including application
errors), panic, and cancellation remain at that boundary.

`Messager::spawn` now returns `tokio::task::JoinHandle<M::Output>` and accepts any message output,
including `Result<T, E>`. Previously it only accepted `Output = ()` and discarded the handle.

```rust,no_run
# use gotcha::{Message, Messager};
# async fn example<M: Message<(), (), Output = Result<u32, std::io::Error>>>(
#     messager: Messager<(), ()>, command: M,
# ) -> Result<(), Box<dyn std::error::Error>> {
let task = messager.spawn(command);
let output = task.await?; // Task panic/cancellation is a JoinError.
let value = output?;     // Application errors stay in the message's Result.
# let _ = value;
# Ok(())
# }
```

Existing calls with a semicolon still compile. Code requiring a `()` return, such as a function
ending in `messager.spawn(command)` without a semicolon or a stored function pointer, must adapt
to the new return type. To explicitly keep the previous detached behavior, use
`drop(messager.spawn(command));`.

Retaining a handle allows joining or aborting; it does **not** enable automatic cancellation on
drop. Dropping it, including when its owning future is cancelled, detaches the task and loses its
result. To cancel, call `task.abort()` and then `task.await` to observe completion. Abort requires
the task to yield and can race with normal completion; already performed side effects remain.

These tasks are not registered with the application's shutdown or scheduler ownership. Await
`messager.send(command)` inside a scheduled execution when the command should share that execution's
lifetime. Commands that spawn further tasks must manage them separately. No queue, retry, or extra
task has been introduced: `spawn` returns the handle from its existing `tokio::spawn` call.

---

# Unreleased: static OpenAPI descriptors

`#[api]` keeps its existing syntax and OpenAPI behavior. Its generated `Operable` now stores
constructor function pointers directly, without `Lazy`, boxed closures, or a runtime constructor
vector. The macro no longer needs UUID-named helper statics or a UUID dependency.

Only code using the low-level descriptor types directly needs to migrate:

| Public type or field | Previous representation | New representation |
| --- | --- | --- |
| `ParamConstructor` | `Box<dyn Fn(String) -> ParamType + Send + Sync>` | `fn(String) -> ParamType` |
| `Operable::parameters` | `&'static Lazy<Vec<ParamConstructor>>` | `&'static [ParamConstructor]` |
| `Operable::responses` | `&'static Lazy<Box<dyn Fn() -> Responses + Send + Sync>>` | `fn() -> Responses` |

Replace lazy constructor collections with a static slice. Replace a lazy boxed response constructor
with a function or a non-capturing closure. For example, these fields can be written directly in a
static `Operable` initializer:

```rust,ignore
parameters: &[<Path<u32> as ParameterProvider>::generate],
responses: <String as Responsible>::response,
```

Capturing closures cannot be stored in the new function-pointer fields. Apply configuration-dependent
changes through the existing `.openapi(move |spec| ...)` transform instead. User transforms still
support captured state. `ParameterProvider::generate`, `ParamType`, and the public `gotcha::Lazy`
and `gotcha::Either` re-exports retain their existing interfaces.

Static constructor storage does not cache generated parameters, responses, schemas, or complete
OpenAPI documents. Constructors still run inside each document's schema collection scope, preserving
path-specific parameter names, independent documents, generic DTOs, recursive references, and
`errors(...)` / `responses(...)` behavior. This is a structural simplification; no benchmarked
performance improvement is claimed.

---

# Unreleased: owned scheduled tasks

**Task registration no longer starts background work.** The application owns its scheduled tasks
and shuts them down together with HTTP serving.

## Registration and ownership

`TaskScheduler::cron` and `interval` now take `&mut self` and only register work. Existing application
hooks already receive a mutable scheduler. Update helper functions that accepted `&TaskScheduler`
to accept `&mut TaskScheduler`.

All registrations must succeed before serving starts any task. A registration error, panic, or
cancelled startup discards the pending tasks. Do not wait inside the registration hook for a
scheduled task to execute: it cannot start until the hook returns.

Standalone callers must explicitly start the scheduler and retain the returned owner:

```rust,ignore
let mut scheduler = TaskScheduler::new(context);
scheduler.interval("cleanup", Duration::from_secs(60), cleanup);
let running = scheduler.start();
// Keep `running` alive for as long as the tasks should run.
running.shutdown(Duration::from_secs(10)).await;
```

`RunningTasks::shutdown(timeout)` stops subsequent executions, waits for current executions, and
aborts and joins unfinished tasks at the deadline. Dropping the owner or cancelling its shutdown
future requests immediate abortion; dropping cannot perform an asynchronous join. The same ownership
includes the current execution, which is no longer a separate detached task.

## Application shutdown

Builder `run` / `listen` / `listen_on` and `GotchaApp::run` now wait for Ctrl-C or Unix SIGTERM by
default, then use one cancellation signal to stop HTTP acceptance and new scheduled executions.
In-flight HTTP requests drain while scheduled tasks finish. Override the signal with a future:

```rust,no_run
use gotcha::Gotcha;
let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
let app = Gotcha::new().shutdown_signal(async move { let _ = stopped.await; });
// Resolve the future by sending through `stop`; `app.run().await` waits for shutdown to finish.
```

For `GotchaApp`, override `fn shutdown_signal(&self) -> impl Future<Output = ()> + Send` instead.
That future may borrow the application. A custom signal replaces the default OS signal handling.

Scheduled executions get a 30-second grace period by default. Configure it with the builder's
`.task_shutdown_timeout(duration)` or the trait's `fn task_shutdown_timeout(&self) -> Duration`.
The deadline starts when shutdown is requested, independently of HTTP draining. It does not limit
HTTP request duration. A serve error also stops and drains the scheduled tasks; cancelling the
serving future requests abortion of all scheduled work and signals HTTP shutdown.

Cancellation requires async tasks to yield. Synchronous blocking work and tasks explicitly detached
by application code (including `Messager::spawn`) remain outside this ownership. Public preparation,
application cleanup hooks, and HTTP drain limits remain follow-up work in #82.

Single-execution panics are logged and the schedule continues, including a panic while constructing
the execution future. Interval tasks retain their existing immediate first tick and non-overlapping
executions. Invalid cron expressions and zero intervals are logged and skipped during registration.

---

# Unreleased: shared startup

**Builder and trait applications now share configuration policy, address resolution, binding,
initialization, and serving.** This changes the builder's default error policy and listener source.

## Listener address

Previously, builder `run()` selected its address before loading configuration and ignored
`config.server`. It now applies this priority:

| Priority | Source |
| --- | --- |
| 1 | Explicit `listen(...)` / `listen_on(...)` socket address |
| 2 | Builder `.host(...)` and `.port(...)`, independently |
| 3 | Loaded or explicitly supplied `ConfigWrapper::server` |
| 4 | Framework defaults from a missing server section |

An explicit `.port(3000)` overrides a configured 8080, even though 3000 is the framework default.
Setting only the port preserves the configured host. `run()` supports IPv4 and IPv6 literals;
use an unbracketed IP in `server.host` and a bracketed IPv6 socket address for `listen()`.
Hostname resolution is not provided.

Applications relying on the old builder behavior should explicitly call `.host("127.0.0.1")`
and `.port(3000)`, or pass an explicit listen address. `Gotcha::from_context` now honors its supplied
server settings as well.

## Configuration errors

Automatic loading and explicitly registered sources now both fail startup on loading errors unless
a fallback is explicitly selected. Optional files still ignore only missing files. Explicitly supplied
configuration still takes precedence over sources. To opt into fallback in the builder:

```rust
use gotcha::{ConfigErrorPolicy, Gotcha};
let app = Gotcha::new().config_error_policy(ConfigErrorPolicy::fallback_to_default());
```

For the trait API, add this method to your `GotchaApp` implementation when its config supports Default:

```rust,ignore
fn config_error_policy(&self) -> gotcha::ConfigErrorPolicy<Self::Config> {
    gotcha::ConfigErrorPolicy::fallback_to_default()
}
```

`ConfigErrorPolicy::Fallback(factory)` accepts a function returning `ConfigWrapper<C>` for custom
fallback values. Fallback logs the original error, replaces the whole configuration, and then applies
the normal address overrides. It applies to the configuration-loading step of startup, including a
custom trait `config()` error; it does not swallow address, state, router, task, or serve errors.
Eager `.build_config(...)` and direct `GotchaApp::config()` calls keep returning their errors.

`with_config::<C>()` and the config parameter of `with_types::<S, C>()` no longer require `Default`.
Only selecting `fallback_to_default()` needs `C: Default`; state default construction retains its
existing bound. Initialized contexts still require no serde or Default implementations.

## Initialization order

After the existing logging hook, startup loads configuration, selects and binds the address,
initializes state, assembles the router, registers tasks, and serves. This order is identical for
both APIs. A bind failure occurs before state construction or task registration; a later startup
error releases the listener. The initialized context's server settings contain the actual bound
address, including a dynamically assigned port, so state initialization and handlers agree.

State initialization now runs while the port is reserved, before requests are accepted. Applications
that depended on initializing state before a bind attempt must account for the new order. See
[owned scheduled tasks](#unreleased-owned-scheduled-tasks) for task startup and shutdown behavior.

---

# Unreleased: runtime type bounds

Initialized values no longer have to implement loading or default-construction traits just to
be stored in a context, extracted by handlers, or used by messages and scheduled tasks.

- Use `Gotcha::from_state(value)` for state without `Default` and automatic empty configuration.
- Use `Gotcha::from_context(GotchaContext { state, config })` when both values are already ready.
  State and application configuration only need `Clone + Send + Sync + 'static` for runtime use.
- `with_state::<S>()`, `with_config::<C>()`, and `with_types::<S, C>()` retain automatic initialization.
  Default state construction requires `S: Default`; configuration loading requires `Deserialize`.
  Creating state defaults and reading files still happen at startup, after any explicit overrides.
- `.state(...)`, `.config(...)`, route composition, task registration, and serving no longer carry
  unrelated `Default` or serde requirements. `.build_config(...)` requires `C: DeserializeOwned`.

`ConfigWrapper<T>` and `GotchaContext<S, C>` impose no bounds at their type definitions.
`ConfigWrapper`'s derived implementations request `Serialize`, `Deserialize`, `Clone`, or `Default`
only for the corresponding operation. `#[state]`, `#[config]`, and `FromRef` extraction no longer
impose loading bounds on the stored configuration or unrelated state type.

For the trait API, `GotchaApp::Config` only requires the runtime bounds. `build_router(context)`
can therefore accept an explicit configuration without serde or `Default`. `config()` and `run()`
require `Self::Config: DeserializeOwned`, because their contract includes configuration loading.
This method bound applies even when `config()` is overridden; use an explicit context if the
configuration cannot be deserialized. Generic callers of these methods must now state that bound
explicitly. `Serialize` and `Default` are no longer required by trait-based loading.

The existing `GotchaConfig` marker retains its original bounds for compatibility with user generic
code. Runtime containers and the extraction macros no longer require it.

Explicit configuration still wins over registered sources. See [shared startup](#unreleased-shared-startup)
for the unified strict loading policy, explicit fallback, and listener address precedence.

See the [initialized-values example](README.md#initialized-state-and-configuration).

---

# Unreleased: HTTP response contracts

**`Schematic` no longer implies `Responsible`.** A data schema does not determine how a value is
sent over HTTP. For automatic response inference, custom HTTP types implement `Responsible`
separately, alongside `IntoResponse`; they can still derive `Schematic`.
Ordinary JSON data should be returned as `Json<T>`.
The HTTP-specific `Schematic::empty_body` hook has been removed; `()` has its own response impl.

Built-in contracts now match the response representation:

| Return type | Documented response |
| --- | --- |
| `String`, `&str`, boxed/Cow strings | `200 text/plain` |
| `Json<T>` | `200 application/json`, using `T`'s schema |
| `Html<T>` | `200 text/html` |
| `Bytes`, `Vec<u8>`, byte slices, boxed/Cow bytes | `200 application/octet-stream`, binary string schema |
| `()` | `200`, no body; `Json<()>` remains JSON |
| `(StatusCode, T)` | `default`, with `T`'s response bodies; the status is a runtime value |
| `StatusCode` / raw `Response` | `default`, with no body schema inferred |

Media keys omit parameters such as `charset=utf-8`. Regenerate client/spec snapshots that
previously described text or binary responses as JSON.

Use `WithStatus<T, STATUS>` to keep a fixed status in the return type and the actual response:

```rust
use gotcha::{Json, WithStatus};

let created: WithStatus<_, 201> = WithStatus::new(Json("created"));
let no_content: WithStatus<_, 204> = WithStatus::new(());
```

This wrapper is available without the `openapi` feature. Statuses outside 100..=599 fail during
compilation when constructed. Informational statuses, 204, 205, and 304 discard bodies and body
headers. As with Axum's status tuple, the outer status overrides an inner error status too;
prefer `Result<WithStatus<Json<User>, 201>, E>` to keep `E`'s status independent.

**Automatic inference for `Result<T, E>` requires `Responsible` on both sides.** `ErrorResponsible`
and its schema blanket impl have been removed. Migrate custom errors to `Responsible`, use a fixed
HTTP type such as `WithStatus<Json<ApiError>, 409>`, or declare the endpoint's complete error contract
with `errors(...)` as shown below. A schema-only error no longer automatically adds `default`.
To retain a dynamic JSON error contract, implement `Responsible` using
`gotcha::response::default_response::<ApiError>("application/json", "Error")`.

For runtime status tuples or additional responses, declare the contract on the handler:

```rust,ignore
#[api(
    responses(
        response(status = 404, body = "ApiError", description = "Not found"),
        response(status = 409, body = "ApiError", description = "Duplicate")
    ),
    drop_default
)]
async fn lookup() -> Result<Json<User>, (StatusCode, Json<ApiError>)> {
    // Return the documented statuses here.
}
```

`body` names a **data type implementing `Schematic`**, not `Json<T>` or another HTTP wrapper.
`content_type` defaults to `application/json` when a body is present; set it explicitly for CSV
or other media. Omit `body` for an empty response. Duplicate statuses, invalid body types and
body declarations on bodyless statuses are rejected by the macro.

Explicit entries replace inference for their status only. `drop_default` removes the inferred
default, and at least one response must remain. These declarations change documentation only;
the handler remains responsible for sending the declared status and content type. Declared body
schemas participate in the same component collection and reference rewriting as inferred bodies.

For a `thiserror` enum that already implements `IntoResponse`, use `errors(...)` to describe all
errors for this endpoint without implementing `Responsible` on the enum:

```rust,ignore
#[api(errors(
    response(status = 404, body = "ErrorBody", description = "Not found"),
    response(status = 409, body = "ErrorBody", description = "Duplicate"),
    response(status = 500, body = "ErrorBody", description = "Internal error")
))]
async fn create_user() -> Result<Json<User>, ApiError> {
    // ApiError::into_response() sends the actual status and ErrorBody.
}
```

`ErrorBody` is the serialized data type and must implement `Schematic`; `ApiError` itself needs
neither `Schematic` nor `Responsible`. The success type still needs `Responsible`. Type aliases
such as `type ApiResult<T> = Result<T, ApiError>` work as well. See the
[README example](README.md#openapi-documentation) for a complete `thiserror` implementation.

`errors(...)` requires a `Result` return type and at least one response, using the same declaration
fields and validation as `responses(...)`. It completely replaces inference of the error branch,
even if the error type implements `Responsible`. Another endpoint using the same enum can declare
a different error set. The success contract is kept in full, including any dynamic `default`;
`drop_default` is not needed to suppress error inference.

Assembly order is: infer the success branch, merge the declared error alternatives, apply
`responses(...)` overrides, then apply an explicit `drop_default`. If success and error branches
share a status, their media types and schemas are combined using the same rules as ordinary
`Result` inference. With no `errors(...)`, existing inference and `responses(...)` behavior are
unchanged. None of these attributes changes runtime HTTP behavior.

Public helpers in `gotcha::response` support custom contracts: `response::<T>(status, media,
description)`, `empty_response(status, description)`, `default_response::<T>(media, description)`,
and `merge_responses(&mut target, other)`. The latter is also used by `Result`: duplicate statuses
retain media alternatives, differing schemas for one media type form `anyOf`, and identical
definitions are reused. Later header/link/example keys win. Distinct response-level `$ref`s
cannot be unioned and fail at assembly; use inline responses with schema-level references instead.

---

# Unreleased: application and route composition

**`Gotcha::nest` and `Gotcha::merge` now accept `GotchaRouter<GotchaContext<S, C>>`, not another
`Gotcha<S, C>`.** Previously they extracted only the child's router and silently discarded its
state, configuration and sources, listening address, and background task registrations.

Replace route-only child builders with route modules:

```rust
use gotcha::{Gotcha, GotchaRouter};

let api = GotchaRouter::default().get("/items", || async { "items" });
let health = GotchaRouter::default().get("/health", || async { "ok" });
let app = Gotcha::new().nest("/api", api).merge(health);
```

Route modules share the application's context type and receive its resolved state and config.
They retain their routes, middleware, and OpenAPI metadata/transforms. The existing nested path
and child-before-parent transform rules continue to apply.

Move child `.state(...)`, `.config(...)`, configuration-source calls, `.host(...)`, `.port(...)`,
and `.tasks(...)` registrations to the top-level application deliberately. If multiple modules
need tasks, register each module's task setup with that application's `.tasks(...)`; route modules
do not own a scheduler. There is no automatic merge of competing application settings or tasks,
and no conversion that extracts routes while discarding the rest of an application.

Full child applications now fail to compile at `nest`/`merge`, including children with task
registrations. Applications that need independent state/configuration and lifecycles should be
started independently. To reuse a trait application's `routes` method, supply a router with the
same context type; doing so reuses only routes, not its `state`, `tasks`, or startup hooks.

---

# Unreleased: documented method routers

**`Gotcha::route` and `GotchaRouter::route` now require `MethodRouter`.** This type carries
HTTP handlers and their OpenAPI descriptors together. Previously, passing an Axum `MethodRouter`
silently omitted even `#[api]` handlers from the generated document.

For documented composition, use Gotcha's method constructors:

```rust
use gotcha::{routing::get, GotchaRouter};

// With the openapi feature, annotate these handlers with #[gotcha::api].
async fn list() -> String { "items".into() }
async fn create() -> String { "created".into() }

let router: GotchaRouter<()> = GotchaRouter::default()
    .route("/items", get(list).post(create));
```

`gotcha::get/post/...` and the corresponding prelude exports now return this Gotcha type too.
When using a plain Axum `Router`, import constructors from `axum::routing` explicitly.
Existing `.get(path, handler)` / `.post(path, handler)` shortcuts keep their signatures and use
the same registration path as `.route(...)`.

To preserve native Axum behavior, change `.route(...)` to `.route_raw(...)` on either Gotcha API:

```rust
use gotcha::{axum, GotchaRouter};

let router: GotchaRouter<()> = GotchaRouter::default()
    .route_raw("/health", axum::routing::get(|| async { "ok" }));
```

Raw routes generate no operations, including for annotated handlers. No implicit conversion
between Axum and Gotcha method routers is provided. Use raw routes for native services or other
Axum-specific method-router APIs.

Gotcha method routers support method chaining, `on`, `merge`, `clone`, and `layer`, retaining
their descriptors through each operation. Components are still collected once for the complete
document. Unannotated handlers run normally and remain undocumented; middleware does not infer
response schemas or security requirements. Overlapping handlers follow Axum's conflict rules.

Combined filters such as `routing::on(MethodFilter::GET.or(MethodFilter::POST), handler)` now
document every selected OpenAPI method. Implicit HEAD handling for GET is unchanged and does
not add a HEAD operation; explicitly register HEAD to document it. CONNECT remains executable
but is not represented in OpenAPI 3.0.

---

# Unreleased: OpenAPI transform composition

Repeated `.openapi(...)` calls now append callbacks instead of replacing the previous callback.
`nest` and `merge` retain child callbacks instead of silently dropping them. Both `Gotcha` and
`GotchaRouter` use these rules:

1. Child subtrees run in `nest`/`merge` insertion order, with descendants before their parent.
2. The current router's own callbacks then run in registration order, even if registered before
   its children. The receiver of `merge` is the parent; regrouping merges can change precedence.
3. Each callback runs once during router assembly, on the complete document with final prefixed
   paths and collected components. Requests to `/openapi.json` reuse that result.

**Previously discarded callbacks now execute.** Review callbacks that overwrite metadata or have
side effects. Parent callbacks can explicitly override child edits. Later assignments win; there
is no implicit merge of `info`, component maps, or security lists. Mutate individual map entries
to preserve unrelated definitions, or explicitly replace a field to reset it.

**Child callbacks configure the whole document.** Top-level `security` applies globally, including
routes outside that child. For local requirements, edit `security` on specific operations using
their final prefixed paths. Moving a callback into a nested router does not make it route-scoped.

---

# Unreleased: nested OpenAPI paths

`GotchaRouter::nest` now records OpenAPI paths using the same join rules as the underlying
Axum router. For example, nesting `/hello` under `/api` documents `/api/hello`, rather than the
incorrect `/api//hello`. Regenerate clients/spec snapshots that previously contained these
incorrect paths; the application's actual HTTP routes and registration API are unchanged.

A child root `/` documents `/api` when nested under `/api`, and `/api/` when nested under
`/api/`. Trailing slashes on non-root child routes and intentional interior double slashes
remain significant. The same rules apply at each level of nesting and preserve path parameters.

---

# Unreleased: schema identity and component names

Schema collection now identifies derived types by their full Rust type name, including generic
arguments, instead of treating the display name as the type's identity. Different types no longer
silently share the first schema encountered. Operations are assembled in path/method order;
component names are resolved after collection, independently of registration order.

| Rust types in one document | Component names |
| --- | --- |
| A unique `User` | `User` |
| `Envelope<String>`, `Envelope<u32>` | `Envelope_String`, `Envelope_u32` |
| `screens::SendResult`, `terminals::SendResult` | `ScreensSendResult`, `TerminalsSendResult` |
| `first::models::Item`, `second::models::Item` | `FirstModelsItem`, `SecondModelsItem` |

All generated references, including recursive and nested references, follow the resolved names.
Each name collision emits a `tracing::warn!` with `schema_name`, `types`, and `resolved_names`.
Repeated use of the same type is normal reuse and does not warn. Add a tracing subscriber to
observe these events, as for other framework logs.

**Component names and generated client type names may change.** Unique short names remain as
before, but adding a colliding type can rename a previously unique component. Reserve stable
public names explicitly:

```rust
use gotcha::Schematic;

#[derive(Schematic)]
#[schematic(name = "ScreenSendResult")]
struct SendResult { screen_id: String }
```

The override works for structs, all supported enum representations, and newtypes. A named newtype
becomes its own component; an unnamed newtype remains transparent. An explicit override also
changes `Schematic::name()`. Names must match `[A-Za-z0-9._-]+`.

A unique explicit name takes priority over an automatically generated name. Duplicate explicit
names warn and are disambiguated, rather than dropping either schema. Prefer unique overrides,
including across generic instances that share a derive. If module prefixes still collide (for
example when generic arguments have identical short names), an encoded full type identity is
used as a deterministic fallback. Use explicit names for such public types rather than relying
on these long fallback names. Automatic identity/naming is not a cross-compiler ABI; explicit
unique names are the mechanism for pinning a public contract.

**Low-level `registry::collect` results now require `SchemaReferences`.** The collector must
rewrite references returned by the closure as well as references in component definitions after
assigning names. Standard schema/operation results, vectors, maps, and parameter-provider results
already implement this trait. Custom result containers should forward it to their schema-bearing
fields; unrelated application state does not need serialization or new bounds. Temporary
references inside the closure are not finalized until `collect` returns.

Handwritten schema implementations can use `registry::schema_or_ref_for::<Self>` with an optional
explicit name and `module_path!()` to participate in type-aware collection. The older
`schema_or_ref(name, ...)` helper still treats its caller-supplied name as an explicit identity;
callers of that helper remain responsible for making those identities unique.

---

# Unreleased: ordered configuration sources

Configuration sources now retain their insertion order when cloned or restored. Later sources
override earlier matching values, including when files and environment prefixes are interleaved:

```rust
use gotcha::config::ConfigBuilder;

let builder = ConfigBuilder::new()
    .file("base.toml")
    .env("APP")
    .file_optional("local.toml");
let restored = ConfigBuilder::from_state(builder.state());
```

Here `APP` overrides `base.toml`, and `local.toml` overrides both. Previously restoration grouped
all environment sources before all files, changing precedence. `Gotcha::with_file_config`,
`with_optional_config`, `with_env_config`, and the default-source helpers use this same ordering.
To let environment values win, add their source after the files.

**Direct `ConfigState` construction is a source-breaking change.** Replace `file_paths` and
`env_prefixes` with the ordered `sources` list, specifying which files are required:

```rust
use gotcha::config::{ConfigSource, ConfigState};

let state = ConfigState {
    sources: vec![
        ConfigSource::File { path: "base.toml".into(), required: true },
        ConfigSource::Env { prefix: "APP".into() },
        ConfigSource::File { path: "local.toml".into(), required: false },
    ],
    enable_vars: true,
};
```

The chainable builder methods keep their signatures. State is a description of where to load
configuration, not a snapshot: files and environment values are read at `build()` time. A file
created after registration is now included, and a required file removed before loading fails.
An optional file only ignores `NotFound`; invalid TOML, invalid UTF-8 and other read errors fail.

**Explicit configuration-source failures now stop Builder startup.** Calls such as
`with_file_config`, `with_optional_config`, and `with_default_config` propagate loading errors
through `run()` / `listen()` instead of replacing the configuration with `Default`. Use
`with_optional_config` for a file whose absence is acceptable. The conventional files selected
by `with_default_files` are still optional, but errors in existing files propagate.

An already supplied `.config(...)` value still takes precedence over accumulated sources.
Automatic loading now follows the same strict default policy; see [shared startup](#unreleased-shared-startup)
for explicitly opting into fallback.

---

# 0.3 → 0.4

Two changes require edits in every application: **route paths** and the **configuration file**. Everything else is a smaller adjustment.

## 1. Route paths use `{name}`, not `:name`

Gotcha now builds on axum 0.8, which changed how a captured path segment is written — and rejects the old form outright rather than silently treating it as a literal:

```rust,ignore
// before                                  // after
router.get("/users/:id", get_user)         router.get("/users/{id}", get_user)
router.get("/f/*rest", serve)              router.get("/f/{*rest}", serve)
```

A path that still starts a segment with `:` fails at startup with *"Path segments must not start with `:`. For capture groups, use `{capture}`"*.

This is the syntax OpenAPI already used, so gotcha no longer translates between the two — a route is documented exactly as it is registered.

## 2. Configuration files: application settings move to the top level

The framework's own settings now live in a reserved `[server]` section, and your application's settings are the top level of the file — they used to be nested under `[application]` while `[basic]` took the top spot.

```toml
# before (0.3)                    # after (0.4)
[basic]                           name = "my-app"
host = "127.0.0.1"                database_url = "postgres://localhost/app"
port = 8080
                                  [server]
[application]                     host = "127.0.0.1"
name = "my-app"                   port = 8080
database_url = "postgres://..."
```

Both profile files (`application.toml` and `application_{profile}.toml`) need the same treatment.

## 3. Reading configuration in code

```rust,ignore
// before                         // after
config.application.name           config.name
config.basic.port                 config.server.port
```

`ConfigWrapper<T>` now dereferences to your own config type, which is what makes `config.name` work.

Handlers can skip the wrapper entirely. Annotate your config type with `#[config]` and extract it directly:

```rust,ignore
#[config]
#[derive(Clone, Default, Serialize, Deserialize)]
struct Config {
    name: String,
}

async fn handler(State(config): State<Config>) -> impl Responder {
    config.name.clone()
}
```

The bind settings are their own extractor, `State<ServerConfig>`. `State<ConfigWrapper<Config>>` still works if you want both at once.

## 4. Environment overrides use `__` between sections

Nested paths are separated by a **double** underscore, which leaves single underscores free for snake_case field names:

```console
# before (never actually worked for typed fields — it failed the whole load)
APP_SERVER_PORT=8080

# after
APP_SERVER__PORT=8080     # -> [server] port
APP_DATABASE_URL=...      # -> the top-level `database_url` field
```

Typed fields (numbers, booleans) are now parsed from the environment string instead of failing to merge.

## 5. The `message` feature is gone

The message system is always available. Drop it from your feature list:

```toml
# before
gotcha = { version = "0.3", features = ["openapi", "message"] }
# after
gotcha = { version = "0.4", features = ["openapi"] }
```

`cors` and `static_files` keep their names, but each now enables only its own half of `tower-http` — a CORS-only application no longer compiles the static-file stack.

## 6. Smaller changes

- **Validation rejections return `422`**, not `400`. `400` is still used for a malformed body. Every error now carries a readable `message`.
- **`Result<T, E>` handlers** require `Responsible` on both sides for automatic inference, or can use `#[api(errors(...))]` for explicit error documentation; see [HTTP response contracts](#unreleased-http-response-contracts) for migration.
- **Handlers returning nothing** now compile (they previously failed with `E0782`) and document an empty body.
- **`Operable`** gained `summary` and `security` fields; only relevant if you construct it by hand rather than through `#[api]`.
- **axum 0.8** also removed `#[async_trait]` from its extractor traits. A hand-written `FromRequest` / `FromRequestParts` impl should drop the attribute and use a plain `async fn`.
- **New re-exports**, so these no longer need `gotcha::axum::…`: `Form`, `Multipart`, `Sse` / `Event` / `KeepAlive`, `WebSocketUpgrade` / `WebSocket`, `middleware`, `MatchedPath`, `OriginalUri`. `GotchaRouter` also gained `fallback_service`.

---

# 0.2 → 0.3: API simplification

This section helps you migrate from the traditional trait-based API to the simplified builder API introduced in Gotcha v0.3.0. Note that the configuration examples below use the 0.3 layout — see the 0.4 section above for the current one.

## TL;DR

- **New projects**: Use the new `gotcha::prelude::*` and builder API
- **Existing projects**: Continue working without changes, migrate at your own pace
- **Both APIs**: Can be used together in the same project

## Overview of Changes

### New Builder API Benefits

✅ **Simplified setup** - No struct definitions or trait implementations required
✅ **Inline handlers** - Define handlers as closures directly in route definitions  
✅ **Fluent interface** - Chain method calls for readable code
✅ **Reduced boilerplate** - 90% less code for simple applications
✅ **Better beginner experience** - Start building APIs immediately
✅ **Full backward compatibility** - Existing code continues to work

## Migration Examples

### Example 1: Simple Hello World

#### Before (v0.2.x)
```rust
use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, State, Responder};
use serde::{Deserialize, Serialize};

pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    pub name: String,
}

pub struct App {}

impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) 
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/", hello_world)
    }

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) 
        -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}
```

#### After (v0.3.x)
```rust
use gotcha::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::new()
        .get("/", || async { "hello world" })
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}
```

**Lines of code**: 35 → 8 (77% reduction)

### Example 2: JSON API with Path Parameters

#### Before
```rust
use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Json, Path, State, Responder};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct User {
    id: u32,
    name: String,
}

pub async fn get_user(Path(id): Path<u32>) -> impl Responder {
    Json(User { id, name: format!("User {}", id) })
}

pub async fn create_user(Json(user): Json<User>) -> impl Responder {
    Json(user)
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {}

pub struct App {}

impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) 
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
            .get("/users/{id}", get_user)
            .post("/users", create_user)
    }

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) 
        -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}
```

#### After
```rust
use gotcha::prelude::*;

#[derive(Serialize, Deserialize)]
pub struct User {
    id: u32,
    name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::new()
        .get("/users/{id}", |Path(id): Path<u32>| async move {
            Json(User { id, name: format!("User {}", id) })
        })
        .post("/users", |Json(user): Json<User>| async move {
            Json(user)
        })
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}
```

### Example 3: Reusing Route Modules

Extract reusable routes into a function whose context type matches the hosting application:

```rust
use gotcha::prelude::*;

type Context = GotchaContext<EmptyState, EmptyConfig>;

fn api_routes() -> GotchaRouter<Context> {
    GotchaRouter::default().get("/items", || async { "items" })
}

let app = Gotcha::new()
    .get("/health", || async { "ok" })
    .nest("/api/v1", api_routes());
```

A trait-based application with the same context type can also include `api_routes()` in its
`routes` method. In either case, the hosting application provides state, configuration and tasks;
the route module is not an independently initialized application.

## Migration Strategies

### Strategy 1: Fresh Start (Recommended for New Projects)
- Start new projects with `use gotcha::prelude::*`
- Use builder API for all new code
- Reference the `/examples/simple/` for patterns

### Strategy 2: Gradual Migration (Existing Projects)
1. **Keep existing code** - No changes needed immediately
2. **Add new routes** using builder API when convenient
3. **Refactor incrementally** during feature updates
4. **No rush** - both APIs will be supported long-term

### Strategy 3: Side-by-Side (Large Projects)
- Use trait API for complex features (state management, tasks, etc.)
- Use builder API for simple endpoints and utilities
- Mix approaches based on complexity needs

## Feature Comparison

| Feature | Trait API | Builder API | Notes |
|---------|-----------|-------------|-------|
| Simple routes | ❌ Complex | ✅ Easy | Builder API much simpler |
| State management | ✅ Full support | ⚠️ Basic | Trait API better for complex state |
| Configuration | ✅ Full control | ✅ Smart defaults | Both supported |
| Task scheduling | ✅ Integrated | ❌ Not available | Use trait API for background tasks |
| Middleware | ✅ Full control | ✅ Simplified | Both approaches work |
| Testing | ✅ Full control | ✅ Simplified | Builder API easier to test |
| OpenAPI | ✅ Full support | ✅ Auto-enabled | Both generate documentation |

## When to Use Which API

### Use Builder API When:
- ✅ Creating simple web services or APIs
- ✅ Prototyping or learning
- ✅ Most routes don't need complex state
- ✅ You want minimal boilerplate
- ✅ Building REST APIs with standard patterns

### Use Trait API When:
- ✅ Complex application state management needed
- ✅ Background task scheduling required
- ✅ Custom configuration loading logic
- ✅ Advanced lifecycle hooks needed
- ✅ Large applications with multiple modules

### Use Both When:
- ✅ Migrating existing applications
- ✅ Different complexity needs in the same app
- ✅ Team has mixed experience levels

## Import Changes

### Before
```rust
use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Json, Path, State, Responder};
use serde::{Deserialize, Serialize};
```

### After
```rust
use gotcha::prelude::*;
// This includes all commonly used types:
// Gotcha, Json, Path, State, Responder, StatusCode, etc.
```

## Configuration Changes

### Simple Configuration (New)
```rust
Gotcha::new()
    .host("0.0.0.0")
    .port(8080)
    .with_cors()
    .with_openapi()
```

### Advanced Configuration (Existing)
```rust
// Still works exactly the same
impl GotchaApp for App {
    async fn config(&self) -> Result<ConfigWrapper<Self::Config>, Box<dyn std::error::Error>> {
        // Custom config loading
    }
}
```

## Common Patterns

### Error Handling
```rust
// Simple error responses
.get("/might-fail", || async {
    if some_condition {
        Ok("Success")
    } else {
        Err("Something went wrong")
    }
})

// Custom status codes
.get("/not-found", || async {
    (StatusCode::NOT_FOUND, "Resource not found")
})
```

### JSON Responses
```rust
// Simple JSON
.get("/json", || async {
    Json(json!({"message": "Hello"}))
})

// Structured responses
.get("/user/{id}", |Path(id): Path<u32>| async move {
    let user = User { id, name: "John" };
    Json(user)
})
```

### Multiple HTTP Methods
```rust
// Same path, different methods
.route("/resource", 
    get(get_handler)
    .post(create_handler)
    .put(update_handler)
    .delete(delete_handler)
)
```

## Compatibility Promise

- **No breaking changes** - Existing trait-based code continues to work
- **Long-term support** - Both APIs will be maintained
- **Feature parity** - New features will support both APIs where possible
- **Migration tools** - Additional tooling may be provided in future versions

## Need Help?

- 📖 Check `/examples/simple/` for comprehensive examples
- 🐛 File issues on GitHub for migration problems
- 💬 Join discussions for migration questions
- 📚 Read the updated documentation at [gotcha.rs](https://gotcha.rs)

---

**Happy migrating! 🦀✨**
