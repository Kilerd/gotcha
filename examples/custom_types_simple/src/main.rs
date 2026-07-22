//! Simplified example showcasing the new Gotcha API improvements

use gotcha::prelude::*;
use serde::{Deserialize, Serialize};

// Custom application state
#[derive(Clone, Default)]
#[allow(dead_code)]
struct MyState {
    name: String,
}

// Custom application configuration
#[derive(Clone, Default, Serialize, Deserialize)]
struct MyConfig {
    api_key: String,
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🚀 Gotcha API Improvements Demo");
    println!("================================");
    println!();

    // ✨ The OLD way (before the improvement) would have required:
    // Gotcha::<(), ()>::with_types::<MyState, MyConfig>()  // 😕 Awkward!

    // ✨ The NEW way - clean and intuitive:
    println!("✅ NEW API - Direct and clean:");
    println!();
    println!("  // For custom state and config:");
    println!("  Gotcha::with_types::<MyState, MyConfig>()");
    println!();
    println!("  // For custom state only:");
    println!("  Gotcha::with_state::<MyState>()");
    println!();
    println!("  // For custom config only:");
    println!("  Gotcha::with_config::<MyConfig>()");
    println!();
    println!("  // For simple apps (unchanged):");
    println!("  Gotcha::new()");
    println!();

    // Test that all variations compile correctly
    let _app1 = Gotcha::with_types::<MyState, MyConfig>()
        .state(MyState { name: "Example".to_string() })
        .get("/", || async { "Custom state and config" });

    let _app2 = Gotcha::with_state::<MyState>()
        .state(MyState { name: "Example".to_string() })
        .get("/", || async { "Custom state only" });

    let _app3 = Gotcha::with_config::<MyConfig>().get("/", || async { "Custom config only" });

    let _app4 = Gotcha::new().get("/", || async { "Simple app" });

    println!("✨ Benefits of the new API:");
    println!("  • No more confusing <(), ()> type parameters");
    println!("  • Cleaner, more intuitive syntax");
    println!("  • Better developer experience");
    println!("  • Same powerful functionality");
    println!();
    println!("✅ All API variations compile successfully!");

    Ok(())
}
