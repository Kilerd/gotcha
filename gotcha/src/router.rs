use std::convert::Infallible;

use axum::extract::Request;
use axum::handler::Handler;
pub use axum::response::IntoResponse as Responder;
use axum::routing::{MethodFilter, MethodRouter, Route};
use axum::Router;
use tower_layer::Layer;
use tower_service::Service;

#[cfg(feature = "openapi")]
use axum::http::Method;

#[cfg(feature = "openapi")]
use crate::Operable;

#[cfg(feature = "openapi")]
use std::collections::HashMap;

/// Generates the per-HTTP-method shorthand (`get`, `post`, …) on the router.
macro_rules! implement_method {
    ($method:expr, $fn_name: tt ) => {
        #[doc = concat!("Route `", stringify!($fn_name), "` requests for `path` to `handler`.")]
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
    /// The operations for the router, kept as their `Operable` descriptors: the `Operation` is
    /// only built during `into_axum_router`, so every route's schemas are generated inside a
    /// single collection scope and can share `components/schemas`.
    pub(crate) operations: std::collections::HashMap<(String, Method), &'static Operable>,
    /// Optional transform applied to the generated OpenAPI spec before it is served,
    /// set via [`GotchaRouter::openapi`]. Lets apps customize `info`, `servers`,
    /// `security`, `components`, etc.
    #[cfg(feature = "openapi")]
    pub(crate) openapi_transform: Option<Box<dyn FnOnce(oas::OpenAPIV3) -> oas::OpenAPIV3 + Send>>,
    pub(crate) router: Router<State>,
}
impl<State: Clone + Send + Sync + 'static> Default for GotchaRouter<State> {
    fn default() -> Self {
        Self {
            #[cfg(feature = "openapi")]
            operations: Default::default(),
            #[cfg(feature = "openapi")]
            openapi_transform: None,
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
            #[cfg(feature = "openapi")]
            openapi_transform: self.openapi_transform,
            router: self.router.route(path, method_router),
        }
    }

    /// add a method route to the router
    /// # Examples
    ///
    /// ```rust,no_run
    /// use gotcha::GotchaRouter;
    /// use gotcha::axum::routing::MethodFilter;
    /// # use gotcha::Responder;
    ///
    /// async fn hello_world() -> impl Responder {
    ///     "Hello World!"
    /// }
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default()
    ///     .method_route("/", MethodFilter::GET, hello_world);
    /// ```
    #[allow(unused_mut)]
    pub fn method_route<H, T>(mut self, path: &str, method: MethodFilter, handler: H) -> Self
    where
        H: Handler<T, State>,
        T: 'static,
    {
        #[cfg(feature = "openapi")]
        let handle_operable = extract_operable::<H, T, State>();
        #[cfg(feature = "openapi")]
        if let Some(operable) = handle_operable {
            tracing::info!("generating openapi spec for {}[{}]", &operable.type_name, &path);
            let documented_method = match method {
                MethodFilter::DELETE => Some(Method::DELETE),
                MethodFilter::GET => Some(Method::GET),
                MethodFilter::HEAD => Some(Method::HEAD),
                MethodFilter::OPTIONS => Some(Method::OPTIONS),
                MethodFilter::PATCH => Some(Method::PATCH),
                MethodFilter::POST => Some(Method::POST),
                MethodFilter::PUT => Some(Method::PUT),
                MethodFilter::TRACE => Some(Method::TRACE),
                // `MethodFilter` is `#[non_exhaustive]`. A method axum adds later should leave the
                // route working and merely undocumented, rather than bringing the application down
                // while it registers its routes (this used to be a `todo!()`).
                _ => None,
            };
            match documented_method {
                Some(method) => {
                    self.operations.insert((path.to_string(), method), operable);
                }
                None => tracing::warn!("unrecognised method filter for {path}; the route works but is left out of the OpenAPI spec"),
            }
        }

        let router = MethodRouter::new().on(method, handler);

        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations,
            #[cfg(feature = "openapi")]
            openapi_transform: self.openapi_transform,
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
            .collect::<HashMap<(String, Method), &'static Operable>>();
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations.into_iter().chain(operations).collect(),
            #[cfg(feature = "openapi")]
            openapi_transform: self.openapi_transform,
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
            #[cfg(feature = "openapi")]
            openapi_transform: self.openapi_transform,
            router: self.router.merge(other.router),
        }
    }

    /// add a layer to the router
    /// # Examples
    ///
    /// ```rust,no_run
    /// use gotcha::GotchaRouter;
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default()
    ///     .layer(gotcha::axum::Extension(0u32));
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
            #[cfg(feature = "openapi")]
            openapi_transform: self.openapi_transform,
            router: self.router.layer(layer),
        }
    }

    /// Handle requests that match no route.
    pub fn fallback<H, T>(self, handler: H) -> Self
    where
        H: Handler<T, State>,
        T: 'static,
    {
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations,
            #[cfg(feature = "openapi")]
            openapi_transform: self.openapi_transform,
            router: self.router.fallback(handler),
        }
    }

    /// Customize the generated OpenAPI spec before it is served at `/openapi.json`.
    ///
    /// The transform receives the fully-generated [`oas::OpenAPIV3`] (with every route's
    /// operation already filled in) and returns the spec to serve, so you can set the
    /// title/version, add servers, security schemes, components, and so on.
    ///
    /// ```rust,no_run
    /// use gotcha::GotchaRouter;
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default().openapi(|mut spec| {
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
        self.openapi_transform = Some(Box::new(transform));
        self
    }

    /// Finalize this router into a plain `axum::Router`, injecting `state`.
    ///
    /// When the `openapi` feature is enabled, this also mounts the generated
    /// spec at `/openapi.json` and the Redoc / Scalar UIs at `/redoc` and
    /// `/scalar`. This is the single assembly path shared by both the
    /// [`GotchaApp`](crate::GotchaApp) trait and the [`Gotcha`](crate::Gotcha)
    /// builder.
    pub(crate) fn into_axum_router(self, state: State) -> Router {
        cfg_if::cfg_if! {
            if #[cfg(feature = "openapi")] {
                let mut openapi_spec = crate::openapi::generate_openapi(self.operations);

                if let Some(transform) = self.openapi_transform {
                    openapi_spec = transform(openapi_spec);
                }
                self.router
                    .with_state(state)
                    .route("/openapi.json", axum::routing::get(move || async move { axum::Json(openapi_spec.clone()) }))
                    .route("/redoc", axum::routing::get(crate::openapi::openapi_html))
                    .route("/scalar", axum::routing::get(crate::openapi::scalar_html))
            } else {
                self.router.with_state(state)
            }
        }
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

#[cfg(all(test, feature = "openapi"))]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn openapi_transform_runs_during_assembly() {
        // Capture the title the transform sees, to prove `.openapi(..)` is stored and applied
        // when the router is finalized (the transformed spec is what gets served).
        let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let sink = captured.clone();

        let router: GotchaRouter<()> = GotchaRouter::default().openapi(move |mut spec| {
            spec.info.title = "Custom API".to_string();
            spec.info.version = "9.9.9".to_string();
            *sink.lock().unwrap() = Some(spec.info.title.clone());
            spec
        });
        let _ = router.into_axum_router(());

        assert_eq!(captured.lock().unwrap().as_deref(), Some("Custom API"));
    }

    #[test]
    fn openapi_transform_survives_chained_builder_calls() {
        // `.openapi(..)` set before other methods must not be dropped by the `Self { .. }`
        // reconstructions in `route`/`layer`/etc.
        let ran: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let flag = ran.clone();

        let router: GotchaRouter<()> = GotchaRouter::default()
            .openapi(move |spec| {
                *flag.lock().unwrap() = true;
                spec
            })
            .route("/health", axum::routing::get(|| async { "ok" }))
            .fallback(|| async { "not found" });
        let _ = router.into_axum_router(());

        assert!(*ran.lock().unwrap(), "transform set before route()/fallback() must still apply");
    }
}
