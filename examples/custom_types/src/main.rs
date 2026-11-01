//! Shows the constructors for applications with custom state and configuration types.
//!
//! - [`Gotcha::from_state`] takes state that is already initialized; it is the right choice for
//!   pools, clients, and other values without a meaningful `Default`.
//! - [`Gotcha::from_context`] takes both the state and the configuration, without loading a file.
//! - [`Gotcha::with_state`], [`Gotcha::with_config`], and [`Gotcha::with_types`] construct the
//!   values automatically and load configuration from `configurations/` and the environment.
//! - [`Gotcha::new`] is the same as `with_types::<EmptyState, EmptyConfig>()`.
//!
//! The applications are built but not served, so every constructor can be shown in one run.

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use gotcha::prelude::*;

#[derive(Clone, Default)]
struct AppState {
    request_counter: Arc<AtomicU64>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct AppConfig {
    api_key: String,
    max_connections: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let state = AppState::default();

    // Initialized state, empty configuration.
    let _from_state = Gotcha::from_state(state.clone()).get("/", |State(context): State<GotchaContext<AppState, EmptyConfig>>| async move {
        format!(
            "request #{}",
            context.state.request_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
        )
    });
    println!("Gotcha::from_state          -> initialized state, empty configuration");

    // Initialized state and configuration. `from_context` asks for neither a `Default`
    // implementation nor serde traits on the application config.
    let _from_context = Gotcha::from_context(GotchaContext::new(
        state.clone(),
        ConfigWrapper {
            app: AppConfig {
                api_key: "demo".into(),
                max_connections: 100,
            },
            server: ServerConfig::default(),
        },
    ))
    .get("/", |State(context): State<GotchaContext<AppState, AppConfig>>| async move {
        format!("allowing {} connections", context.config.max_connections)
    });
    println!("Gotcha::from_context        -> initialized state and configuration");

    // Default-constructed values with the automatic configuration loader.
    let _with_state = Gotcha::with_state::<AppState>().get("/", || async { "default state" });
    println!("Gotcha::with_state::<S>()   -> default state, empty configuration");
    let _with_config = Gotcha::with_config::<AppConfig>().get("/", || async { "default config" });
    println!("Gotcha::with_config::<C>()  -> empty state, loaded configuration");
    let _with_types = Gotcha::with_types::<AppState, AppConfig>().get("/", || async { "default state and config" });
    println!("Gotcha::with_types::<S, C>() -> loaded configuration, both types");

    // No custom types.
    let _simple = Gotcha::new().get("/", || async { "no custom types" });
    println!("Gotcha::new()               -> EmptyState and EmptyConfig");

    println!();
    println!("All constructors compiled; call `.listen(..)` on any of them to serve it.");

    Ok(())
}
