// The crate documentation is the README, so the front page and the repository landing page cannot
// drift apart — and its examples become doctests, so they cannot rot either.
#![doc = include_str!("../README.md")]
// Every public item carries documentation. This is denied rather than warned so an undocumented
// item fails the build instead of quietly accumulating (64 had, before this was turned on).
#![deny(missing_docs)]
// `doc(cfg(..))` is nightly-only, so it is applied on docs.rs (which sets `--cfg docsrs`) and
// skipped everywhere else. It puts a "requires feature X" badge on each gated item.
#![cfg_attr(docsrs, feature(doc_cfg))]

// Exercise the public macros (which emit `::gotcha` paths) in crate-local assembly tests.
#[cfg(test)]
extern crate self as gotcha;

pub use async_trait::async_trait;
/// WebSocket upgrade and the socket itself. The frame type stays behind `ws::Message`, since
/// [`Message`] is already the message-system trait.
pub use axum::extract::ws::{self, WebSocket, WebSocketUpgrade};
use axum::extract::FromRef;
pub use axum::extract::{Extension, State};
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

pub use router::GotchaRouter;
use serde::Deserialize;
use std::sync::Arc;
pub use {axum, axum_extra, inventory, tracing};

pub use crate::builder::{EmptyConfig, EmptyState, Gotcha};
pub use crate::config::GotchaConfigLoader;
pub use crate::error::{ErrorBody, GotchaError, GotchaResult, HttpError};
/// Extractor wrappers whose rejections are rendered by per-provider configs. Axum's originals
/// remain available as `gotcha::axum::extract::{Json, Path, Query, Form, Multipart}` and
/// `gotcha::axum_extra::TypedHeader`.
pub use crate::extract::{
    CookieConfig, ErrorBodyResponse, ExtractorConfig, ExtractorConfigs, Form, FormConfig, HeaderConfig, Json, JsonConfig, Multipart, MultipartConfig, Path,
    PathConfig, Query, QueryConfig, Rejected, TypedHeader, TypedHeaderConfig, ValidConfig,
};
/// Attribute macro that makes a struct usable as `State<T>` in handlers by
/// generating a `FromRef<GotchaContext<T, C>>` impl. See [`GotchaContext`].
pub use gotcha_macro::{config, state};

pub mod message;
/// Schema core, also usable directly by libraries without the web framework.
pub use gotcha_core;
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
/// Request body documentation helpers, mirroring [`crate::response`].
pub use gotcha_core::request;
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use gotcha_core::Responsible;

#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use crate::openapi::schematic::{ExtractorKind, ParameterProvider};
/// The schema trait and its derive macro, with [`EnhancedSchema`] and [`WireField`].
///
/// These are available in every build, not only with `openapi`: [`Valid`] uses the wire-name
/// table to report validation errors under the names serde reads, and a library can derive
/// `Schematic` without pulling in document generation. The `openapi` feature adds `#[api]`,
/// `ParameterProvider`, and document endpoints on top.
pub use gotcha_core::{EnhancedSchema, Schematic, WireField};
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use gotcha_macro::api;
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use oas;

pub use crate::message::{Message, Messager};
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub use crate::openapi::{OpenApiEndpoints, Operable};
pub use crate::params::{Cookie, CookieParam, Header, HeaderParam, ParamRejection};
pub use crate::validation::{flatten_errors, Valid, ValidRejection};
/// The header types [`TypedHeader`] works with. `TypedHeader<T>` documents itself as an OpenAPI
/// header parameter (the name comes from `headers::Header`).
/// `TypedHeaderRejection` converts into [`HttpError`] for handlers that want the JSON error body.
pub use axum_extra::headers;
pub use axum_extra::typed_header::TypedHeaderRejection;
/// The validation crate used by [`Valid`]. Use `#[validate(crate = "gotcha::validator")]`
/// with [`Validate`] to avoid a separate validator dependency.
pub use validator;
/// Derive and trait for request validation (re-exported from the `validator` crate).
/// Use with the [`Valid`] extractor. Without a direct validator dependency, set its crate path:
///
/// ```
/// #[derive(gotcha::Validate)]
/// #[validate(crate = "gotcha::validator")]
/// struct Request {
///     #[validate(length(min = 1))]
///     name: String,
/// }
///
/// assert!(gotcha::Validate::validate(&Request { name: String::new() }).is_err());
/// ```
pub use validator::Validate;

mod assembly;
pub mod builder;
pub mod config;
#[cfg(any(feature = "openapi", feature = "prometheus"))]
mod endpoint;
pub mod error;
pub mod extract;
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub mod openapi;
pub mod params;
pub mod prelude;
pub mod response;
mod startup;
pub use response::WithStatus;
pub use startup::default_logger;
/// A bound but not yet serving application; [`GotchaApp::bind`] and [`Gotcha::bind`] return it.
pub use startup::BoundServer;
/// Cancellation token carried by [`GotchaContext::shutdown`]. Cancelled when shutdown starts,
/// before HTTP connections drain.
pub use tokio_util::sync::CancellationToken;
/// The router that tracks OpenAPI operations alongside axum routes.
pub mod router;
pub mod routing;

pub mod task;
pub mod validation;

#[cfg(feature = "prometheus")]
#[cfg_attr(docsrs, doc(cfg(feature = "prometheus")))]
pub mod prometheus;
#[cfg(feature = "prometheus")]
#[cfg_attr(docsrs, doc(cfg(feature = "prometheus")))]
/// Configuration for the opt-in Prometheus metrics endpoint; see [`prometheus`].
pub use prometheus::MetricsEndpoint;

/// Middleware layers re-exported from `tower-http`.
pub mod layers {
    #[cfg(feature = "cors")]
    pub use tower_http::cors::{self, CorsLayer};
}

pub use serde_json;
pub use task::{RunningTasks, TaskScheduler};
#[cfg(feature = "static_files")]
#[cfg_attr(docsrs, doc(cfg(feature = "static_files")))]
pub use tower_http::services::{ServeDir, ServeFile};

/// The axum state the framework injects: the loaded configuration plus the application state.
///
/// Handlers rarely name this directly — `#[state]` and `#[config]` make `State<AppState>` and
/// `State<AppConfig>` extractable instead.
/// This container imposes no loading or initialization bounds; runtime APIs require
/// `Clone + Send + Sync + 'static` when sharing its values between handlers or tasks.
///
/// A context created by a running application also carries a [`CancellationToken`] cancelled
/// when shutdown starts, before HTTP connections begin draining. Long-lived handlers (WebSockets,
/// server-sent events, other streaming responses) can select on [`Self::shutdown`] to stop at the
/// same moment the listener stops accepting. Use [`Self::new`] to build a context directly for
/// tests or manual router assembly; that token is never cancelled by the framework. A standalone
/// [`TaskScheduler::new`] derives a child token from it, so shutting that scheduler down does not
/// cancel this token or other schedulers built from the same context.
pub struct GotchaContext<State, Config> {
    /// The loaded configuration, shared by every clone of the context.
    ///
    /// axum clones the state passed through `with_state` for every request, so storing the
    /// configuration behind an `Arc` keeps request handling from deep-cloning it. Field access
    /// (`context.config.field`) still works through `Arc`'s `Deref`.
    pub config: Arc<ConfigWrapper<Config>>,
    /// The application state.
    pub state: State,
    pub(crate) shutdown: CancellationToken,
}

// The configuration is shared, so cloning a context does not require `Config: Clone`.
impl<State: Clone, Config> Clone for GotchaContext<State, Config> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            state: self.state.clone(),
            shutdown: self.shutdown.clone(),
        }
    }
}

impl<State, Config> GotchaContext<State, Config> {
    /// Build a context from initialized values. The shutdown token starts uncancelled and no
    /// server owns it; applications served through [`GotchaApp::run`], [`GotchaApp::bind`],
    /// [`Gotcha::run`], or [`Gotcha::bind`] receive the server's token instead.
    pub fn new(state: State, config: ConfigWrapper<Config>) -> Self {
        Self {
            config: Arc::new(config),
            state,
            shutdown: CancellationToken::new(),
        }
    }

    /// The cancellation token for this application's shutdown.
    ///
    /// It is cancelled when the shutdown signal resolves, before HTTP draining begins, so
    /// handlers and tasks can observe shutdown at the moment the listener stops accepting.
    /// Cancelling the returned token also requests shutdown. Cloning returns a handle to the same
    /// token.
    pub fn shutdown(&self) -> CancellationToken {
        self.shutdown.clone()
    }
}

impl<State, Config> FromRef<GotchaContext<State, Config>> for ConfigWrapper<Config>
where
    Config: Clone,
{
    fn from_ref(context: &GotchaContext<State, Config>) -> Self {
        (*context.config).clone()
    }
}

/// Lets a handler take `State<ServerConfig>` to read the bind address. `ServerConfig` is one of
/// this crate's own types, so unlike the application's config this needs no attribute macro.
impl<State, Config> FromRef<GotchaContext<State, Config>> for crate::config::ServerConfig {
    fn from_ref(context: &GotchaContext<State, Config>) -> Self {
        context.config.server.clone()
    }
}

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
    ///
    /// The default loads `configurations/application.toml`, the profile-specific file, and the
    /// `APP` environment prefix above the framework's typed server defaults, so
    /// `APP_SERVER__PORT` also works without a configuration file. An override should start from
    /// [`ConfigBuilder::framework_defaults`](config::ConfigBuilder::framework_defaults) to keep
    /// that typed base, and can add application defaults with
    /// [`ConfigBuilder::defaults`](config::ConfigBuilder::defaults).
    /// [`Gotcha::load_config`] is the builder equivalent of an override.
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
    /// The default waits for Ctrl-C or, on Unix, SIGTERM.
    ///
    /// When this resolves, the [`GotchaContext`] shutdown token is cancelled and the listener
    /// stops accepting. Then HTTP connections and background tasks drain at the same time, and
    /// [`Self::on_shutdown`] runs after both have finished. See [`Self::bind`] for serving a
    /// pre-bound listener.
    ///
    /// The future is polled on the serving task, so it must not block the thread: blocking work
    /// starves the server it is meant to stop. Use an async primitive such as
    /// [`tokio::sync::Notify`] or a channel.
    fn shutdown_signal(&self) -> impl std::future::Future<Output = ()> + Send {
        crate::startup::shutdown_signal()
    }

    /// How long to let in-flight HTTP connections drain after shutdown is requested.
    /// Defaults to 30 seconds. This includes upgraded connections (WebSockets), which axum does
    /// not track after the upgrade. At the deadline, remaining sockets are shut down at the OS
    /// level, handlers get up to one second to observe the closure, and then shutdown continues.
    /// This is independent of, and runs concurrently with, `task_shutdown_timeout()`.
    fn http_shutdown_timeout(&self) -> std::time::Duration {
        crate::startup::DEFAULT_HTTP_SHUTDOWN_TIMEOUT
    }

    /// Run application cleanup after serving stops: after HTTP draining and task shutdown.
    ///
    /// Use this for ordered teardown of resources that outlive requests (feeds, terminals,
    /// child processes, watchers). The default does nothing. Hooks run inside the shutdown
    /// future, so `run()` and `BoundServer::serve()` return only after cleanup finishes.
    /// Returning errors is not supported; log inside the hook.
    ///
    /// A panicking hook is re-raised to the caller of `run()` / `serve()` after the remaining
    /// cleanup hooks have run. A hook that never resolves blocks shutdown; there is no timeout
    /// around application cleanup. Cancelling the serving future aborts a running hook.
    fn on_shutdown(&self, _context: &GotchaContext<Self::State, Self::Config>) -> impl std::future::Future<Output = ()> + Send {
        async {}
    }

    /// How long to wait for background tasks and in-flight scheduled executions after shutdown is
    /// requested. Defaults to 30 seconds. At the deadline, remaining tasks are aborted and get up
    /// to one second to stop; see [`RunningTasks::shutdown`].
    /// This runs concurrently with HTTP request draining.
    fn task_shutdown_timeout(&self) -> std::time::Duration {
        crate::task::DEFAULT_SHUTDOWN_TIMEOUT
    }

    /// Install the tracing subscriber. The default reads `RUST_LOG`.
    /// [`Gotcha::with_default_logger`] installs the same subscriber on the builder.
    fn logger(&self) -> GotchaResult<()> {
        startup::default_logger()
    }

    /// Register the application's routes.
    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>>;

    /// Build the application state, given the loaded configuration.
    /// [`Gotcha::from_state_fn`] is the builder equivalent.
    fn state(&self, config: &ConfigWrapper<Self::Config>) -> impl std::future::Future<Output = GotchaResult<Self::State>> + Send;

    /// Register background tasks without starting them. Returning an error discards all registrations.
    /// Tasks start with serving after successful initialization. The default registers none.
    /// The builder equivalent is [`Gotcha::try_tasks`], or [`Gotcha::tasks`] for infallible registration.
    fn tasks(&self, _task_scheduler: &mut TaskScheduler<Self::State, Self::Config>) -> impl std::future::Future<Output = GotchaResult<()>> + Send {
        async { Ok(()) }
    }

    /// Select the application's documentation endpoints. Defaults to `None` (disabled).
    /// Return `Some(OpenApiEndpoints::default())` to enable the standard JSON and UI paths.
    #[cfg(feature = "openapi")]
    fn openapi_endpoints(&self) -> Option<OpenApiEndpoints> {
        None
    }

    /// The [`oas::Info`] block of the generated OpenAPI document.
    ///
    /// The default is neutral rather than gotcha's own identity: `title: "API"`,
    /// `version: "0.1.0"`, no description, and every optional field `None`. Override it to
    /// publish the application's identity; [`cargo_info!`](crate::cargo_info) builds the block
    /// from the calling crate's Cargo metadata:
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "openapi")]
    /// # {
    /// use gotcha::{ConfigWrapper, EmptyConfig, GotchaApp, GotchaResult};
    ///
    /// struct App;
    /// impl GotchaApp for App {
    ///     type State = ();
    ///     type Config = EmptyConfig;
    ///     fn openapi_info(&self) -> gotcha::oas::Info {
    ///         gotcha::cargo_info!()
    ///     }
    ///     fn routes(&self, router: gotcha::GotchaRouter<gotcha::GotchaContext<(), EmptyConfig>>) -> gotcha::GotchaRouter<gotcha::GotchaContext<(), EmptyConfig>> {
    ///         router
    ///     }
    ///     async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
    ///         Ok(())
    ///     }
    /// }
    /// # }
    /// ```
    ///
    /// The same block is used by the served document and [`Self::openapi_document`]. Exporting
    /// directly from `routes(..)` does not see this hook: pass the value on with
    /// [`GotchaRouter::openapi_info`], as in
    /// `app.routes(GotchaRouter::default()).openapi_info(app.openapi_info()).into_openapi()`.
    /// `Info` describes the whole application, so nested and merged routers do not contribute
    /// one. `.openapi(..)` transforms run afterwards and can still override it.
    #[cfg(feature = "openapi")]
    fn openapi_info(&self) -> oas::Info {
        crate::openapi::default_info()
    }

    /// Select the application's Prometheus metrics endpoint. Defaults to `None` (disabled).
    /// Return `Some(MetricsEndpoint::default())` to record HTTP metrics for the business routes
    /// and serve them at `/metrics`; [`Gotcha::metrics`] is the builder equivalent.
    ///
    /// The metric layer covers the routes registered by [`Self::routes`], including nested
    /// routers. Documentation endpoints, the metrics endpoint itself, and routes, services, or
    /// middleware added by [`Self::finish_router`] are not measured; the extractor-config layer
    /// stays outermost.
    ///
    /// The `metrics` recorder is process-wide: it is installed once and shared by every
    /// application in the process that enables metrics. See [`prometheus`] for the implications.
    #[cfg(feature = "prometheus")]
    fn metrics_endpoint(&self) -> Option<MetricsEndpoint> {
        None
    }

    /// Whether operations document the rejection responses of Gotcha's extractor providers.
    ///
    /// The default is `true`: every operation documents the statuses its `Json`, `Form`, `Query`,
    /// `Path`, `Multipart`, `TypedHeader`, `Header`, `Cookie`, and `Valid` arguments can reject
    /// with, using [`Self::extractor_configs`]. Return `false` to document only the responses
    /// inferred from the handler's return type and declared in `#[api]`.
    ///
    /// The same setting applies to the served document and to [`Self::openapi_document`]. The
    /// builder equivalent is
    /// [`Gotcha::document_extractor_errors`](crate::Gotcha::document_extractor_errors). Exporting
    /// directly from `routes(..)` does not see this hook: pass the value on with
    /// [`GotchaRouter::document_extractor_errors`], as in
    /// `app.routes(GotchaRouter::default()).document_extractor_errors(app.document_extractor_errors())`.
    #[cfg(feature = "openapi")]
    fn document_extractor_errors(&self) -> bool {
        true
    }

    /// Rejection responses for the extractors this application uses.
    ///
    /// The default is every provider's default handler. Override it and use
    /// [`ExtractorConfigs::set`] to replace one provider:
    /// [`Gotcha::extractor_config`] is the builder equivalent.
    ///
    /// ```rust,no_run
    /// use gotcha::{ConfigWrapper, EmptyConfig, ErrorBody, ExtractorConfigs, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, JsonConfig};
    ///
    /// struct App;
    /// impl GotchaApp for App {
    ///     type State = ();
    ///     type Config = EmptyConfig;
    ///     fn extractor_configs(&self) -> ExtractorConfigs {
    ///         ExtractorConfigs::default().set(JsonConfig::default().error_handler(|rejection| {
    ///             (gotcha::axum::http::StatusCode::UNPROCESSABLE_ENTITY, ErrorBody::new(rejection.body_text()))
    ///         }))
    ///     }
    ///     fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
    ///         router
    ///     }
    ///     async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
    ///         Ok(())
    ///     }
    /// }
    /// ```
    fn extractor_configs(&self) -> ExtractorConfigs {
        ExtractorConfigs::default()
    }

    /// Generate the complete OpenAPI document from `routes()`, without starting the app.
    ///
    /// Does not load configuration, initialize state, bind a listener, register tasks,
    /// or call `finish_router`. Each call creates and consumes a fresh route definition,
    /// applying its transforms once, as HTTP documentation assembly does. Route registration
    /// and user transforms still execute, so their own side effects remain the caller's concern.
    #[cfg(feature = "openapi")]
    fn openapi_document(&self) -> oas::OpenAPIV3 {
        self.routes(GotchaRouter::default())
            .with_extractor_configs(self.extractor_configs())
            .openapi_info(self.openapi_info())
            .document_extractor_errors(self.document_extractor_errors())
            .into_openapi()
    }

    /// Apply application-wide middleware, or mount state-dependent services, after business
    /// routes and documentation are mounted.
    ///
    /// The hook receives the assembled router and a clone of the application's [`GotchaContext`]:
    /// the configuration is shared behind an `Arc`, and state that is itself shared (behind an
    /// `Arc`, say) is the same value the routes see. A service that needs application state at
    /// construction time — a reverse proxy closing over a registry, for example — can therefore
    /// be built from `context` and mounted with `nest_service` or `fallback_service`.
    ///
    /// For example, return `router.layer(auth)` to protect every endpoint. This hook runs once
    /// in the default `build_router` implementation and is not called during document export.
    /// Routes added here use native Axum routing and are not included in OpenAPI metadata.
    fn finish_router(&self, router: axum::Router, _context: &GotchaContext<Self::State, Self::Config>) -> axum::Router {
        router
    }

    /// Assemble business routes, optional documentation, and then `finish_router`.
    ///
    /// Prefer `finish_router` for whole-application middleware or services that need the
    /// application context. Overriding this method takes responsibility for the complete
    /// assembly, including documentation, the metrics endpoint, and final middleware. It must
    /// also apply `self.extractor_configs().layer(router)` after every route and middleware;
    /// without that layer, extractors fall back to the default rejection handlers.
    /// [`Gotcha::into_router`] is the builder equivalent of the default implementation.
    #[cfg_attr(
        feature = "prometheus",
        doc = "When metrics are enabled, apply [`MetricsEndpoint::layer`](crate::MetricsEndpoint::layer) to the business router before mounting documentation."
    )]
    fn build_router(&self, context: GotchaContext<Self::State, Self::Config>) -> impl std::future::Future<Output = GotchaResult<axum::Router>> + Send {
        async move {
            let router = GotchaRouter::<GotchaContext<Self::State, Self::Config>>::default();
            let router = self.routes(router).with_extractor_configs(self.extractor_configs());
            #[cfg(feature = "openapi")]
            let router = router
                .openapi_info(self.openapi_info())
                .document_extractor_errors(self.document_extractor_errors());
            assembly::assemble(
                router,
                context,
                #[cfg(feature = "openapi")]
                self.openapi_endpoints(),
                #[cfg(feature = "prometheus")]
                self.metrics_endpoint(),
                |router, context| self.finish_router(router, context),
            )
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

    /// Load configuration, bind its address, build state and routes, and register tasks, but do
    /// not serve yet. The returned [`BoundServer`] exposes the effective address before serving,
    /// so port 0 works for integration tests and in-process tools.
    ///
    /// Binding failure happens before state initialization or task registration and runs no
    /// cleanup hooks. Dropping the result without serving releases the port without running
    /// cleanup. Requires `Self: 'static` because the bound server erases the application into
    /// its signal and cleanup driver; use [`Self::run`] for applications that borrow external data.
    fn bind(self) -> impl std::future::Future<Output = GotchaResult<BoundServer>> + Send
    where
        Self: 'static,
        Self::Config: for<'de> Deserialize<'de>,
    {
        async move { startup::bind(startup::OwnedApp(self), None).await }
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

    /// Fixtures that need a data-type impl, kept out of the glob so `--no-default-features`
    /// builds without that feature still compile.
    #[test]
    #[cfg(all(feature = "openapi", feature = "uuid"))]
    fn pass_uuid_fixtures() {
        trybuild::TestCases::new().pass("tests/pass/openapi/data_types/uuid_newtype_struct.rs");
    }

    #[test]
    #[cfg(all(feature = "openapi", feature = "rust_decimal"))]
    fn pass_rust_decimal_fixtures() {
        trybuild::TestCases::new().pass("tests/pass/openapi/data_types/rust_decimal.rs");
    }

    #[test]
    #[cfg(feature = "openapi")]
    fn test_handler() {
        let t = trybuild::TestCases::new();
        t.pass("tests/pass/handler/*.rs");
    }
}
