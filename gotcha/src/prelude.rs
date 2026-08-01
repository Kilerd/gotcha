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
pub use crate::config::{ConfigWrapper, GotchaConfigLoader, ServerConfig};
pub use crate::error::{GotchaError, GotchaResult};
pub use crate::router::Responder;
pub use crate::{config, state, GotchaApp, GotchaConfig, GotchaContext, GotchaRouter};

// Common Axum extractors and utilities
pub use axum::extract::{Extension, Json, Path, Query, State};
pub use axum::http::{HeaderMap, Method, StatusCode};
pub use axum::response::{Html, Redirect, Response};
pub use axum::routing::{delete, get, patch, post, put};

// JSON handling
pub use serde::{Deserialize, Serialize};
pub use serde_json::{json, Value as JsonValue};

// Async traits
pub use crate::async_trait;

// Common result type
pub type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;

// Feature-specific exports
#[cfg(feature = "openapi")]
pub use crate::{api, Responsible, Schematic};

#[cfg(feature = "cors")]
pub use crate::layers::CorsLayer;

#[cfg(feature = "task")]
pub use crate::TaskScheduler;

// Utility macros for common patterns
#[macro_export]
macro_rules! handler {
    // Simple async closure handler
    ($body:expr) => {
        || async move { $body }
    };

    // Handler with single parameter
    ($param:ident: $param_type:ty => $body:expr) => {
        |$param: $param_type| async move { $body }
    };
}

#[macro_export]
macro_rules! json_response {
    ($data:expr) => {
        axum::Json(serde_json::json!($data))
    };
}

#[macro_export]
macro_rules! quick_server {
    (
        $(($method:ident, $path:literal, $handler:expr)),* $(,)?
    ) => {
        {
            use $crate::prelude::*;
            let mut app = Gotcha::new();
            $(
                app = app.$method($path, $handler);
            )*
            app
        }
    };
}

// Re-export macros
pub use crate::{handler, json_response, quick_server};
