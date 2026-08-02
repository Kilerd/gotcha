// The crate documentation is the README, so the front page and the repository landing page cannot
// drift apart — and its examples become doctests, so they cannot rot either.
#![doc = include_str!("../README.md")]
// Every public item carries documentation. This is denied rather than warned so an undocumented
// item fails the build instead of quietly accumulating (64 had, before this was turned on).
#![deny(missing_docs)]
// `doc(cfg(..))` is nightly-only, so it is applied on docs.rs (which sets `--cfg docsrs`) and
// skipped everywhere else. It puts a "requires feature X" badge on each gated item.
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use async_trait::async_trait;
/// WebSocket upgrade and the socket itself. The frame type stays behind `ws::Message`, since
/// [`Message`] is already the message-system trait.
pub use axum::extract::ws::{self, WebSocket, WebSocketUpgrade};
use axum::extract::FromRef;
pub use axum::extract::{Extension, Form, Json, Multipart, Path, Query, State};
/// The request path as matched by the router (`/users/{id}`) and the URI before any nesting
/// rewrote it. Both need axum features that this crate turns on.
pub use axum::extract::{MatchedPath, OriginalUri};
/// Writing custom middleware — `middleware::from_fn` and friends.
pub use axum::middleware;
/// Server-sent events.
pub use axum::response::sse::{self, Event, KeepAlive, Sse};
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
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use gotcha_core::Responsible;

#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use crate::openapi::schematic::{ParameterProvider, Schematic};
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use gotcha_macro::api;
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use oas;

pub use crate::message::{Message, Messager};
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
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
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub mod openapi;
pub mod params;
pub mod prelude;
/// The router that tracks OpenAPI operations alongside axum routes.
pub mod router;

#[cfg(feature = "task")]
#[cfg_attr(docsrs, doc(cfg(feature = "task")))]
pub mod task;
pub mod validation;

#[cfg(feature = "prometheus")]
#[cfg_attr(docsrs, doc(cfg(feature = "prometheus")))]
/// Prometheus metrics, re-exported from `axum-prometheus`.
pub mod prometheus {
    pub use axum_prometheus::metrics::*;
}

/// Middleware layers re-exported from `tower-http`.
pub mod layers {
    #[cfg(feature = "cors")]
    pub use tower_http::cors::{self, CorsLayer};
}

#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use crate::openapi::schematic::EnhancedSchema;

pub use serde_json;
#[cfg(feature = "task")]
#[cfg_attr(docsrs, doc(cfg(feature = "task")))]
pub use task::TaskScheduler;
#[cfg(feature = "static_files")]
#[cfg_attr(docsrs, doc(cfg(feature = "static_files")))]
pub use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
/// The axum state the framework injects: the loaded configuration plus the application state.
///
/// Handlers rarely name this directly — `#[state]` and `#[config]` make `State<AppState>` and
/// `State<AppConfig>` extractable instead.
pub struct GotchaContext<State: Clone + Send + Sync + 'static, Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default> {
    /// The loaded configuration.
    pub config: ConfigWrapper<Config>,
    /// The application state.
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

/// The trait API: implement it to describe an application, then call `run()`.
///
/// The builder ([`Gotcha`]) is the simpler alternative; both assemble the router through the same
/// path, so they behave identically.
pub trait GotchaApp: Sized + Send + Sync {
    /// The application state, shared by every handler.
    type State: Clone + Send + Sync + 'static;
    /// The application's own configuration, read from the top level of the config file.
    type Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default;

    /// Load the configuration. The default honours `GOTCHA_ACTIVE_PROFILE`.
    fn config(&self) -> impl std::future::Future<Output = GotchaResult<ConfigWrapper<Self::Config>>> + Send {
        async move {
            let config = GotchaConfigLoader::load::<ConfigWrapper<Self::Config>>(std::env::var("GOTCHA_ACTIVE_PROFILE").ok())?;
            Ok(config)
        }
    }

    /// Install the tracing subscriber. The default reads `RUST_LOG`.
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

    /// Register the application's routes.
    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>>;

    /// Build the application state, given the loaded configuration.
    fn state(&self, config: &ConfigWrapper<Self::Config>) -> impl std::future::Future<Output = GotchaResult<Self::State>> + Send;

    #[cfg(feature = "task")]
    /// Register background tasks. The default registers none.
    fn tasks(&self, _task_scheduler: &mut TaskScheduler<Self::State, Self::Config>) -> impl std::future::Future<Output = GotchaResult<()>> + Send {
        async { Ok(()) }
    }

    /// Assemble the final axum router. Override only to wrap the whole application.
    fn build_router(&self, context: GotchaContext<Self::State, Self::Config>) -> impl std::future::Future<Output = GotchaResult<axum::Router>> + Send {
        async move {
            let router = GotchaRouter::<GotchaContext<Self::State, Self::Config>>::default();
            let router = self.routes(router);
            Ok(router.into_axum_router(context))
        }
    }

    /// Load configuration, build state and routes, then serve until shutdown.
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
