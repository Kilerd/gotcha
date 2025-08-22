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
use axum::Router;
use serde::{Deserialize, Serialize};
use tower_layer::Layer;
use tower_service::Service;

use crate::config::{BasicConfig, ConfigWrapper, GotchaConfigLoader};
use crate::router::{GotchaRouter, Responder};
use crate::GotchaContext;

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
        }
    }
}

impl<S, C> Gotcha<S, C>
where
    S: Clone + Send + Sync + 'static + Default,
    C: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
{
    /// Create a new Gotcha builder with custom state and config types
    pub fn with_types<NS, NC>() -> Gotcha<NS, NC>
    where
        NS: Clone + Send + Sync + 'static + Default,
        NC: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
    {
        Gotcha {
            router: GotchaRouter::default(),
            host: "127.0.0.1".to_string(),
            port: 3000,
            state: None,
            config: None,
        }
    }

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
    pub async fn listen<A>(self, addr: A) -> Result<(), Box<dyn std::error::Error>>
    where
        A: AsRef<str>,
    {
        let addr_str = addr.as_ref();
        let socket_addr: SocketAddr = addr_str.parse()
            .map_err(|_| format!("Invalid address format: {}", addr_str))?;
        
        self.listen_on(socket_addr).await
    }

    /// Start the server on a specific socket address
    pub async fn listen_on(self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("🚀 Starting Gotcha server on {}", addr);
        
        let context = self.build_context().await?;
        let app_router = self.build_app_router(context).await?;
        
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("✅ Server listening on http://{}", addr);
        
        axum::serve(listener, app_router).await?;
        Ok(())
    }

    /// Start the server using the configured host and port
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let host = self.host.clone();
        let port = self.port;
        let addr = SocketAddrV4::new(
            Ipv4Addr::from_str(&host)?,
            port
        );
        self.listen_on(SocketAddr::V4(addr)).await
    }

    /// Build the application context
    async fn build_context(&self) -> Result<GotchaContext<S, C>, Box<dyn std::error::Error>> {
        // Load or create configuration
        let config = match &self.config {
            Some(config) => config.clone(),
            None => {
                // Try to load from files, fall back to default
                match std::panic::catch_unwind(|| {
                    GotchaConfigLoader::load::<ConfigWrapper<C>>(
                        std::env::var("GOTCHA_ACTIVE_PROFILE").ok()
                    )
                }) {
                    Ok(config) => config,
                    Err(_) => {
                        // If loading fails, use default
                        tracing::warn!("Failed to load configuration, using defaults");
                        ConfigWrapper {
                            #[cfg(not(feature = "cloudflare_worker"))]
                            basic: BasicConfig {
                                host: self.host.clone(),
                                port: self.port,
                            },
                            application: C::default(),
                        }
                    }
                }
            }
        };

        // Create or use provided state
        let state = match &self.state {
            Some(state) => state.clone(),
            None => S::default(),
        };

        Ok(GotchaContext { config, state })
    }

    /// Build the final Axum router
    async fn build_app_router(
        self, 
        context: GotchaContext<S, C>
    ) -> Result<Router, Box<dyn std::error::Error>> {
        let GotchaRouter {
            #[cfg(feature = "openapi")]
            operations,
            router: raw_router,
        } = self.router;

        #[cfg(feature = "openapi")]
        let openapi_spec = crate::openapi::generate_openapi(operations);

        cfg_if::cfg_if! {
            if #[cfg(feature = "openapi")] {
                use axum::Json;
                let router = raw_router
                    .with_state(context.clone())
                    .route("/openapi.json", axum::routing::get(move || async move { 
                        Json(openapi_spec.clone()) 
                    }))
                    .route("/redoc", axum::routing::get(crate::openapi::openapi_html))
                    .route("/scalar", axum::routing::get(crate::openapi::scalar_html));
            } else {
                let router = raw_router.with_state(context.clone());
            }
        }

        Ok(router)
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
    ///         .get("/", || async { "Hello World" })
    ///         .await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn quick_start() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_subscriber::fmt::init();
        Ok(Self::new())
    }
}