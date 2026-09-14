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
pub use axum_macros::debug_handler;
pub use config::{ConfigErrorPolicy, ConfigWrapper, ServerConfig};
pub use either::Either;
pub use routing::{delete, get, head, on, options, patch, post, put, trace, MethodRouter};

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
pub mod response;
mod startup;
pub use response::WithStatus;
/// The router that tracks OpenAPI operations alongside axum routes.
pub mod router;
pub mod routing;

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
pub use task::{RunningTasks, TaskScheduler};
#[cfg(feature = "static_files")]
#[cfg_attr(docsrs, doc(cfg(feature = "static_files")))]
pub use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
/// The axum state the framework injects: the loaded configuration plus the application state.
///
/// Handlers rarely name this directly — `#[state]` and `#[config]` make `State<AppState>` and
/// `State<AppConfig>` extractable instead.
/// This container imposes no loading or initialization bounds; runtime APIs require
/// `Clone + Send + Sync + 'static` when sharing its values between handlers or tasks.
pub struct GotchaContext<State, Config> {
    /// The loaded configuration.
    pub config: ConfigWrapper<Config>,
    /// The application state.
    pub state: State,
}

impl<State, Config> FromRef<GotchaContext<State, Config>> for ConfigWrapper<Config>
where
    Config: Clone,
{
    fn from_ref(context: &GotchaContext<State, Config>) -> Self {
        context.config.clone()
    }
}

/// Lets a handler take `State<ServerConfig>` to read the bind address. `ServerConfig` is one of
/// this crate's own types, so unlike the application's config this needs no attribute macro.
impl<State, Config> FromRef<GotchaContext<State, Config>> for crate::config::ServerConfig {
    fn from_ref(context: &GotchaContext<State, Config>) -> Self {
        context.config.server.clone()
    }
}

/// Compatibility bundle of the original configuration bounds.
///
/// Existing generic code can keep using this trait. Runtime containers, extractors, messages,
/// and tasks do not require it; loading, serialization, and default construction impose their
/// individual bounds only where needed.
pub trait GotchaConfig: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default {}
impl<T> GotchaConfig for T where T: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default {}

/// The trait API: implement it to describe an application, then call `run()`.
///
/// The builder ([`Gotcha`]) is the simpler alternative; both assemble the router through the same
/// path, so they behave identically.
pub trait GotchaApp: Sized + Send + Sync {
    /// The application state, shared by every handler.
    type State: Clone + Send + Sync + 'static;
    /// The application's own configuration. Loading via `config()`/`run()` additionally
    /// requires `Deserialize`; `build_router()` accepts an already initialized context.
    type Config: Clone + Send + Sync + 'static;

    /// Load the configuration. The default honours `GOTCHA_ACTIVE_PROFILE`.
    /// This method's contract requires `Deserialize`, including when overridden.
    fn config(&self) -> impl std::future::Future<Output = GotchaResult<ConfigWrapper<Self::Config>>> + Send
    where
        Self::Config: for<'de> Deserialize<'de>,
    {
        async move {
            let config = GotchaConfigLoader::load::<ConfigWrapper<Self::Config>>(std::env::var("GOTCHA_ACTIVE_PROFILE").ok())?;
            Ok(config)
        }
    }

    /// Startup is strict by default. Override to opt into a fallback after a configuration error.
    /// This applies to `run()`, including errors returned by a custom `config()` implementation.
    /// Direct calls to `config()` still return their loading errors.
    fn config_error_policy(&self) -> ConfigErrorPolicy<Self::Config> {
        ConfigErrorPolicy::Strict
    }

    /// Resolve to stop accepting HTTP connections and scheduling new task executions.
    /// The default waits for Ctrl-C or, on Unix, SIGTERM. Existing HTTP requests drain;
    /// scheduled executions wait for at most `task_shutdown_timeout()` (with the `task` feature).
    fn shutdown_signal(&self) -> impl std::future::Future<Output = ()> + Send {
        crate::startup::shutdown_signal()
    }

    /// How long to wait for in-flight scheduled executions after shutdown is requested.
    /// Defaults to 30 seconds. At the deadline, remaining scheduled tasks are aborted and joined.
    /// This does not limit HTTP request draining.
    #[cfg(feature = "task")]
    fn task_shutdown_timeout(&self) -> std::time::Duration {
        crate::task::DEFAULT_SHUTDOWN_TIMEOUT
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
    /// Register background tasks without starting them. Returning an error discards all registrations.
    /// Tasks start with serving after successful initialization. The default registers none.
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

    /// Load configuration, bind its address, build state and routes, register tasks, then serve.
    /// State initialization and handlers receive the effective address, including the port chosen
    /// for port 0. Binding failure does not initialize state or register background tasks.
    /// For configuration without `Deserialize`, assemble an explicit context using
    /// [`Self::build_router`] or [`Gotcha::from_context`].
    fn run(self) -> impl std::future::Future<Output = GotchaResult<()>> + Send
    where
        Self::Config: for<'de> Deserialize<'de>,
    {
        async move { startup::run(&self, None).await }
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
