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
- 📁 **Static Files** - Serve static content effortlessly
- ⏰ **Task Scheduling** - Cron and interval-based background tasks
- 💌 **Message System** - Built-in inter-service communication
- ⚙️ **Smart Configuration** - Environment-based config with variable resolution
- 🏗️ **Two APIs** - Choose between simple builder API or advanced trait-based API

## 🚀 Quick Start

### Simple Builder API (Recommended for new projects)

```rust
use gotcha::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::new()
        .get("/", || async { "Hello World" })
        .get("/hello/:name", |Path(name): Path<String>| async move {
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

```rust
use gotcha::{async_trait, ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, State};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
}

pub struct App {}

#[async_trait]
impl GotchaApp for App {
    type State = AppState;
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) 
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
            .get("/", hello_world)
            .get("/users/:id", get_user)
    }

    async fn state(&self, config: &ConfigWrapper<Self::Config>) 
        -> Result<Self::State, Box<dyn std::error::Error>> {
        // Initialize database connections, etc.
        Ok(AppState::new(&config.database_url).await?)
    }
}

async fn hello_world(_state: State<ConfigWrapper<Config>>) -> &'static str {
    "Hello World"
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
gotcha = "0.2"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
```

### Optional Features

Enable additional features as needed:

```toml
[dependencies]
gotcha = { version = "0.2", features = ["openapi", "prometheus", "cors", "static_files", "task", "message"] }
```

Available features:
- `openapi` - Automatic OpenAPI/Swagger documentation
- `prometheus` - Metrics collection and exposition
- `cors` - Cross-Origin Resource Sharing support  
- `static_files` - Static file serving capabilities
- `task` - Background task scheduling with cron support
- `message` - Built-in message passing system
- `cloudflare_worker` - Cloudflare Worker runtime support

## 📖 Documentation & Examples

### OpenAPI Documentation

With the `openapi` feature enabled, use the `#[api]` macro for automatic documentation:

```rust
use gotcha::{api, Json, Path, Schematic};

#[derive(Schematic, Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

/// Get user by ID
#[api(id = "get_user", group = "users")]
async fn get_user(Path(id): Path<u32>) -> Json<User> {
    // Implementation here
}
```

Visit these endpoints when running:
- `/redoc` - ReDoc documentation interface
- `/scalar` - Scalar documentation interface  
- `/openapi.json` - Raw OpenAPI specification

### Configuration System

Create a `configurations/application.toml` file:

```toml
[basic]
host = "127.0.0.1"
port = 3000

[application]
database_url = "${DATABASE_URL}"
api_key = "${API_KEY}"
app_name = "My Gotcha App"
```

Configuration supports:
- Environment variable resolution: `${ENV_VAR}`
- Path variable resolution: `${app.database.name}`
- Profile-based overrides via `GOTCHA_ACTIVE_PROFILE` environment variable

### Task Scheduling

```rust
use gotcha::{GotchaApp, task::TaskScheduler};

#[async_trait]
impl GotchaApp for App {
    // ... other implementations

    async fn tasks(&self) -> Vec<TaskScheduler> {
        vec![
            TaskScheduler::new("cleanup", "0 2 * * *", || async {
                // Daily cleanup at 2 AM
                println!("Running cleanup task");
            }),
            TaskScheduler::interval("heartbeat", Duration::from_secs(30), || async {
                // Every 30 seconds
                println!("Heartbeat");
            }),
        ]
    }
}
```

## 🏗️ Architecture

Gotcha is organized as a Rust workspace with the following structure:

```
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

# Test Cloudflare Worker features
python3 test-feature-matrix.py echo-cf-worker
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

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Related Projects

- [Axum](https://github.com/tokio-rs/axum) - The underlying web framework
- [mofa](https://crates.io/crates/mofa) - Configuration management
- [oas](https://crates.io/crates/oas) - OpenAPI schema generation