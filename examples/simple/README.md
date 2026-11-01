# Simple Gotcha Example

This example demonstrates the new simplified Gotcha API that allows you to create web applications with minimal boilerplate code.

## Features Demonstrated

- ✨ **Simple route definitions** with closures and inline handlers
- 🛣️ **Path parameters** with automatic type conversion
- 📦 **JSON APIs** with automatic serialization/deserialization
- 📚 **OpenAPI documentation** for `#[api]` handlers (visit `/scalar`); closure routes are not
  documented automatically
- 🌐 **CORS support** with `with_cors()`
- 🔧 **Health checks** and error handling
- 🏗️ **Nested routes** and modular organization

## Running the Example

```bash
cargo run
```

Then visit:
- **Main app**: http://localhost:3000
- **API Documentation**: http://localhost:3000/scalar
- **OpenAPI Spec**: http://localhost:3000/openapi.json
- **Health Check**: http://localhost:3000/health

## Key Differences from Traditional API

### Old Way (Trait-based)
```rust
pub struct App {}

impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) 
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/", hello_world)
    }

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) 
        -> Result<Self::State, GotchaError> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}
```

### New Way (Builder API)
```rust
use gotcha::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::new()
        .get("/", || async { "Hello World" })
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}
```

## Available Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Welcome message |
| GET | `/health` | Health check with JSON response |
| GET | `/hello/{name}` | Greeting with path parameter |
| GET | `/users/{id}` | Get user by ID (mock data) |
| POST | `/users` | Create new user |
| GET | `/api/v1/ping` | Simple ping endpoint |
| GET | `/api/v1/time` | Current timestamp |
| GET | `/error` | Example error response |
| GET | `/teapot` | Custom status code example |
| GET | `/html` | HTML response example |

## Code Highlights

### Inline Closures
```rust
.get("/", || async { "Hello World" })
.get("/health", || async {
    Json(serde_json::json!({
        "status": "ok",
        "timestamp": "2024-01-01T00:00:00Z"
    }))
})
```

### Path Parameters
```rust
.get("/hello/{name}", |Path(name): Path<String>| async move {
    format!("👋 Hello, {}! Welcome to Gotcha!", name)
})
```

### JSON APIs
```rust
.post("/users", |Json(req): Json<CreateUserRequest>| async move {
    let user = User {
        id: 42,
        name: req.name,
        email: req.email,
    };
    (StatusCode::CREATED, Json(user))
})
```

### Nested Routes
```rust
.routes(|router| {
    router
        .get("/api/v1/ping", || async { "pong" })
        .get("/api/v1/time", || async { "2024-01-01T00:00:00Z" })
})
```

### Error Handling
```rust
.get("/error", || async {
    Err::<String, _>("Something went wrong!")
})
```

## Features Enabled

This example uses the following Gotcha features:
- `openapi` - OpenAPI documentation for `#[api]` handlers
- `cors` - Cross-origin resource sharing support

## Migration from Traditional API

The new builder API is fully compatible with the existing trait-based API. You can:

1. **Start fresh** with the new API for new projects
2. **Gradually migrate** existing projects by adding new routes with the builder API
3. **Mix both approaches** in the same application

The traditional `GotchaApp` trait remains available for complex applications that need:
- Custom state management
- Complex configuration
- Advanced lifecycle hooks
- Task scheduling integration