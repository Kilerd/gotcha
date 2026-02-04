//! Real-world example showing how the improved Gotcha API makes it easier
//! to work with custom state and configuration types.

use gotcha::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use std::collections::HashMap;

// Application state with a counter and a simple in-memory store
#[derive(Clone, Default)]
struct AppState {
    request_counter: Arc<AtomicU64>,
    users: Arc<RwLock<HashMap<u64, User>>>,
}

// Application configuration
#[derive(Clone, Serialize, Deserialize)]
struct AppConfig {
    api_key: String,
    max_users: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: "demo-key".to_string(),
            max_users: 100,
        }
    }
}

// Domain model
#[derive(Clone, Serialize, Deserialize, Debug)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🚀 Starting Real-World Gotcha Example with Custom Types");
    println!();

    // Initialize state with some sample data
    let mut initial_users = HashMap::new();
    initial_users.insert(1, User {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    });
    initial_users.insert(2, User {
        id: 2,
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
    });

    let state = AppState {
        request_counter: Arc::new(AtomicU64::new(0)),
        users: Arc::new(RwLock::new(initial_users)),
    };

    // ✨ The NEW clean API - No more Gotcha::<(), ()>::with_types!
    let app = Gotcha::with_types::<AppState, AppConfig>()
        .state(state)
        .with_env_config("APP")

        // Home route with request counter
        .get("/", |ctx: State<GotchaContext<AppState, AppConfig>>| async move {
            let count = ctx.state.request_counter.fetch_add(1, Ordering::SeqCst);
            format!("Welcome to Gotcha! This is request #{}", count + 1)
        })

        // Get all users
        .get("/users", |ctx: State<GotchaContext<AppState, AppConfig>>| async move {
            let users = ctx.state.users.read().await;
            let users_vec: Vec<&User> = users.values().collect();
            Json(users_vec)
        })

        // Get user by ID
        .get("/users/:id", |
            Path(id): Path<u64>,
            ctx: State<GotchaContext<AppState, AppConfig>>
        | async move {
            let users = ctx.state.users.read().await;
            match users.get(&id) {
                Some(user) => Ok(Json(user.clone())),
                None => Err((StatusCode::NOT_FOUND, "User not found"))
            }
        })

        // Create new user
        .post("/users", || async {
            Json(serde_json::json!({
                "message": "User creation endpoint (simplified for demo)"
            }))
        })

        // Show configuration
        .get("/config", |ctx: State<GotchaContext<AppState, AppConfig>>| async move {
            Json(serde_json::json!({
                "api_key": if ctx.config.application.api_key.is_empty() {
                    "not-configured"
                } else {
                    "***hidden***"
                },
                "max_users": ctx.config.application.max_users
            }))
        })

        // Health check
        .get("/health", || async {
            Json(serde_json::json!({
                "status": "healthy",
                "service": "gotcha-example"
            }))
        });

    println!("✅ API endpoints:");
    println!("  GET  /        - Home with request counter");
    println!("  GET  /users   - List all users");
    println!("  GET  /users/:id - Get user by ID");
    println!("  POST /users   - Create new user");
    println!("  GET  /config  - Show configuration");
    println!("  GET  /health  - Health check");
    println!();
    println!("🌐 Server running at http://127.0.0.1:3000");
    println!();
    println!("💡 Try:");
    println!("  curl http://127.0.0.1:3000/users");
    println!("  curl -X POST http://127.0.0.1:3000/users -H 'Content-Type: application/json' -d '{{\"name\":\"Charlie\",\"email\":\"charlie@example.com\"}}'");
    println!();

    app.listen("127.0.0.1:3000").await?;

    Ok(())
}