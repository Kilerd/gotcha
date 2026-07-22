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

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;

use axum::extract::Request;
use axum::handler::Handler;
use axum::routing::MethodRouter;
use serde::{Deserialize, Serialize};
use tower_layer::Layer;
use tower_service::Service;

use crate::config::{BasicConfig, Config, ConfigBuilder, ConfigState, ConfigWrapper, GotchaConfigLoader};
use crate::error::{GotchaError, GotchaResult};
use crate::router::{GotchaRouter, Responder};
use crate::GotchaContext;

/// A one-shot closure that registers background tasks on the scheduler when the
/// server starts.
#[cfg(feature = "task")]
type TaskRegistrar<S, C> = Box<dyn FnOnce(&mut crate::TaskScheduler<S, C>) + Send>;

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
    C: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
{
    router: GotchaRouter<GotchaContext<S, C>>,
    host: String,
    port: u16,
    state: Option<S>,
    config: Option<ConfigWrapper<C>>,
    config_builder: Option<ConfigState>,
    #[cfg(feature = "task")]
    tasks: Vec<TaskRegistrar<S, C>>,
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
        Self {
            router: GotchaRouter::default(),
            host: "127.0.0.1".to_string(),
            port: 3000,
            state: None,
            config: None,
            config_builder: None,
            #[cfg(feature = "task")]
            tasks: Vec::new(),
        }
    }
}

impl Gotcha {
    /// Create a new Gotcha builder with custom state and config types
    ///
    /// This is a convenience method that allows you to specify custom types
    /// without needing to provide dummy type parameters.
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
        C: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
    {
        Gotcha {
            router: GotchaRouter::default(),
            host: "127.0.0.1".to_string(),
            port: 3000,
            state: None,
            config: None,
            config_builder: None,
            #[cfg(feature = "task")]
            tasks: Vec::new(),
        }
    }

    /// Create a Gotcha builder with custom state type and default config
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
        Gotcha {
            router: GotchaRouter::default(),
            host: "127.0.0.1".to_string(),
            port: 3000,
            state: None,
            config: None,
            config_builder: None,
            #[cfg(feature = "task")]
            tasks: Vec::new(),
        }
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
        C: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
    {
        Gotcha {
            router: GotchaRouter::default(),
            host: "127.0.0.1".to_string(),
            port: 3000,
            state: None,
            config: None,
            config_builder: None,
            #[cfg(feature = "task")]
            tasks: Vec::new(),
        }
    }
}

impl<S, C> Gotcha<S, C>
where
    S: Clone + Send + Sync + 'static + Default,
    C: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
{
    /// Set the application state
    pub fn state(mut self, state: S) -> Self {
        self.state = Some(state);
        self
    }

    /// Set the application configuration
    pub fn config(mut self, config: ConfigWrapper<C>) -> Self {
        self.config = Some(config);
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
    {
        let builder = Config::builder();
        let configured_builder = builder_fn(builder);
        let config: ConfigWrapper<C> = configured_builder.build()?;
        self.config = Some(config);
        self.config_builder = None; // Clear any accumulated builder
        Ok(self)
    }

    /// Add default configuration sources (files + environment variables)
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Equivalent to calling `.with_default_files().with_default_env()`
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
        let mut state = self.config_builder.take().unwrap_or_else(|| ConfigState {
            file_paths: Vec::new(),
            env_prefixes: Vec::new(),
            enable_vars: true,
        });

        state.env_prefixes.push(prefix.as_ref().to_string());
        self.config_builder = Some(state);
        self
    }

    /// Add a required configuration file source
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Multiple calls will add multiple file sources.
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
        let mut state = self.config_builder.take().unwrap_or_else(|| ConfigState {
            file_paths: Vec::new(),
            env_prefixes: Vec::new(),
            enable_vars: true,
        });

        state.file_paths.push(path.as_ref().to_path_buf());
        self.config_builder = Some(state);
        self
    }

    /// Add an optional configuration file source (won't fail if file doesn't exist)
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    /// Multiple calls will add multiple optional file sources.
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
        let mut state = self.config_builder.take().unwrap_or_else(|| ConfigState {
            file_paths: Vec::new(),
            env_prefixes: Vec::new(),
            enable_vars: true,
        });

        state.file_paths.push(path.as_ref().to_path_buf());
        self.config_builder = Some(state);
        self
    }

    /// Add default configuration files (configurations/application.toml and profile-specific files)
    ///
    /// This adds to any existing configuration sources rather than replacing them.
    ///
    /// # Example
    /// ```no_run
    /// use gotcha::prelude::*;
    ///
    /// let app = Gotcha::new()
    ///     .with_default_files();
    /// ```
    pub fn with_default_files(mut self) -> Self {
        let mut state = self.config_builder.take().unwrap_or_default();

        // Add default file paths
        state.file_paths.push("configurations/application.toml".into());

        // Add profile-specific file if profile is set
        if let Ok(profile) = std::env::var("GOTCHA_ACTIVE_PROFILE") {
            let profile_path = format!("configurations/application_{}.toml", profile);
            state.file_paths.push(profile_path.into());
        }

        self.config_builder = Some(state);
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
        let mut state = self.config_builder.take().unwrap_or_default();

        state.env_prefixes.push("APP".to_string());
        self.config_builder = Some(state);
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
        let mut state = self.config_builder.take().unwrap_or_default();

        state.enable_vars = true;
        self.config_builder = Some(state);
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
        self.host = host.into();
        self
    }

    /// Set the port number
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
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
    ///     .get("/users/:id", get_user);
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

    /// Add a route with custom method
    pub fn route(mut self, path: &str, method_router: MethodRouter<GotchaContext<S, C>>) -> Self {
        self.router = self.router.route(path, method_router);
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

    /// Nest a sub-application at a path
    pub fn nest(mut self, path: &str, other: Self) -> Self {
        self.router = self.router.nest(path, other.router);
        self
    }

    /// Merge with another Gotcha application
    pub fn merge(mut self, other: Self) -> Self {
        self.router = self.router.merge(other.router);
        self
    }

    /// Add a layer to the application
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: Layer<axum::routing::Route> + Clone + Send + 'static,
        L::Service: Service<Request> + Clone + Send + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<std::convert::Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        self.router = self.router.layer(layer);
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

    /// Register background tasks (requires the `task` feature).
    ///
    /// The closure receives a [`TaskScheduler`](crate::TaskScheduler) when the
    /// server starts, on which you can register `cron` / `interval` jobs. This
    /// brings the builder to parity with `GotchaApp::tasks`.
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

    /// Add CORS support (requires "cors" feature)
    #[cfg(feature = "cors")]
    pub fn with_cors(self) -> Self {
        use crate::layers::CorsLayer;
        self.layer(CorsLayer::permissive())
    }

    /// Add OpenAPI support (requires "openapi" feature)  
    #[cfg(feature = "openapi")]
    pub fn with_openapi(self) -> Self {
        // OpenAPI routes are automatically added when the feature is enabled
        self
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
        tracing::info!("🚀 Starting Gotcha server on {}", addr);

        let context = self.build_context().await?;

        #[cfg(feature = "task")]
        {
            let tasks = self.tasks;
            if !tasks.is_empty() {
                let mut scheduler = crate::TaskScheduler::new(context.clone());
                for register in tasks {
                    register(&mut scheduler);
                }
            }
        }

        let app_router = self.router.into_axum_router(context);

        let listener = tokio::net::TcpListener::bind(addr).await.map_err(|source| GotchaError::Bind {
            addr: addr.to_string(),
            source,
        })?;
        tracing::info!("✅ Server listening on http://{}", addr);

        axum::serve(listener, app_router).await.map_err(GotchaError::Io)?;
        Ok(())
    }

    /// Start the server using the configured host and port
    pub async fn run(self) -> GotchaResult<()> {
        let ip = Ipv4Addr::from_str(&self.host).map_err(|_| GotchaError::InvalidAddress(self.host.clone()))?;
        let addr = SocketAddrV4::new(ip, self.port);
        self.listen_on(SocketAddr::V4(addr)).await
    }

    /// Build the application context (loads configuration and resolves state).
    ///
    /// On configuration failure this logs a warning and falls back to defaults,
    /// keeping the builder lenient. Use the trait API for strict config loading.
    async fn build_context(&self) -> GotchaResult<GotchaContext<S, C>> {
        let config = match (&self.config, &self.config_builder) {
            // Explicit config wins
            (Some(config), _) => config.clone(),
            // Accumulated configuration sources
            (None, Some(state)) => {
                let builder = ConfigBuilder::from_state(state.clone());
                match builder.build::<ConfigWrapper<C>>() {
                    Ok(config) => {
                        tracing::info!("Configuration loaded successfully from accumulated sources");
                        config
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load accumulated configuration: {e}, using defaults");
                        ConfigWrapper {
                            basic: BasicConfig {
                                host: self.host.clone(),
                                port: self.port,
                            },
                            application: C::default(),
                        }
                    }
                }
            }
            // Default loading, falling back to defaults on failure
            (None, None) => match GotchaConfigLoader::load::<ConfigWrapper<C>>(std::env::var("GOTCHA_ACTIVE_PROFILE").ok()) {
                Ok(config) => config,
                Err(e) => {
                    tracing::warn!("Failed to load configuration: {e}, using defaults");
                    ConfigWrapper {
                        basic: BasicConfig {
                            host: self.host.clone(),
                            port: self.port,
                        },
                        application: C::default(),
                    }
                }
            },
        };

        let state = match &self.state {
            Some(state) => state.clone(),
            None => S::default(),
        };

        Ok(GotchaContext { config, state })
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
