# Migration Guide

- [Unreleased: documented method routers](#unreleased-documented-method-routers)
- [Unreleased: OpenAPI transform composition](#unreleased-openapi-transform-composition)
- [Unreleased: nested OpenAPI paths](#unreleased-nested-openapi-paths)
- [Unreleased: schema identity and component names](#unreleased-schema-identity-and-component-names)
- [Unreleased: ordered configuration sources](#unreleased-ordered-configuration-sources)
- [0.3 → 0.4](#03--04) — **every application must edit its route paths and configuration file**
- [0.2 → 0.3: API simplification](#02--03-api-simplification)

---

# Unreleased: documented method routers

**`Gotcha::route` and `GotchaRouter::route` now require `GotchaMethodRouter`.** This type carries
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
With no explicit configuration or sources, automatic default loading retains its existing
warning-and-fallback behavior. A broader unification of startup policies is tracked separately
in [#92](https://github.com/Kilerd/gotcha/issues/92).

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
- **`Result<T, E>` handlers** need `E: ErrorResponsible`. This is implemented for any `E: Schematic` and for axum's `(StatusCode, Json<E>)` idiom, so most code needs no change.
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

### Example 3: Mixed Approach (Gradual Migration)

You can use both APIs in the same application:

```rust
use gotcha::prelude::*;

// Existing trait-based app (unchanged)
pub struct ApiV1 {}

impl GotchaApp for ApiV1 {
    type State = DatabasePool;
    type Config = ApiConfig;
    
    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) 
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
            .get("/api/v1/complex", complex_handler)
            .post("/api/v1/process", process_handler)
    }
    
    async fn state(&self, config: &ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
        DatabasePool::connect(&config.application.database_url).await
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Start with new builder API
    let builder = Gotcha::new()
        // Simple routes using new API
        .get("/", || async { "Welcome!" })
        .get("/health", || async { 
            Json(json!({"status": "ok"})) 
        });
    
    // Nest the existing trait-based app
    let api_v1 = ApiV1 {};
    let v1_router = /* build ApiV1 router and extract it */;
    
    builder
        .nest("/", v1_router)
        .listen("127.0.0.1:3000")
        .await?;
        
    Ok(())
}
```

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
