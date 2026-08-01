//! `#[config]` makes the application's own config extractable as `State<T>` directly, so a handler
//! reads `config.name` instead of reaching through a wrapper level (`config.application.name`).
//!
//! A blanket `impl FromRef<GotchaContext<S, C>> for C` is impossible (the orphan rule rejects a
//! bare type parameter as `Self`), which is why this is an attribute macro — the same reason
//! `#[state]` exists.

use gotcha::prelude::*;
use serde::{Deserialize, Serialize};

#[state]
#[derive(Clone, Default)]
struct AppState {}

#[config]
#[derive(Clone, Default, Serialize, Deserialize)]
struct AppConfig {
    name: String,
}

// The point: the application's own config, one level deep.
async fn config_handler(State(config): State<AppConfig>) -> impl Responder {
    config.name
}

// The framework's own settings are a separate extractor — no attribute needed, since
// `ServerConfig` belongs to gotcha.
async fn server_handler(State(server): State<ServerConfig>) -> impl Responder {
    format!("{}:{}", server.host, server.port)
}

// The whole wrapper is still available for anyone who wants both at once, and derefs to the
// application config.
async fn wrapper_handler(State(config): State<ConfigWrapper<AppConfig>>) -> impl Responder {
    format!("{} on {}", config.name, config.server.port)
}

fn main() {
    let _app = Gotcha::with_types::<AppState, AppConfig>()
        .get("/config", config_handler)
        .get("/server", server_handler)
        .get("/wrapper", wrapper_handler);
}
