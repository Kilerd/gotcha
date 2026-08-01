//! # Gotcha
//!
//! Gotcha is an enhanced web framework based on Axum, providing additional features and conveniences
//! for building web applications in Rust.
//!
//! ## Features
//!
//! - Built on top of Axum for high performance and reliability
//! - OpenAPI documentation generation (optional)
//! - Prometheus metrics integration (optional)
//! - CORS support and static file serving (optional)
//! - Task scheduling (optional)
//! - Configuration management
//! - Request validation
//! - Message system
//! - State management
//!
//! ## Simple Example (New Builder API)
//!
//! ```no_run
//! use gotcha::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     Gotcha::new()
//!         .get("/", || async { "Hello World" })
//!         .get("/hello/:name", |Path(name): Path<String>| async move {
//!             format!("Hello, {}!", name)
//!         })
//!         .listen("127.0.0.1:3000")
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Advanced Example (Traditional Trait API)
//!
//! ```no_run
//! use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Responder, State};
//! use serde::{Deserialize, Serialize};
//!
//! pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
//!     "hello world"
//! }
//!
//! #[derive(Debug, Deserialize, Serialize, Clone, Default)]
//! pub struct Config {
//!     pub name: String,
//! }
//!
//! pub struct App {}
//!
//! impl GotchaApp for App {
//!     type State = ();
//!     type Config = Config;
//!
//!     fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
//!         router.get("/", hello_world)
//!     }
//!
//!     async fn state(&self, _config: &ConfigWrapper<Self::Config>) -> GotchaResult<Self::State> {
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     App {}.run().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Optional Features
//!
//! Each one gates a dependency that not every application needs; everything else — configuration,
//! the message system, validation, header/cookie parameters — is always available.
//!
//! - `openapi` - OpenAPI documentation generation (`oas`, `gotcha_core`)
//! - `prometheus` - Prometheus metrics (`axum-prometheus`)
//! - `cors` - CORS layer (`tower-http/cors`, which has no dependencies of its own)
//! - `static_files` - Static file serving (`tower-http/fs`, which pulls in mime and range handling)
//! - `task` - Background task scheduling (`cron`)
//!

pub use async_trait::async_trait;
use axum::extract::FromRef;
pub use axum::extract::{Extension, Json, Path, Query, State};
pub use axum::response::IntoResponse as Responder;
pub use axum::routing::{delete, get, patch, post, put};
pub use axum_macros::debug_handler;
pub use config::{ConfigWrapper, ServerConfig};
pub use either::Either;

pub use once_cell::sync::Lazy;
pub use router::GotchaRouter;
use serde::{Deserialize, Serialize};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};
pub use {axum, inventory, tracing};

pub use crate::builder::{EmptyConfig, EmptyState, Gotcha};
pub use crate::config::GotchaConfigLoader;
pub use crate::error::{GotchaError, GotchaResult};
/// Attribute macro that makes a struct usable as `State<T>` in handlers by
/// generating a `FromRef<GotchaContext<T, C>>` impl. See [`GotchaContext`].
pub use gotcha_macro::{config, state};

pub mod message;
#[cfg(feature = "openapi")]
pub use gotcha_core::Responsible;

#[cfg(feature = "openapi")]
pub use crate::openapi::schematic::{ParameterProvider, Schematic};
#[cfg(feature = "openapi")]
pub use gotcha_macro::api;
#[cfg(feature = "openapi")]
pub use oas;

pub use crate::message::{Message, Messager};
#[cfg(feature = "openapi")]
pub use crate::openapi::Operable;
pub use crate::params::{Cookie, CookieParam, Header, HeaderParam, ParamRejection};
pub use crate::validation::{Valid, ValidRejection};
/// axum's typed-header extractor and the header types it works with. `TypedHeader<T>` documents
/// itself as an OpenAPI header parameter (the name comes from `headers::Header`).
pub use axum_extra::{headers, TypedHeader};
/// Derive and trait for request validation (re-exported from the `validator` crate).
/// Use with the [`Valid`] extractor.
pub use validator::Validate;

pub mod builder;
pub mod config;
pub mod error;
#[cfg(feature = "openapi")]
pub mod openapi;
pub mod params;
pub mod prelude;
pub mod router;

#[cfg(feature = "task")]
pub mod task;
pub mod validation;

#[cfg(feature = "prometheus")]
pub mod prometheus {
    pub use axum_prometheus::metrics::*;
}

pub mod layers {
    #[cfg(feature = "cors")]
    pub use tower_http::cors::{self, CorsLayer};
}

#[cfg(feature = "openapi")]
pub use crate::openapi::schematic::EnhancedSchema;

pub use serde_json;
#[cfg(feature = "task")]
pub use task::TaskScheduler;
#[cfg(feature = "static_files")]
pub use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct GotchaContext<State: Clone + Send + Sync + 'static, Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default> {
    pub config: ConfigWrapper<Config>,
    pub state: State,
}

impl<State, Config> FromRef<GotchaContext<State, Config>> for ConfigWrapper<Config>
where
    State: Clone + Send + Sync + 'static,
    Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
{
    fn from_ref(context: &GotchaContext<State, Config>) -> Self {
        context.config.clone()
    }
}

/// Lets a handler take `State<ServerConfig>` to read the bind address. `ServerConfig` is one of
/// this crate's own types, so unlike the application's config this needs no attribute macro.
impl<State, Config> FromRef<GotchaContext<State, Config>> for crate::config::ServerConfig
where
    State: Clone + Send + Sync + 'static,
    Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
{
    fn from_ref(context: &GotchaContext<State, Config>) -> Self {
        context.config.server.clone()
    }
}

/// Marker trait bundling the bounds every Gotcha application `Config` must meet.
///
/// Blanket-implemented for every qualifying type. It exists so macro-generated
/// code (the `#[state]` attribute) can bound the config type with a single path
/// (`::gotcha::GotchaConfig`) instead of restating the serde bounds.
pub trait GotchaConfig: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default {}
impl<T> GotchaConfig for T where T: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default {}

pub trait GotchaApp: Sized + Send + Sync {
    type State: Clone + Send + Sync + 'static;
    type Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default;

    fn config(&self) -> impl std::future::Future<Output = GotchaResult<ConfigWrapper<Self::Config>>> + Send {
        async move {
            let config = GotchaConfigLoader::load::<ConfigWrapper<Self::Config>>(std::env::var("GOTCHA_ACTIVE_PROFILE").ok())?;
            Ok(config)
        }
    }

    fn logger(&self) -> GotchaResult<()> {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .with_env_var("RUST_LOG")
                    .from_env_lossy(),
            )
            .try_init()
            .ok();
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>>;

    fn state(&self, config: &ConfigWrapper<Self::Config>) -> impl std::future::Future<Output = GotchaResult<Self::State>> + Send;

    #[cfg(feature = "task")]
    fn tasks(&self, _task_scheduler: &mut TaskScheduler<Self::State, Self::Config>) -> impl std::future::Future<Output = GotchaResult<()>> + Send {
        async { Ok(()) }
    }

    fn build_router(&self, context: GotchaContext<Self::State, Self::Config>) -> impl std::future::Future<Output = GotchaResult<axum::Router>> + Send {
        async move {
            let router = GotchaRouter::<GotchaContext<Self::State, Self::Config>>::default();
            let router = self.routes(router);
            Ok(router.into_axum_router(context))
        }
    }

    fn run(self) -> impl std::future::Future<Output = GotchaResult<()>> + Send {
        async move {
            use std::net::{Ipv4Addr, SocketAddrV4};
            use std::str::FromStr;
            self.logger()?;
            tracing::info!("logger has been initialized");
            let config: ConfigWrapper<Self::Config> = self.config().await?;
            let state = self.state(&config).await?;

            let context = GotchaContext { config: config.clone(), state };

            let router = self.build_router(context.clone()).await?;

            cfg_if::cfg_if! {
                if #[cfg(feature = "task")] {
                    let mut task_scheduler = TaskScheduler::new(context.clone());
                    self.tasks(&mut task_scheduler).await?;
                }
            }

            let ip = Ipv4Addr::from_str(&config.server.host).map_err(|_| GotchaError::InvalidAddress(config.server.host.clone()))?;
            let addr = SocketAddrV4::new(ip, config.server.port);
            let listener = tokio::net::TcpListener::bind(addr).await.map_err(|source| GotchaError::Bind {
                addr: addr.to_string(),
                source,
            })?;
            axum::serve(listener, router).await.map_err(GotchaError::Io)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    #[cfg(feature = "openapi")]
    fn pass() {
        let t = trybuild::TestCases::new();
        t.pass("tests/pass/openapi/*.rs");
    }

    #[test]
    #[cfg(feature = "openapi")]
    fn test_handler() {
        let t = trybuild::TestCases::new();
        t.pass("tests/pass/handler/*.rs");
    }
}
