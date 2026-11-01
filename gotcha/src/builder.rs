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

use crate::config::{ConfigBuilder, ConfigErrorPolicy, ConfigWrapper, GotchaConfigLoader};
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

/// A post-assembly hook with access to the application context. See [`Gotcha::finish_router`].
type FinishRouter<S, C> = Box<dyn FnOnce(axum::Router, &GotchaContext<S, C>) -> axum::Router + Send>;

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

/// Cleanup registered with [`Gotcha::on_shutdown`]. Owns a context clone so the returned future
/// can outlive the borrow.
type ShutdownHook<S, C> = Box<dyn FnOnce(GotchaContext<S, C>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send>;

/// Aborts a running cleanup hook if the shutdown driver is dropped before the hook finishes.
struct CleanupTask(Option<tokio::task::JoinHandle<()>>);

impl CleanupTask {
    async fn join(mut self) -> Result<(), tokio::task::JoinError> {
        // Keep the handle inside `self` while awaiting, so cancelling this future (for example by
        // dropping the serving future) drops the `CleanupTask` and aborts the running hook.
        let handle = self.0.as_mut().expect("cleanup task handle was already taken");
        std::future::poll_fn(|cx| std::future::Future::poll(std::pin::Pin::new(&mut *handle), cx)).await
    }
}

impl Drop for CleanupTask {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
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
    finish_routers: Vec<FinishRouter<S, C>>,
    #[cfg(feature = "openapi")]
    openapi_endpoints: Option<crate::OpenApiEndpoints>,
    #[cfg(feature = "prometheus")]
    metrics_endpoint: Option<crate::MetricsEndpoint>,
    listen_options: ListenOptions,
    config_error_policy: ConfigErrorPolicy<C>,
    shutdown_signal: Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>,
    shutdown_hooks: Vec<ShutdownHook<S, C>>,
    http_shutdown_timeout: std::time::Duration,
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
    ///
    /// **The supplied context's shutdown token is discarded.** The application receives the
    /// server-owned token when it starts, so handlers and `on_shutdown` hooks see the live one.
    /// A context built with [`GotchaContext::new`] only carries a token for direct router
    /// assembly, where no server owns it.
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
            finish_routers: Vec::new(),
            #[cfg(feature = "openapi")]
            openapi_endpoints: None,
            #[cfg(feature = "prometheus")]
            metrics_endpoint: None,
            listen_options: ListenOptions::default(),
            config_error_policy: ConfigErrorPolicy::Strict,
            shutdown_signal: None,
            shutdown_hooks: Vec::new(),
            http_shutdown_timeout: startup::DEFAULT_HTTP_SHUTDOWN_TIMEOUT,
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
    /// The builder function starts above the framework's typed server defaults, so it can add
    /// application defaults with [`ConfigBuilder::defaults`](crate::config::ConfigBuilder::defaults)
    /// and still let files and environment variables override them.
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
        let builder = ConfigBuilder::framework_defaults();
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
    /// The framework's typed server defaults always sit beneath these sources, so an
    /// environment-only `APP_SERVER__PORT` is coerced without a configuration file.
    /// `${VAR}` and path variables in these sources are resolved, as in automatic loading.
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
            .update_sources(|builder| builder.unwrap_or_else(ConfigBuilder::framework_defaults).enable_vars().env(prefix.as_ref()));
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
            .update_sources(|builder| builder.unwrap_or_else(ConfigBuilder::framework_defaults).enable_vars().file(path));
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
            .update_sources(|builder| builder.unwrap_or_else(ConfigBuilder::framework_defaults).enable_vars().file_optional(path));
        self
    }

    /// Add default configuration files (configurations/application.toml and profile-specific files)
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Missing default files are optional; existing files that cannot be loaded fail startup.
    /// Variable substitution is enabled for these sources, as for automatic loading.
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
            let mut builder = builder
                .unwrap_or_else(ConfigBuilder::framework_defaults)
                .enable_vars()
                .file_optional("configurations/application.toml");
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
    /// Variable substitution is enabled for these sources, as for automatic loading.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_default_env();
    /// ```
    pub fn with_default_env(mut self) -> Self {
        self.config
            .update_sources(|builder| builder.unwrap_or_else(ConfigBuilder::framework_defaults).enable_vars().env("APP"));
        self
    }

    /// Enable variable substitution
    ///
    /// Values may reference environment variables with `${VAR}` and other configuration paths
    /// with `${app.database.name}`. A variable or path that is not set resolves to an empty
    /// string; substitution does not fail the load. This affects all configuration sources
    /// added to the builder.
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
        self.config
            .update_sources(|builder| builder.unwrap_or_else(ConfigBuilder::framework_defaults).enable_vars());
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

    /// Replace one extractor provider's rejection response.
    ///
    /// Configurations default to the framework's handlers (the original status with a JSON
    /// `{"error": "..."}` body, or `422` with `{"error": "validation failed", "fields": {...}}` for
    /// [`ValidConfig`](crate::ValidConfig)). Each provider is configured independently; this
    /// method is the builder equivalent of
    /// [`GotchaApp::extractor_configs`](crate::GotchaApp::extractor_configs).
    ///
    /// ```rust,no_run
    /// use gotcha::{Gotcha, JsonConfig};
    ///
    /// let app = Gotcha::new()
    ///     .extractor_config(JsonConfig::default().error_handler(|rejection| {
    ///         (gotcha::axum::http::StatusCode::BAD_REQUEST, gotcha::ErrorBody::new(rejection.body_text()))
    ///     }))
    ///     .post("/items", || async { "created" });
    /// ```
    pub fn extractor_config<Cfg: crate::extract::ExtractorConfig>(mut self, config: Cfg) -> Self {
        self.router = self.router.extractor_config(config);
        self
    }

    /// Include or omit the rejection responses of Gotcha's extractor providers in OpenAPI.
    ///
    /// They are documented by default. Pass `false` to document only the responses inferred from
    /// the handler's return type and declared in `#[api]`. The builder equivalent of
    /// [`GotchaApp::document_extractor_errors`](crate::GotchaApp::document_extractor_errors);
    /// the setting applies to served and exported documents.
    ///
    /// ```rust,no_run
    /// use gotcha::{Gotcha, Json};
    ///
    /// let app = Gotcha::new()
    ///     .document_extractor_errors(false)
    ///     .post("/items", |_: Json<String>| async { "created" });
    /// ```
    #[cfg(feature = "openapi")]
    pub fn document_extractor_errors(mut self, document: bool) -> Self {
        self.router = self.router.document_extractor_errors(document);
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

    /// Apply a layer only to business routes already registered, following Axum's ordering.
    ///
    /// Requests that match no route bypass the layer entirely, including the fallback, and
    /// documentation endpoints are not covered. Use [`Self::app_layer`] for every endpoint.
    ///
    /// As in Axum, this panics if no routes have been registered yet; register the routes the
    /// layer should cover first.
    pub fn route_layer<L>(mut self, layer: L) -> Self
    where
        L: Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<std::convert::Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        self.router = self.router.route_layer(layer);
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

    /// Register a post-assembly hook that receives the assembled router and a clone of the
    /// application context. This is the builder equivalent of
    /// [`GotchaApp::finish_router`](crate::GotchaApp::finish_router).
    ///
    /// Hooks run once when the application starts, after business routes, fallback, and
    /// documentation endpoints are mounted, in registration order. Use them to mount services
    /// that need state at construction time, for example a proxy or file handler closing over
    /// `context.state` via `nest_service` or `fallback_service`. Routes and services added here
    /// use native Axum routing and are not included in OpenAPI metadata.
    ///
    /// [`Self::app_layer`] middleware is installed after these hooks, regardless of registration
    /// order, so it also covers everything a hook mounts. Hooks do not run during
    /// [`Self::into_openapi`], which never assembles the application.
    ///
    /// ```rust,ignore
    /// use gotcha::{Gotcha, GotchaContext};
    ///
    /// let app = Gotcha::from_state(())
    ///     .get("/health", || async { "ok" })
    ///     .finish_router(|router, context: &GotchaContext<(), gotcha::EmptyConfig>| {
    ///         // let proxy = Proxy::new(context.state.clone());
    ///         // router.nest_service("/proxy", proxy)
    ///         router
    ///     });
    /// ```
    pub fn finish_router<F>(mut self, finish: F) -> Self
    where
        F: FnOnce(axum::Router, &GotchaContext<S, C>) -> axum::Router + Send + 'static,
    {
        self.finish_routers.push(Box::new(finish));
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

    /// Mount a tower `Service` at `path`, handling every request below that prefix.
    ///
    /// The service is opaque to OpenAPI: neither the prefix nor its responses appear in the
    /// generated document. It also does not receive the application context; build it from state
    /// in [`Self::finish_router`] when it needs application state. See
    /// [`GotchaRouter::nest_service`](crate::GotchaRouter::nest_service) for the full semantics,
    /// including how the nested prefix is stripped from the request.
    pub fn nest_service<T>(mut self, path: &str, service: T) -> Self
    where
        T: Service<Request, Error = std::convert::Infallible> + Clone + Send + Sync + 'static,
        T::Response: Responder,
        T::Future: Send + 'static,
    {
        self.router = self.router.nest_service(path, service);
        self
    }

    /// Customize the generated OpenAPI document when it is exported or served.
    ///
    /// Transforms run after the `info` block set by [`Self::openapi_info`], so they can override
    /// it along with anything else in the document.
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

    /// Set the [`oas::Info`] block of the generated OpenAPI document.
    ///
    /// The default is neutral rather than gotcha's own identity: `title: "API"`,
    /// `version: "0.1.0"`, no description, and every optional field `None`. The same block is
    /// used by documentation endpoints and [`Self::into_openapi`]. Use
    /// [`cargo_info!`](crate::cargo_info) to fill it from the application crate's Cargo metadata:
    ///
    /// ```rust,no_run
    /// use gotcha::Gotcha;
    ///
    /// let app = Gotcha::new().openapi_info(gotcha::cargo_info!());
    /// ```
    ///
    /// `Info` describes the whole application, so `nest` and `merge` keep the parent's value.
    /// It is applied before [`Self::openapi`] transforms, which can still override it.
    #[cfg(feature = "openapi")]
    pub fn openapi_info(mut self, info: oas::Info) -> Self {
        self.router = self.router.openapi_info(info);
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

    /// Replace the default Ctrl-C / Unix SIGTERM signal. When this future resolves, the context
    /// shutdown token is cancelled, HTTP stops accepting connections, and scheduled tasks stop
    /// starting new executions.
    ///
    /// The future is polled on the serving task, so it must not block the thread: blocking work
    /// starves the server it is meant to stop. Use an async primitive such as
    /// [`tokio::sync::Notify`] or a channel.
    pub fn shutdown_signal<F>(mut self, signal: F) -> Self
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.shutdown_signal = Some(Box::pin(signal));
        self
    }

    /// Set the grace period for in-flight HTTP connections (default: 30 seconds).
    ///
    /// At the deadline, remaining connections (including WebSocket upgrades, which axum does not
    /// track after the upgrade) are shut down at the OS socket level; handlers get up to one
    /// second to observe the closure before cleanup runs. This deadline runs concurrently with,
    /// and independently of, `task_shutdown_timeout`.
    pub fn http_shutdown_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.http_shutdown_timeout = timeout;
        self
    }

    /// Register application cleanup to run after serving stops: after HTTP draining and task
    /// shutdown. Hooks run in registration order and may be registered more than once.
    ///
    /// Use this to tear down resources in a defined order (feeds, terminals, child processes,
    /// watchers). The hook receives a clone of the application context, so it can read `state`,
    /// `config`, and the [`GotchaContext::shutdown`] token. [`listen`](Self::listen),
    /// [`run`](Self::run), and [`BoundServer::serve`](crate::BoundServer::serve) return only
    /// after every hook completes. Return `()`; log inside the hook instead of returning errors.
    /// A panicking hook does not stop later hooks; the first panic is re-raised after all hooks
    /// have run. A hook that never resolves blocks shutdown; there is no cleanup timeout.
    /// Cancelling the serving future while a hook runs aborts that hook.
    ///
    /// ```rust,no_run
    /// use gotcha::Gotcha;
    /// let app = Gotcha::new()
    ///     .on_shutdown(|context| async move {
    ///         tracing::info!(port = context.config.server.port, "application stopped");
    ///     })
    ///     .on_shutdown(|_| async { /* later resources */ });
    /// ```
    pub fn on_shutdown<F, Fut>(mut self, hook: F) -> Self
    where
        F: FnOnce(GotchaContext<S, C>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.shutdown_hooks.push(Box::new(move |context| Box::pin(hook(context))));
        self
    }

    /// Set the grace period for in-flight scheduled executions (default: 30 seconds).
    /// Remaining tasks are aborted and joined at the deadline. This runs concurrently with
    /// HTTP draining; see [`Self::http_shutdown_timeout`].
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

    /// Enable the default OpenAPI JSON and Scalar endpoints.
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

    /// Enable the Prometheus metrics endpoint and record HTTP metrics for business routes.
    ///
    /// [`MetricsEndpoint::default`](crate::MetricsEndpoint::default) serves the Prometheus text
    /// format at `/metrics`; set [`MetricsEndpoint::path`](crate::MetricsEndpoint::path) to serve
    /// it elsewhere. Without this call, the `prometheus` feature only exposes the `metrics`
    /// re-exports and records nothing.
    ///
    /// The layer covers the routes registered through this builder, including [`nest`](Self::nest)
    /// and [`merge`](Self::merge), but not documentation endpoints, the metrics endpoint itself,
    /// or routes and services mounted by [`finish_router`](Self::finish_router). Middleware added
    /// with [`app_layer`](Self::app_layer) runs outside the layer. See
    /// [`GotchaApp::metrics_endpoint`](crate::GotchaApp::metrics_endpoint) for the trait API and
    /// the process-wide recorder semantics.
    ///
    /// ```rust,no_run
    /// use gotcha::{Gotcha, MetricsEndpoint};
    ///
    /// let app = Gotcha::new()
    ///     .metrics(MetricsEndpoint::default())
    ///     .get("/health", || async { "ok" });
    /// ```
    #[cfg(feature = "prometheus")]
    pub fn metrics(mut self, endpoint: crate::MetricsEndpoint) -> Self {
        self.metrics_endpoint = Some(endpoint);
        self
    }

    /// Consume the application builder and export its complete OpenAPI document without HTTP.
    ///
    /// Uses the same generation and transform pipeline as documentation endpoints. Does not
    /// load configuration, construct default state, register tasks, install finish hooks or
    /// application layers, or start a server. Registered routes and `FnOnce` transforms are
    /// consumed; endpoint settings do not affect the document. User transforms execute normally.
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
        self.bind_on(addr).await?.serve().await
    }

    /// Start using `host`/`port` overrides, then loaded server settings, then framework defaults.
    /// Bind before initializing state, assembling routes, or registering tasks. The context
    /// contains the effective bound address, including the port chosen for port 0.
    pub async fn run(self) -> GotchaResult<()> {
        self.bind().await?.serve().await
    }

    /// Bind the configured address without serving yet.
    ///
    /// The same resolution as [`Self::run`] applies: `listen`/`host`/`port` overrides, then
    /// loaded server settings, then framework defaults. State, routes, and tasks are initialized
    /// before this returns, and a bind failure starts none of them. The returned
    /// [`BoundServer`](crate::BoundServer) exposes [`local_addr`](crate::BoundServer::local_addr),
    /// which reports the assigned port when the configuration uses port 0.
    ///
    /// ```rust,no_run
    /// use gotcha::Gotcha;
    /// # async fn example() -> gotcha::GotchaResult<()> {
    /// let server = Gotcha::new().port(0).bind().await?;
    /// let address = server.local_addr();
    /// tokio::spawn(server.serve());
    /// println!("listening on http://{address}");
    /// # Ok(()) }
    /// ```
    pub async fn bind(self) -> GotchaResult<crate::BoundServer> {
        startup::bind(self, None).await
    }

    /// Bind a specific socket address without serving. See [`Self::bind`].
    pub async fn bind_on(self, addr: SocketAddr) -> GotchaResult<crate::BoundServer> {
        startup::bind(self, Some(addr)).await
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

    fn http_shutdown_timeout(&self) -> std::time::Duration {
        self.http_shutdown_timeout
    }

    async fn on_shutdown(&mut self, context: &GotchaContext<S, C>) {
        let mut first_panic = None;
        for hook in std::mem::take(&mut self.shutdown_hooks) {
            // Isolate both the closure call and its future so one panicking hook cannot skip the
            // remaining hooks. The first panic is re-raised after all hooks have run.
            let future = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hook(context.clone()))) {
                Ok(future) => future,
                Err(payload) => {
                    first_panic.get_or_insert(payload);
                    continue;
                }
            };
            match CleanupTask(Some(tokio::spawn(future))).join().await {
                Ok(()) => {}
                Err(error) if error.is_panic() => {
                    if first_panic.is_none() {
                        first_panic = Some(error.into_panic());
                    }
                }
                Err(error) => tracing::error!("cleanup hook task failed: {error}"),
            }
        }
        if let Some(payload) = first_panic {
            std::panic::resume_unwind(payload);
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
        let finish_routers = std::mem::take(&mut self.finish_routers);
        let app_layers = std::mem::take(&mut self.app_layers);
        crate::assembly::assemble(
            std::mem::take(&mut self.router),
            context,
            #[cfg(feature = "openapi")]
            self.openapi_endpoints.take(),
            #[cfg(feature = "prometheus")]
            self.metrics_endpoint.take(),
            |router, context| {
                let router = finish_routers.into_iter().fold(router, |router, finish| finish(router, context));
                // Application layers stay outermost so they cover services mounted by the hooks.
                app_layers.into_iter().fold(router, |router, layer| layer(router))
            },
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
        let mut app = Gotcha::from_context(GotchaContext::new(
            RuntimeState(calls.clone()),
            ConfigWrapper {
                app: RuntimeConfig { value: "parent".into() },
                server: ServerConfig::default(),
            },
        ))
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
        let router = app.router.into_axum_router(GotchaContext::new(EmptyState::default(), ConfigWrapper::default()));
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

    #[tokio::test]
    async fn builder_route_layers_nest_services_and_finish_hooks_compose() {
        use axum::{
            body::{to_bytes, Body},
            extract::Request,
            http::{Request as HttpRequest, StatusCode},
            middleware::{self, Next},
            response::Response,
        };
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use tower::ServiceExt;

        #[derive(Clone)]
        struct RuntimeState(Arc<AtomicUsize>);

        async fn mark_route(request: Request, next: Next) -> Response {
            let mut response = next.run(request).await;
            response.headers_mut().insert("x-route-layer", "applied".parse().unwrap());
            response
        }

        async fn mark_app(request: Request, next: Next) -> Response {
            let mut response = next.run(request).await;
            response.headers_mut().insert("x-app-layer", "applied".parse().unwrap());
            response
        }

        let calls = Arc::new(AtomicUsize::new(3));
        let mut app = Gotcha::from_context(GotchaContext::new(RuntimeState(calls.clone()), ConfigWrapper::<EmptyConfig>::default()))
            .get("/guarded", || async { "guarded" })
            .route_layer(middleware::from_fn(mark_route))
            .nest_service("/assets", tower::service_fn(|_: Request| async { Ok::<_, std::convert::Infallible>("asset") }))
            .finish_router(|router, context| {
                let calls = context.state.0.clone();
                router.nest_service(
                    "/proxy",
                    tower::service_fn(move |_: Request| {
                        let calls = calls.clone();
                        async move {
                            let current = calls.fetch_add(1, Ordering::SeqCst);
                            Ok::<_, std::convert::Infallible>(format!("proxied {current}"))
                        }
                    }),
                )
            })
            .fallback(|| async { (StatusCode::NOT_FOUND, "fallback") })
            .app_layer(middleware::from_fn(mark_app));
        let router = startup::prepare(&mut app, Some("127.0.0.1:0".parse().unwrap())).await.unwrap().router;

        let get = |path: &'static str| {
            let router = router.clone();
            async move {
                let response = router.oneshot(HttpRequest::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
                let status = response.status();
                let headers = response.headers().clone();
                let body = to_bytes(response.into_body(), 1024).await.unwrap().to_vec();
                (status, headers, body)
            }
        };

        let (status, headers, body) = get("/guarded").await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, b"guarded".as_slice()));
        assert_eq!(headers.get("x-route-layer").unwrap(), "applied");
        assert_eq!(headers.get("x-app-layer").unwrap(), "applied");

        let (status, headers, body) = get("/missing").await;
        assert_eq!((status, body.as_slice()), (StatusCode::NOT_FOUND, b"fallback".as_slice()));
        assert!(headers.get("x-route-layer").is_none(), "unmatched requests bypass route_layer");
        assert_eq!(headers.get("x-app-layer").unwrap(), "applied");

        let (status, headers, body) = get("/assets/file").await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, b"asset".as_slice()));
        assert_eq!(headers.get("x-app-layer").unwrap(), "applied", "app_layer covers nested services");

        let (status, headers, body) = get("/proxy/status").await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, b"proxied 3".as_slice()));
        assert_eq!(headers.get("x-app-layer").unwrap(), "applied", "app_layer covers finish-hook services");
        assert_eq!(calls.load(Ordering::SeqCst), 4, "the finish hook built the proxy from application state");
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

    #[cfg(feature = "openapi")]
    #[tokio::test]
    async fn builder_openapi_info_reaches_the_served_document() {
        use axum::body::{to_bytes, Body};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let info = crate::oas::Info {
            title: "Builder API".into(),
            description: Some("Served with info".into()),
            terms_of_service: None,
            contact: None,
            license: None,
            version: "9.9.9".into(),
        };
        let mut app = Gotcha::new().openapi_info(info).with_openapi().get("/hello", || async { "hello" });
        let router = startup::prepare(&mut app, Some("127.0.0.1:0".parse().unwrap())).await.unwrap().router;
        let response = router
            .oneshot(Request::builder().uri("/openapi.json").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(spec["info"]["title"], "Builder API");
        assert_eq!(spec["info"]["version"], "9.9.9");
        assert_eq!(spec["info"]["description"], "Served with info");
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

    /// `with_default_config` starts from the framework's typed server defaults, so an
    /// environment-only `APP_SERVER__PORT` coerces even without a configuration file.
    #[test]
    fn default_sources_install_typed_server_defaults() {
        const CHILD: &str = "GOTCHA_BUILDER_ENV_ONLY_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let dir = tempfile::tempdir().unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("APP_")) {
                command.env_remove(key);
            }
            let output = command
                .args(["--exact", "builder::tests::default_sources_install_typed_server_defaults", "--nocapture"])
                .current_dir(dir.path())
                .env(CHILD, "1")
                .env("APP_SERVER__PORT", "9000")
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

        // No configuration file exists in the child's working directory.
        let app = Gotcha::with_config::<EmptyConfig>().with_default_config();
        let config = app.config.resolve().expect("environment-only configuration loads");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.host, ServerConfig::default().host);
    }

    #[test]
    fn build_config_accepts_application_defaults_above_framework_defaults() {
        #[derive(Serialize)]
        struct Defaults {
            server: ServerConfig,
        }
        let app = Gotcha::with_config::<EmptyConfig>()
            .build_config(|builder| {
                builder
                    .defaults(Defaults {
                        server: ServerConfig {
                            host: "0.0.0.0".into(),
                            port: 8700,
                        },
                    })
                    .expect("defaults serialize")
            })
            .unwrap();
        let config = app.config.resolve().unwrap();
        assert_eq!(config.server.port, 8700);
        assert_eq!(config.server.host, "0.0.0.0");
    }

    /// Default files resolve environment and path variables through `with_default_config()`.
    #[test]
    fn default_sources_resolve_variables() {
        const CHILD: &str = "GOTCHA_BUILDER_DEFAULT_VARS_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir(dir.path().join("configurations")).unwrap();
            // `default_host` resolves the environment variable; `server.host` resolves the
            // path variable to that value.
            std::fs::write(
                dir.path().join("configurations/application.toml"),
                "default_host = \"${GOTCHA_DEFAULT_VARS_FILE}\"\n\n[server]\nhost = \"${default_host}\"\nport = 18080\n",
            )
            .unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("APP_")) {
                command.env_remove(key);
            }
            let output = command
                .args(["--exact", "builder::tests::default_sources_resolve_variables", "--nocapture"])
                .current_dir(dir.path())
                .env(CHILD, "1")
                .env("GOTCHA_DEFAULT_VARS_FILE", "10.0.0.1")
                .env("APP_SERVER__PORT", "9000")
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

        let resolve = |app: Gotcha<EmptyState, EmptyConfig>| app.config.resolve().unwrap().server;
        let config = resolve(Gotcha::with_config::<EmptyConfig>().with_default_config());
        assert_eq!(config.host, "10.0.0.1", "the path variable resolves to the environment-resolved file value");
        assert_eq!(config.port, 9000, "the environment still wins over the file");

        let config = resolve(Gotcha::with_config::<EmptyConfig>().with_default_files());
        assert_eq!(config.host, "10.0.0.1", "with_default_files resolves variables in the default files");
        assert_eq!(config.port, 18080, "file values win over framework defaults");
    }

    /// `with_default_env` resolves `${VAR}` in environment values.
    #[test]
    fn default_environment_source_resolves_variables() {
        const CHILD: &str = "GOTCHA_BUILDER_DEFAULT_ENV_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let dir = tempfile::tempdir().unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("APP_")) {
                command.env_remove(key);
            }
            let output = command
                .args(["--exact", "builder::tests::default_environment_source_resolves_variables", "--nocapture"])
                .current_dir(dir.path())
                .env(CHILD, "1")
                .env("GOTCHA_DEFAULT_VARS_ENV", "10.0.0.2")
                .env("APP_SERVER__HOST", "${GOTCHA_DEFAULT_VARS_ENV}")
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

        let app = Gotcha::with_config::<EmptyConfig>().with_default_env();
        let config = app.config.resolve().unwrap();
        assert_eq!(config.server.host, "10.0.0.2");
    }

    #[tokio::test]
    async fn cancelling_cleanup_join_aborts_the_running_hook() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct Dropped(std::sync::Arc<AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
        let hook = {
            let dropped = dropped.clone();
            tokio::spawn(async move {
                let _guard = Dropped(dropped);
                started.send(()).unwrap();
                std::future::pending::<()>().await;
            })
        };
        // The shutdown driver awaits `join` and can itself be cancelled, for example when the
        // serving future is dropped while a hook is still running.
        let join = tokio::spawn(async move {
            let _ = CleanupTask(Some(hook)).join().await;
        });
        starts.recv().await.unwrap();
        join.abort();
        assert!(join.await.unwrap_err().is_cancelled());
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !dropped.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelling the join must abort the running hook");
    }
}
