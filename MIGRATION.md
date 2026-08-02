# Migration Guide

- [0.3 → 0.4](#03--04) — **every application must edit its route paths and configuration file**
- [0.2 → 0.3: API simplification](#02--03-api-simplification)

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