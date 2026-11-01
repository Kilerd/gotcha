//! # Gotcha Prelude
//!
//! This module contains the most commonly used items from Gotcha.
//! By importing this module, you get everything you need for most applications.
//!
//! ## Example
//!
//! ```no_run
//! use gotcha::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     Gotcha::new()
//!         .get("/", || async { "Hello World" })
//!         .post("/echo", |body: String| async move { body })
//!         .listen("127.0.0.1:3000")
//!         .await?;
//!     Ok(())
//! }
//! ```

// Re-export the most commonly used items

// Core builder API
pub use crate::builder::{EmptyConfig, EmptyState, Gotcha};

// Essential traits and types
pub use crate::config::{ConfigErrorPolicy, ConfigWrapper, GotchaConfigLoader, ServerConfig};
pub use crate::error::{ErrorBody, GotchaError, GotchaResult, HttpError};
pub use crate::response::WithStatus;
pub use crate::router::Responder;
pub use crate::{config, state, GotchaApp, GotchaContext, GotchaRouter};

// Method routing with OpenAPI metadata
pub use crate::routing::{delete, get, head, on, options, patch, post, put, trace, MethodRouter};

// Common extractors and utilities. `Json`, `Path`, `Query`, `Form`, and `Multipart` are Gotcha's
// wrappers, whose rejections use the per-provider configs; Axum's originals stay in `gotcha::axum`.
pub use crate::extract::{Form, Json, Multipart, Path, Query, Rejected};
pub use axum::extract::ws::{WebSocket, WebSocketUpgrade};
pub use axum::extract::{Extension, State};
pub use axum::extract::{MatchedPath, OriginalUri};
pub use axum::http::{HeaderMap, Method, StatusCode};
pub use axum::middleware;
pub use axum::response::sse::{Event, KeepAlive, Sse};
pub use axum::response::{Html, Redirect, Response};

// JSON handling
pub use serde::{Deserialize, Serialize};
pub use serde_json::{json, Value as JsonValue};

// Async traits
pub use crate::async_trait;

// The schema trait and derive macro exist in every build; `Valid` needs them for wire names.
pub use crate::Schematic;

// Feature-specific exports
#[cfg(feature = "openapi")]
pub use crate::{api, OpenApiEndpoints, Responsible};

#[cfg(feature = "prometheus")]
pub use crate::MetricsEndpoint;

#[cfg(feature = "cors")]
pub use crate::layers::CorsLayer;

#[cfg(feature = "task")]
pub use crate::{RunningTasks, TaskScheduler};

// Utility macros for common patterns
#[macro_export]
/// Builds a `Json` response from a `serde_json::json!` literal.
///
/// ```
/// let response = gotcha::json_response!({"ok": true});
/// assert_eq!(
///     gotcha::axum::response::IntoResponse::into_response(response).headers()["content-type"],
///     "application/json"
/// );
/// ```
///
/// The tokens are forwarded unchanged, so object and array literals parse as JSON.
macro_rules! json_response {
    ($($data:tt)+) => {
        $crate::Json($crate::serde_json::json!($($data)+))
    };
}

// Re-export macros
pub use crate::json_response;
