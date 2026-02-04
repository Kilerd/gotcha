use std::convert::Infallible;

use axum::extract::Request;
use axum::handler::Handler;
pub use axum::response::IntoResponse as Responder;
use axum::routing::{MethodFilter, MethodRouter, Route};
use axum::Router;
#[cfg(feature = "openapi")]
use oas::Operation;
use tower_layer::Layer;
use tower_service::Service;

#[cfg(feature = "openapi")]
use http::Method;

#[cfg(feature = "openapi")]
use crate::Operable;

#[cfg(feature = "openapi")]
use std::collections::HashMap;

macro_rules! implement_method {
    ($method:expr, $fn_name: tt ) => {
        pub fn $fn_name<H: Handler<T, State>, T: 'static>(self, path: &str, handler: H) -> Self {
            self.method_route(path, $method, handler)
        }
    };
}

/// # GotchaRouter
///
/// A router for Gotcha web applications.
pub struct GotchaRouter<State = ()> {
    #[cfg(feature = "openapi")]
    /// The operations for the router.
    pub(crate) operations: std::collections::HashMap<(String, Method), Operation>,
    pub(crate) router: Router<State>,
}
impl<State: Clone + Send + Sync + 'static> Default for GotchaRouter<State> {
    fn default() -> Self {
        Self {
            #[cfg(feature = "openapi")]
            operations: Default::default(),
            router: Router::new(),
        }
    }
}

impl<State: Clone + Send + Sync + 'static> GotchaRouter<State> {
    /// add a route to the router
    /// # Examples
    ///
    /// ```rust,no_run
    /// use gotcha::{GotchaRouter, Responder};
    ///
    /// async fn hello_world() -> impl Responder {
    ///     "Hello World!"
    /// }
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default()
    ///     .route("/", axum::routing::get(hello_world));
    /// ```
    pub fn route(self, path: &str, method_router: MethodRouter<State>) -> Self {
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations,
            router: self.router.route(path, method_router),
        }
    }

    /// add a method route to the router
    /// # Examples
    ///
    /// ```rust,no_run
    /// use gotcha::{GotchaRouter, Responder};
    ///
    /// async fn hello_world() -> impl Responder {
    ///     "Hello World!"
    /// }
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default()
    ///     .method_route("/", MethodFilter::GET, hello_world);
    /// ```
    pub fn method_route<H, T>(self, path: &str, method: MethodFilter, handler: H) -> Self
    where
        H: Handler<T, State>,
        T: 'static,
    {
        #[cfg(feature = "openapi")]
        let handle_operable = extract_operable::<H, T, State>();
        #[cfg(feature = "openapi")]
        if let Some(operable) = handle_operable {
            tracing::info!("generating openapi spec for {}[{}]", &operable.type_name, &path);
            let operation = operable.generate(path.to_string());
            let method = match method {
                MethodFilter::DELETE => Method::DELETE,
                MethodFilter::GET => Method::GET,
                MethodFilter::HEAD => Method::HEAD,
                MethodFilter::OPTIONS => Method::OPTIONS,
                MethodFilter::PATCH => Method::PATCH,
                MethodFilter::POST => Method::POST,
                MethodFilter::PUT => Method::PUT,
                MethodFilter::TRACE => Method::TRACE,
                _ => todo!(),
            };
            self.operations.insert((path.to_string(), method), operation);
        }

        let router = MethodRouter::new().on(method, handler);

        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations,
            router: self.router.route(path, router),
        }
    }

    implement_method!(MethodFilter::GET, get);
    implement_method!(MethodFilter::POST, post);
    implement_method!(MethodFilter::PUT, put);
    implement_method!(MethodFilter::PATCH, patch);
    implement_method!(MethodFilter::HEAD, head);
    implement_method!(MethodFilter::DELETE, delete);
    implement_method!(MethodFilter::OPTIONS, options);
    implement_method!(MethodFilter::TRACE, trace);

    /// nest a router inside another router
    /// # Examples
    ///
    /// ```rust,no_run
    /// use gotcha::{GotchaRouter, Responder};
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default()
    ///     .nest("/users", GotchaRouter::default());
    /// ```
    pub fn nest(self, path: &str, router: Self) -> Self {
        #[cfg(feature = "openapi")]
        let operations = router
            .operations
            .into_iter()
            .map(|(key, value)| {
                let (path_str, method) = key;
                let new_path = format!("{}/{}", path, path_str);
                ((new_path, method), value)
            })
            .collect::<HashMap<(String, Method), Operation>>();
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations.into_iter().chain(operations).collect(),
            router: self.router.nest(path, router.router),
        }
    }

    /// merge two routers
    /// # Examples
    ///
    /// ```rust,no_run
    /// use gotcha::{GotchaRouter};
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default()
    ///     .merge(GotchaRouter::default());
    /// ```
    pub fn merge(self, other: Self) -> Self {
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations.into_iter().chain(other.operations).collect(),
            router: self.router.merge(other.router),
        }
    }

    /// add a layer to the router
    /// # Examples
    ///
    /// ```rust,no_run
    /// use gotcha::{GotchaRouter};
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default()
    ///     .layer(MyLayer::default());
    /// ```
    pub fn layer<L>(self, layer: L) -> Self
    where
        L: Layer<Route> + Clone + Send + 'static,
        L::Service: Service<Request> + Clone + Send + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations,
            router: self.router.layer(layer),
        }
    }

    pub fn fallback<H, T>(self, handler: H) -> Self
    where
        H: Handler<T, State>,
        T: 'static,
    {
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations,
            router: self.router.fallback(handler),
        }
    }
}

#[cfg(feature = "cloudflare_worker")]
impl GotchaRouter<()> {
    pub async fn call(&self, request: crate::worker::Request) -> Result<worker::Response, Box<dyn std::error::Error>> {
        Ok(worker::Response::ok("Hello, World!").unwrap())
    }
}

#[doc(hidden)]
#[cfg(feature = "openapi")]
pub fn extract_operable<H, T, State>() -> Option<&'static Operable>
where
    H: Handler<T, State>,
    T: 'static,
{
    let handle_name = std::any::type_name::<H>();
    inventory::iter::<Operable>.into_iter().find(|it| it.type_name.eq(handle_name))
}
