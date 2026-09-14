//! # Gotcha Builder API
//!
//! This module provides a simplified, fluent API for building Gotcha applications
//! without requiring trait implementations. It's designed to make simple applications
//! easy to create while still supporting complex use cases.
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
//!         .get("/health", || async { Json(json!({"status": "ok"})) })
//!         .listen("127.0.0.1:3000")
//!         .await?;
//!     Ok(())
//! }
//! ```

use std::net::SocketAddr;

use axum::extract::Request;
use axum::handler::Handler;
use axum::routing::MethodRouter as AxumMethodRouter;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tower_layer::Layer;
use tower_service::Service;

use crate::config::{Config, ConfigBuilder, ConfigErrorPolicy, ConfigWrapper, GotchaConfigLoader};
use crate::error::{GotchaError, GotchaResult};
use crate::router::{GotchaRouter, Responder};
use crate::routing::MethodRouter;
use crate::startup::{self, ListenOptions};
use crate::GotchaContext;

/// A one-shot closure that registers background tasks on the scheduler when the
/// server starts.
#[cfg(feature = "task")]
type TaskRegistrar<S, C> = Box<dyn FnOnce(&mut crate::TaskScheduler<S, C>) + Send>;

type AppLayer = Box<dyn FnOnce(axum::Router) -> axum::Router + Send>;

enum StateSource<S> {
    Provided(S),
    Default(fn() -> S),
}

impl<S: Clone> StateSource<S> {
    fn resolve(&self) -> S {
        match self {
            Self::Provided(state) => state.clone(),
            Self::Default(create) => create(),
        }
    }
}

enum Configuration<C> {
    Provided(ConfigWrapper<C>),
    Deferred {
        sources: Option<ConfigBuilder>,
        load: fn(Option<&ConfigBuilder>) -> GotchaResult<ConfigWrapper<C>>,
    },
}

impl<C> Configuration<C> {
    fn automatic() -> Self
    where
        C: DeserializeOwned,
    {
        Self::Deferred {
            sources: None,
            load: load_config::<C>,
        }
    }

    fn update_sources(&mut self, update: impl FnOnce(Option<ConfigBuilder>) -> ConfigBuilder) {
        // Explicit configuration always wins, regardless of when sources are registered.
        if let Self::Deferred { sources, .. } = self {
            *sources = Some(update(sources.take()));
        }
    }

    fn resolve(&self) -> GotchaResult<ConfigWrapper<C>>
    where
        C: Clone,
    {
        match self {
            Self::Provided(config) => Ok(config.clone()),
            Self::Deferred { sources, load } => load(sources.as_ref()),
        }
    }
}

fn load_config<C: DeserializeOwned>(sources: Option<&ConfigBuilder>) -> GotchaResult<ConfigWrapper<C>> {
    if let Some(builder) = sources {
        return Ok(builder.clone().build()?);
    }
    Ok(GotchaConfigLoader::load::<ConfigWrapper<C>>(std::env::var("GOTCHA_ACTIVE_PROFILE").ok())?)
}

/// Default empty configuration for simple applications
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EmptyConfig {}

/// Default empty state for simple applications  
#[derive(Clone, Debug, Default)]
pub struct EmptyState {}

/// Builder for creating Gotcha applications with a fluent API
pub struct Gotcha<S = EmptyState, C = EmptyConfig>
where
    S: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    router: GotchaRouter<GotchaContext<S, C>>,
    app_layers: Vec<AppLayer>,
    #[cfg(feature = "openapi")]
    openapi_endpoints: Option<crate::OpenApiEndpoints>,
    listen_options: ListenOptions,
    config_error_policy: ConfigErrorPolicy<C>,
    shutdown_signal: Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>,
    state: StateSource<S>,
    config: Configuration<C>,
    #[cfg(feature = "task")]
    tasks: Vec<TaskRegistrar<S, C>>,
    #[cfg(feature = "task")]
    task_shutdown_timeout: std::time::Duration,
}

impl Default for Gotcha<EmptyState, EmptyConfig> {
    fn default() -> Self {
        Self::new()
    }
}

impl Gotcha<EmptyState, EmptyConfig> {
    /// Create a new Gotcha builder with default empty state and config
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .get("/", || async { "Hello World" });
    /// ```
    pub fn new() -> Self {
        Self::with_types::<EmptyState, EmptyConfig>()
    }
}

impl Gotcha {
    /// Create a new Gotcha builder with custom state and config types
    ///
    /// This is a convenience method that allows you to specify custom types
    /// without needing to provide dummy type parameters.
    /// Defaults and configuration loading are deferred until startup. For already initialized
    /// values, use [`Self::from_state`] or [`Self::from_context`] without those bounds.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    /// use serde::{Deserialize, Serialize};
    ///
    /// #[state]
    /// #[derive(Clone, Default)]
    /// struct AppState {}
    ///
    /// #[derive(Clone, Default, Serialize, Deserialize)]
    /// struct AppConfig {}
    ///
    /// let app = Gotcha::with_types::<AppState, AppConfig>()
    ///     .get("/", |State(_): State<AppState>| async move { "Hello with custom state" });
    /// ```
    pub fn with_types<S, C>() -> Gotcha<S, C>
    where
        S: Clone + Send + Sync + 'static + Default,
        C: Clone + Send + Sync + 'static + DeserializeOwned,
    {
        Gotcha::from_sources(StateSource::Default(S::default), Configuration::automatic())
    }

    /// Create a Gotcha builder with custom state type and default config
    /// Use [`Self::from_state`] when the state has no meaningful `Default`.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// #[state]
    /// #[derive(Clone, Default)]
    /// struct AppState {
    ///     counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// }
    ///
    /// let app = Gotcha::with_state::<AppState>()
    ///     .state(AppState::default())
    ///     .get("/", |State(_): State<AppState>| async move { "Hello with custom state" });
    /// ```
    pub fn with_state<S>() -> Gotcha<S, EmptyConfig>
    where
        S: Clone + Send + Sync + 'static + Default,
    {
        Self::with_types::<S, EmptyConfig>()
    }

    /// Create a Gotcha builder with custom config type and default state
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    /// use serde::{Deserialize, Serialize};
    ///
    /// #[derive(Clone, Default, Serialize, Deserialize)]
    /// struct AppConfig {
    ///     api_key: String,
    ///     max_connections: u32,
    /// }
    ///
    /// let app = Gotcha::with_config::<AppConfig>()
    ///     .with_env_config("APP")
    ///     .get("/", |config: State<ConfigWrapper<AppConfig>>| async move {
    ///         "Hello with custom config"
    ///     });
    /// ```
    pub fn with_config<C>() -> Gotcha<EmptyState, C>
    where
        C: Clone + Send + Sync + 'static + DeserializeOwned,
    {
        Self::with_types::<EmptyState, C>()
    }

    /// Create an application from initialized state, with automatic empty configuration.
    /// The state does not need `Default`. Use [`Self::from_context`] to also supply configuration.
    ///
    /// ```no_run
    /// use gotcha::{Gotcha, State, state};
    /// #[state]
    /// #[derive(Clone)]
    /// struct AppState { started_at: std::time::Instant }
    /// let app = Gotcha::from_state(AppState { started_at: std::time::Instant::now() })
    ///     .get("/uptime", |State(state): State<AppState>| async move {
    ///         state.started_at.elapsed().as_secs().to_string()
    ///     });
    /// ```
    pub fn from_state<S>(state: S) -> Gotcha<S, EmptyConfig>
    where
        S: Clone + Send + Sync + 'static,
    {
        Gotcha::from_sources(StateSource::Provided(state), Configuration::automatic())
    }

    /// Create an application from initialized state and configuration, without loading files.
    /// Neither type needs `Default`; the configuration does not need serde traits.
    /// `run()` uses the supplied server settings unless `host`/`port` or `listen` override them.
    pub fn from_context<S, C>(context: GotchaContext<S, C>) -> Gotcha<S, C>
    where
        S: Clone + Send + Sync + 'static,
        C: Clone + Send + Sync + 'static,
    {
        Gotcha::from_sources(StateSource::Provided(context.state), Configuration::Provided(context.config))
    }
}

impl<S, C> Gotcha<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn from_sources(state: StateSource<S>, config: Configuration<C>) -> Self {
        Self {
            router: GotchaRouter::default(),
            app_layers: Vec::new(),
            #[cfg(feature = "openapi")]
            openapi_endpoints: None,
            listen_options: ListenOptions::default(),
            config_error_policy: ConfigErrorPolicy::Strict,
            shutdown_signal: None,
            state,
            config,
            #[cfg(feature = "task")]
            tasks: Vec::new(),
            #[cfg(feature = "task")]
            task_shutdown_timeout: crate::task::DEFAULT_SHUTDOWN_TIMEOUT,
        }
    }

    /// Set the application state
    pub fn state(mut self, state: S) -> Self {
        self.state = StateSource::Provided(state);
        self
    }

    /// Set the application configuration
    pub fn config(mut self, config: ConfigWrapper<C>) -> Self {
        self.config = Configuration::Provided(config);
        self
    }

    /// Choose how startup handles configuration loading errors. The default is strict.
    /// Applies to automatic and registered sources, but not eager `build_config()` calls.
    ///
    /// ```no_run
    /// use gotcha::{ConfigErrorPolicy, Gotcha};
    /// let app = Gotcha::new().config_error_policy(ConfigErrorPolicy::fallback_to_default());
    /// ```
    pub fn config_error_policy(mut self, policy: ConfigErrorPolicy<C>) -> Self {
        self.config_error_policy = policy;
        self
    }

    /// Build configuration using a custom configuration builder function
    ///
    /// This replaces any existing configuration sources with the result of the builder function.
    /// Use the individual methods (with_env_config, with_file_config, etc.) for cumulative configuration.
    ///
    /// # Example
    /// ```no_run
    /// # fn demo() -> gotcha::GotchaResult<()> {
    /// use gotcha::prelude::*;
    ///
    /// let _app = Gotcha::new().build_config(|builder| builder.file_optional("config.toml").env("APP"))?;
    /// # Ok(()) }
    /// ```
    pub fn build_config<F>(mut self, builder_fn: F) -> GotchaResult<Self>
    where
        F: FnOnce(ConfigBuilder) -> ConfigBuilder,
        C: DeserializeOwned,
    {
        let builder = Config::builder();
        let configured_builder = builder_fn(builder);
        let config: ConfigWrapper<C> = configured_builder.build()?;
        self.config = Configuration::Provided(config);
        Ok(self)
    }

    /// Add default configuration sources (files + environment variables)
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Equivalent to calling `.with_default_files().with_default_env()`
    /// Errors from these explicitly selected sources are returned when the server starts.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_default_config();
    /// ```
    pub fn with_default_config(self) -> Self {
        self.with_default_files().with_default_env()
    }

    /// Add environment variable configuration source
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Multiple calls will add multiple environment prefixes.
    /// Later sources override earlier values; loading errors are returned at server startup.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_env_config("APP")
    ///     .with_env_config("GOTCHA"); // Both prefixes will be used
    /// ```
    pub fn with_env_config<P: AsRef<str>>(mut self, prefix: P) -> Self {
        self.config
            .update_sources(|builder| builder.unwrap_or_else(|| ConfigBuilder::new().enable_vars()).env(prefix.as_ref()));
        self
    }

    /// Add a required configuration file source
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Multiple calls will add multiple file sources.
    /// Later sources override earlier values. A missing, unreadable, or invalid file fails startup.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_file_config("config.toml")
    ///     .with_file_config("local.toml"); // Both files will be loaded
    /// ```
    pub fn with_file_config<P: AsRef<std::path::Path>>(mut self, path: P) -> Self {
        self.config
            .update_sources(|builder| builder.unwrap_or_else(|| ConfigBuilder::new().enable_vars()).file(path));
        self
    }

    /// Add an optional configuration file source (won't fail if file doesn't exist)
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Multiple calls will add multiple optional file sources.
    /// Only missing files are ignored; unreadable or invalid files fail startup.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_optional_config("config.toml")
    ///     .with_optional_config("local.toml"); // Both files will be loaded if they exist
    /// ```
    pub fn with_optional_config<P: AsRef<std::path::Path>>(mut self, path: P) -> Self {
        self.config
            .update_sources(|builder| builder.unwrap_or_else(|| ConfigBuilder::new().enable_vars()).file_optional(path));
        self
    }

    /// Add default configuration files (configurations/application.toml and profile-specific files)
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Missing default files are optional; existing files that cannot be loaded fail startup.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_default_files();
    /// ```
    pub fn with_default_files(mut self) -> Self {
        self.config.update_sources(|builder| {
            let mut builder = builder.unwrap_or_default().file_optional("configurations/application.toml");
            if let Ok(profile) = std::env::var("GOTCHA_ACTIVE_PROFILE") {
                builder = builder.file_optional(format!("configurations/application_{}.toml", profile));
            }
            builder
        });
        self
    }

    /// Add default environment variable prefix ("APP")
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_default_env();
    /// ```
    pub fn with_default_env(mut self) -> Self {
        self.config.update_sources(|builder| builder.unwrap_or_default().env("APP"));
        self
    }

    /// Enable variable substitution (${VAR} and ${VAR:-default} syntax)
    ///
    /// This affects all configuration sources added to the builder.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_optional_config("config.toml")
    ///     .enable_variable_substitution();
    /// ```
    pub fn enable_variable_substitution(mut self) -> Self {
        self.config.update_sources(|builder| builder.unwrap_or_default().enable_vars());
        self
    }

    /// Set the host address
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .host("0.0.0.0")
    ///     .port(8080);
    /// ```
    pub fn host<H: Into<String>>(mut self, host: H) -> Self {
        self.listen_options.host = Some(host.into());
        self
    }

    /// Set the port number
    pub fn port(mut self, port: u16) -> Self {
        self.listen_options.port = Some(port);
        self
    }

    /// Add a GET route
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .get("/", || async { "Hello World" })
    ///     .get("/users/{id}", get_user);
    ///
    /// async fn get_user() -> impl Responder {
    ///     "User info"
    /// }
    /// ```
    pub fn get<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.get(path, handler);
        self
    }

    /// Add a POST route
    pub fn post<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.post(path, handler);
        self
    }

    /// Add a PUT route
    pub fn put<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.put(path, handler);
        self
    }

    /// Add a DELETE route
    pub fn delete<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.delete(path, handler);
        self
    }

    /// Add a PATCH route
    pub fn patch<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.patch(path, handler);
        self
    }

    /// Add a HEAD route
    pub fn head<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.head(path, handler);
        self
    }

    /// Add an OPTIONS route
    pub fn options<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.options(path, handler);
        self
    }

    /// Add a TRACE route
    pub fn trace<H, T>(mut self, path: &str, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.trace(path, handler);
        self
    }

    /// Add composed methods with their annotated handlers' OpenAPI metadata.
    /// See [`crate::routing`] for constructors such as `get(handler).post(handler)`.
    pub fn route(mut self, path: &str, method_router: MethodRouter<GotchaContext<S, C>>) -> Self {
        self.router = self.router.route(path, method_router);
        self
    }

    /// Add a native Axum method router without generating OpenAPI operations, even for
    /// `#[api]` handlers. Use [`route`](Self::route) with [`crate::routing`] to retain metadata.
    pub fn route_raw(mut self, path: &str, method_router: AxumMethodRouter<GotchaContext<S, C>>) -> Self {
        self.router = self.router.route_raw(path, method_router);
        self
    }

    /// Add multiple routes using a closure
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    /// # async fn home_handler() -> impl Responder { "" }
    /// # async fn about_handler() -> impl Responder { "" }
    /// # async fn create_user() -> impl Responder { "" }
    ///
    /// let app = Gotcha::new()
    ///     .routes(|router| {
    ///         router
    ///             .get("/", home_handler)
    ///             .get("/about", about_handler)
    ///             .post("/users", create_user)
    ///     });
    /// ```
    pub fn routes<F>(mut self, routes_fn: F) -> Self
    where
        F: FnOnce(GotchaRouter<GotchaContext<S, C>>) -> GotchaRouter<GotchaContext<S, C>>,
    {
        self.router = routes_fn(self.router);
        self
    }

    /// Nest a route module at a path, using this application's state and configuration.
    ///
    /// The module carries routes, middleware, and OpenAPI metadata. Configure state,
    /// configuration sources, listening addresses, and background tasks on the application.
    ///
    /// With `openapi`, retains the child's transforms and runs them on the complete document
    /// before this application's own transforms.
    ///
    /// ```rust,no_run
    /// use gotcha::{Gotcha, GotchaRouter};
    /// let app = Gotcha::new()
    ///     .nest("/api", GotchaRouter::default().get("/items", || async { "items" }));
    /// ```
    ///
    /// A complete application is rejected rather than silently discarding its settings:
    ///
    /// ```compile_fail
    /// use gotcha::Gotcha;
    /// let child = Gotcha::new().port(9000).config(Default::default());
    /// let app = Gotcha::new().nest("/api", child);
    /// ```
    pub fn nest(mut self, path: &str, router: GotchaRouter<GotchaContext<S, C>>) -> Self {
        self.router = self.router.nest(path, router);
        self
    }

    /// Merge a route module using this application's state and configuration.
    ///
    /// As with [`nest`](Self::nest), application settings and tasks belong to the top level.
    ///
    /// With `openapi`, treats `router` as a child for transform ordering: its transforms run
    /// on the complete document before this application's own transforms.
    ///
    /// ```rust,no_run
    /// use gotcha::{Gotcha, GotchaRouter};
    /// let app = Gotcha::new()
    ///     .merge(GotchaRouter::default().get("/health", || async { "ok" }));
    /// ```
    ///
    /// A complete child application, including one with tasks, cannot be merged:
    ///
    /// ```compile_fail
    /// use gotcha::Gotcha;
    /// let child = Gotcha::new();
    /// # #[cfg(feature = "task")]
    /// let child = child.tasks(|_scheduler| {});
    /// let app = Gotcha::new().merge(child);
    /// ```
    pub fn merge(mut self, router: GotchaRouter<GotchaContext<S, C>>) -> Self {
        self.router = self.router.merge(router);
        self
    }

    /// Apply a layer to business routes already registered, following Axum's ordering.
    /// Documentation endpoints are not covered. Use [`Self::app_layer`] for every endpoint.
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<std::convert::Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        self.router = self.router.layer(layer);
        self
    }

    /// Apply middleware to the final application, including documentation and fallback routes.
    ///
    /// Layers are deferred until all routes are assembled, so this covers routes registered
    /// before or after this call. Repeated calls follow Axum's layering order: the last layer
    /// receives the request first. The layer is installed once, not reconstructed per request.
    /// Use [`Self::layer`] to cover only the business routes registered so far.
    pub fn app_layer<L>(mut self, layer: L) -> Self
    where
        L: Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<std::convert::Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        self.app_layers.push(Box::new(move |router| router.layer(layer)));
        self
    }

    /// Set a fallback handler for requests that don't match any route
    pub fn fallback<H, T>(mut self, handler: H) -> Self
    where
        H: Handler<T, GotchaContext<S, C>>,
        T: 'static,
    {
        self.router = self.router.fallback(handler);
        self
    }

    /// Customize the generated OpenAPI document when it is exported or served.
    ///
    /// Repeated calls append transforms. Child subtrees run in `nest`/`merge` insertion order,
    /// then this application's own transforms run in registration order, once per document.
    /// This does not enable HTTP documentation. Without serving or exporting a document,
    /// callbacks are not executed.
    /// All transforms receive the complete document; top-level security is global even when
    /// set by a child. See [`GotchaRouter::openapi`](crate::GotchaRouter::openapi) for scope and
    /// conflict rules, shared with the trait API.
    ///
    /// ```rust,no_run
    /// use gotcha::Gotcha;
    ///
    /// let app = Gotcha::new().openapi(|mut spec| {
    ///     spec.info.title = "My API".to_string();
    ///     spec.info.version = "2.0.0".to_string();
    ///     spec
    /// });
    /// ```
    #[cfg(feature = "openapi")]
    pub fn openapi<F>(mut self, transform: F) -> Self
    where
        F: FnOnce(oas::OpenAPIV3) -> oas::OpenAPIV3 + Send + 'static,
    {
        self.router = self.router.openapi(transform);
        self
    }

    /// Register background tasks (requires the `task` feature).
    ///
    /// The closure receives a [`TaskScheduler`](crate::TaskScheduler) when the
    /// server starts, on which you can register `cron` / `interval` jobs. This
    /// brings the builder to parity with `GotchaApp::tasks`. Jobs start only after all
    /// registrations succeed and are owned by the application until shutdown.
    /// Register module tasks here on the top-level application; `nest` and `merge` accept
    /// only route modules and cannot accept another application's task registrations.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    /// use std::time::Duration;
    ///
    /// let app = Gotcha::new().tasks(|scheduler| {
    ///     scheduler.interval("heartbeat", Duration::from_secs(60), |_ctx| async move {
    ///         tracing::info!("tick");
    ///     });
    /// });
    /// ```
    #[cfg(feature = "task")]
    pub fn tasks<F>(mut self, register: F) -> Self
    where
        F: FnOnce(&mut crate::TaskScheduler<S, C>) + Send + 'static,
    {
        self.tasks.push(Box::new(register));
        self
    }

    /// Replace the default Ctrl-C / Unix SIGTERM signal. When this future resolves, HTTP
    /// stops accepting connections and scheduled tasks stop starting new executions.
    pub fn shutdown_signal<F>(mut self, signal: F) -> Self
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.shutdown_signal = Some(Box::pin(signal));
        self
    }

    /// Set the grace period for in-flight scheduled executions (default: 30 seconds).
    /// Remaining tasks are aborted and joined at the deadline. HTTP draining has no time limit here.
    #[cfg(feature = "task")]
    pub fn task_shutdown_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.task_shutdown_timeout = timeout;
        self
    }

    /// Add CORS support (requires "cors" feature)
    #[cfg(feature = "cors")]
    pub fn with_cors(self) -> Self {
        use crate::layers::CorsLayer;
        self.layer(CorsLayer::permissive())
    }

    /// Enable the default OpenAPI JSON, Redoc, and Scalar endpoints.
    ///
    /// The `openapi` feature alone does not expose HTTP documentation. This replaces any
    /// previous endpoint configuration with [`crate::OpenApiEndpoints::default`]. Business
    /// `layer` middleware does not cover these endpoints; `app_layer` middleware does.
    #[cfg(feature = "openapi")]
    pub fn with_openapi(self) -> Self {
        self.openapi_endpoints(Some(crate::OpenApiEndpoints::default()))
    }

    /// Configure documentation paths, or pass `None` to disable every documentation endpoint.
    /// The last configuration call wins. This is application configuration, not route metadata.
    #[cfg(feature = "openapi")]
    pub fn openapi_endpoints(mut self, endpoints: Option<crate::OpenApiEndpoints>) -> Self {
        self.openapi_endpoints = endpoints;
        self
    }

    /// Consume the application builder and export its complete OpenAPI document without HTTP.
    ///
    /// Uses the same generation and transform pipeline as documentation endpoints. Does not
    /// load configuration, construct default state, register tasks, install application layers,
    /// or start a server. Registered routes and `FnOnce` transforms are consumed; endpoint
    /// settings do not affect the document. User transforms execute normally.
    #[cfg(feature = "openapi")]
    pub fn into_openapi(self) -> oas::OpenAPIV3 {
        self.router.into_openapi()
    }

    /// Start the server and listen on the configured address
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     Gotcha::new()
    ///         .get("/", || async { "Hello World" })
    ///         .listen("127.0.0.1:3000")
    ///         .await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn listen<A>(self, addr: A) -> GotchaResult<()>
    where
        A: AsRef<str>,
    {
        let addr_str = addr.as_ref();
        let socket_addr: SocketAddr = addr_str.parse().map_err(|_| GotchaError::InvalidAddress(addr_str.to_string()))?;
        self.listen_on(socket_addr).await
    }

    /// Start the server on a specific socket address
    pub async fn listen_on(self, addr: SocketAddr) -> GotchaResult<()> {
        startup::run(self, Some(addr)).await
    }

    /// Start using `host`/`port` overrides, then loaded server settings, then framework defaults.
    /// Bind before initializing state, assembling routes, or registering tasks. The context
    /// contains the effective bound address, including the port chosen for port 0.
    pub async fn run(self) -> GotchaResult<()> {
        startup::run(self, None).await
    }
}

impl<S, C> startup::Application for Gotcha<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    type State = S;
    type Config = C;

    fn listen_options(&mut self) -> ListenOptions {
        std::mem::take(&mut self.listen_options)
    }

    fn config_error_policy(&mut self) -> ConfigErrorPolicy<C> {
        std::mem::take(&mut self.config_error_policy)
    }

    async fn shutdown_signal(&mut self) {
        match self.shutdown_signal.take() {
            Some(signal) => signal.await,
            None => startup::shutdown_signal().await,
        }
    }

    #[cfg(feature = "task")]
    fn task_shutdown_timeout(&self) -> std::time::Duration {
        self.task_shutdown_timeout
    }

    async fn config(&mut self) -> GotchaResult<ConfigWrapper<C>> {
        self.config.resolve()
    }

    async fn state(&mut self, _: &ConfigWrapper<C>) -> GotchaResult<S> {
        Ok(self.state.resolve())
    }

    async fn router(&mut self, context: GotchaContext<S, C>) -> GotchaResult<axum::Router> {
        crate::assembly::assemble(
            std::mem::take(&mut self.router),
            context,
            #[cfg(feature = "openapi")]
            self.openapi_endpoints.take(),
            |router| std::mem::take(&mut self.app_layers).into_iter().fold(router, |router, layer| layer(router)),
        )
    }

    #[cfg(feature = "task")]
    async fn tasks(&mut self, scheduler: &mut crate::TaskScheduler<S, C>) -> GotchaResult<()> {
        for register in std::mem::take(&mut self.tasks) {
            register(scheduler);
        }
        Ok(())
    }
}

// Convenience methods for empty state and config
impl Gotcha<EmptyState, EmptyConfig> {
    /// Quick start method for simple applications
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     Gotcha::quick_start()
    ///         .await?
    ///         .get("/", || async { "Hello World" })
    ///         .run()
    ///         .await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn quick_start() -> GotchaResult<Self> {
        tracing_subscriber::fmt::init();
        Ok(Self::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerConfig;

    #[tokio::test]
    async fn provided_state_does_not_construct_the_unused_default() {
        #[derive(Clone)]
        struct Initialized;
        impl Default for Initialized {
            fn default() -> Self {
                panic!("state was already initialized");
            }
        }
        let mut app = Gotcha::with_state::<Initialized>().state(Initialized).config(ConfigWrapper::default());
        startup::prepare(&mut app, Some("127.0.0.1:0".parse().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn nested_and_merged_routes_share_the_application_context() {
        use axum::{
            body::{to_bytes, Body},
            extract::State,
        };
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use tower::ServiceExt;

        #[derive(Clone)]
        struct RuntimeState(Arc<AtomicUsize>);
        #[derive(Clone)]
        struct RuntimeConfig {
            value: String,
        }
        async fn read(State(context): State<GotchaContext<RuntimeState, RuntimeConfig>>) -> String {
            format!("{}:{}", context.config.value, context.state.0.fetch_add(1, Ordering::SeqCst))
        }

        let calls = Arc::new(AtomicUsize::new(41));
        let mut app = Gotcha::from_context(GotchaContext {
            state: RuntimeState(calls.clone()),
            config: ConfigWrapper {
                app: RuntimeConfig { value: "parent".into() },
                server: ServerConfig::default(),
            },
        })
        .with_file_config("unused-required-file.toml")
        .nest("/api", GotchaRouter::default().get("/context", read))
        .merge(GotchaRouter::default().get("/context", read));
        let prepared = startup::prepare(&mut app, Some("127.0.0.1:0".parse().unwrap())).await.unwrap();
        let router = prepared.router;

        for (path, expected) in [("/api/context", "parent:41"), ("/context", "parent:42")] {
            let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), expected);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 43, "both modules update the application's shared state");
    }

    #[tokio::test]
    async fn builder_accepts_composed_and_raw_method_routers() {
        use axum::body::{to_bytes, Body};
        use tower::ServiceExt;

        // The public builder delegates both entry points, with its application context type.
        let app = Gotcha::new()
            .route("/composed", crate::get(|| async { "get" }).post(|| async { "post" }))
            .route_raw("/raw", axum::routing::get(|| async { "raw" }));
        let router = app.router.into_axum_router(GotchaContext {
            state: EmptyState::default(),
            config: ConfigWrapper::default(),
        });
        for (method, path, expected) in [("GET", "/composed", "get"), ("POST", "/composed", "post"), ("GET", "/raw", "raw")] {
            let response = router
                .clone()
                .oneshot(Request::builder().method(method).uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), expected);
        }
    }

    #[cfg(feature = "openapi")]
    #[test]
    fn builder_forwards_all_openapi_transforms_to_router_assembly() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
        let sink = captured.clone();
        let app = Gotcha::new()
            .openapi(|mut spec| {
                spec.info.title.push_str(" parent");
                spec
            })
            .nest(
                "/child",
                GotchaRouter::default().openapi(|mut spec| {
                    spec.info.title = "nested".into();
                    spec
                }),
            )
            .merge(GotchaRouter::default().openapi(|mut spec| {
                spec.info.title.push_str(" merged");
                spec
            }))
            .openapi(move |spec| {
                *sink.lock().unwrap() = Some(spec.info.title.clone());
                spec
            });
        let _ = app.into_openapi();
        assert_eq!(captured.lock().unwrap().as_deref(), Some("nested merged parent"));
    }

    #[derive(Clone, Default, Serialize, Deserialize)]
    struct TestConfig {
        value: String,
        reference: String,
    }

    #[test]
    fn configuration_sources_preserve_order_and_explicit_error_policy() {
        const CHILD: &str = "GOTCHA_CONFIG_BUILDER_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir(dir.path().join("configurations")).unwrap();
            std::fs::write(dir.path().join("configurations/application.toml"), "value = 'base'\nreference = '${value}'").unwrap();
            std::fs::write(dir.path().join("configurations/application_review.toml"), "value = 'profile'").unwrap();
            std::fs::write(dir.path().join("override.toml"), "value = 'override'").unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            // Keep process-global environment and cwd changes outside the parent test process.
            for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("APP_")) {
                command.env_remove(key);
            }
            let output = command
                .args([
                    "--exact",
                    "builder::tests::configuration_sources_preserve_order_and_explicit_error_policy",
                    "--nocapture",
                ])
                .current_dir(dir.path())
                .env(CHILD, "1")
                .env("GOTCHA_ACTIVE_PROFILE", "review")
                .env("APP_VALUE", "default-env")
                .env("GOTCHABUILDER_VALUE", "custom-env")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "child failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let app = || Gotcha::with_config::<TestConfig>();
        for (builder, expected) in [
            (app().with_default_config(), "default-env"),
            (app().with_default_env().with_default_files(), "profile"),
            (
                app()
                    .with_file_config("configurations/application.toml")
                    .with_env_config("GOTCHABUILDER")
                    .with_optional_config("override.toml"),
                "override",
            ),
            (
                app()
                    .with_file_config("configurations/application.toml")
                    .with_optional_config("override.toml")
                    .with_env_config("GOTCHABUILDER"),
                "custom-env",
            ),
        ] {
            let config = builder.enable_variable_substitution().config.resolve().unwrap();
            assert_eq!(config.value, expected);
            assert_eq!(config.reference, expected, "variables use the final merged configuration");
        }

        std::fs::write("configurations/application.toml", "value = [").unwrap();
        assert!(matches!(app().with_default_config().config.resolve(), Err(GotchaError::Config(_))));
        assert!(matches!(app().config.resolve(), Err(GotchaError::Config(_))));
    }
}
