//! `#[state]` makes a struct extractable as `State<T>` directly in handlers.
//!
//! Before `#[state]`, a handler taking `State<AppState>` did not compile — there
//! was no `FromRef<GotchaContext<AppState, _>> for AppState`, so users had to
//! extract the whole `State<GotchaContext<AppState, AppConfig>>` and reach into
//! `.state`. This test fails to compile if that regressed.

use gotcha::prelude::*;
use serde::{Deserialize, Serialize};

#[state]
#[derive(Clone, Default)]
struct AppState {
    name: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct AppConfig {}

// The whole point: extract the application state directly.
async fn state_handler(State(state): State<AppState>) -> impl Responder {
    state.name
}

// Config remains extractable too (via the framework's own FromRef impl).
async fn config_handler(State(config): State<ConfigWrapper<AppConfig>>) -> impl Responder {
    config.server.host.clone()
}

fn main() {
    let _app = Gotcha::with_types::<AppState, AppConfig>()
        .get("/state", state_handler)
        .get("/config", config_handler);
}
