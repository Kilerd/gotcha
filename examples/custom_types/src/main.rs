use gotcha::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// Custom application state
#[derive(Clone, Default)]
struct AppState {
    request_counter: Arc<AtomicU64>,
}

// Custom application configuration
#[derive(Clone, Default, Serialize, Deserialize)]
struct AppConfig {
    api_key: String,
    max_connections: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🚀 Demonstrating new Gotcha API improvements");
    println!();

    // The NEW way - clean and simple!
    println!("✨ NEW API - No more <(), ()> needed!");
    println!("  Gotcha::with_types::<AppState, AppConfig>()");
    println!("  Gotcha::with_state::<AppState>()");
    println!("  Gotcha::with_config::<AppConfig>()");

    // Example 1: Using with_types for both state and config
    let _app1 = Gotcha::with_types::<AppState, AppConfig>()
        .state(AppState::default())
        .get("/", || async { "App with custom state and config" });

    // Example 2: Using with_state for custom state only
    let _app2 = Gotcha::with_state::<AppState>()
        .state(AppState::default())
        .get("/", || async { "App with custom state" });

    // Example 3: Using with_config for custom config only
    let _app3 = Gotcha::with_config::<AppConfig>()
        .with_env_config("APP")
        .get("/", || async { "App with custom config" });

    // Example 4: Traditional approach for simple apps still works
    let _app4 = Gotcha::new()
        .get("/", || async { "Simple app" });

    println!();
    println!("✅ All API variations compile successfully!");
    println!();
    println!("💡 The old way would have required:");
    println!("  Gotcha::<(), ()>::with_types::<AppState, AppConfig>() // 😕 Awkward!");

    Ok(())
}