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
use crate::{openapi::transforms::OpenApiTransforms, Operable};

#[cfg(feature = "openapi")]
use std::collections::HashMap;

/// Canonicalize a nested documentation path using Axum's `path_for_nested_route` rules.
/// Only the join boundary is normalized: a prefix's trailing slash is significant for a
/// child root, and intentional empty path segments elsewhere must remain intact.
#[cfg(feature = "openapi")]
fn canonicalize_nested_path(prefix: &str, child_path: &str) -> String {
    if prefix.ends_with('/') {
        format!("{prefix}{}", child_path.trim_start_matches('/'))
    } else if child_path == "/" {
        prefix.to_owned()
    } else {
        format!("{prefix}{child_path}")
    }
}

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
    /// Child transforms in composition order, followed by this router's own transforms.
    #[cfg(feature = "openapi")]
    openapi_transforms: OpenApiTransforms,
    pub(crate) router: Router<State>,
}
impl<State: Clone + Send + Sync + 'static> Default for GotchaRouter<State> {
    fn default() -> Self {
        Self {
            #[cfg(feature = "openapi")]
            operations: Default::default(),
            #[cfg(feature = "openapi")]
            openapi_transforms: Default::default(),
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
            openapi_transforms: self.openapi_transforms,
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
            openapi_transforms: self.openapi_transforms,
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

    /// Nest a router inside another router, using Axum's path-joining rules for documentation.
    /// A child root `/` becomes `/api` under `/api`, but `/api/` under `/api/`.
    /// With `openapi`, the child's transforms are retained and run before this router's
    /// own transforms, on the final complete document.
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
            .map(|((child_path, method), operable)| ((canonicalize_nested_path(path, &child_path), method), operable))
            .collect::<HashMap<(String, Method), &'static Operable>>();
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations.into_iter().chain(operations).collect(),
            #[cfg(feature = "openapi")]
            openapi_transforms: self.openapi_transforms.with_child(router.openapi_transforms),
            router: self.router.nest(path, router.router),
        }
    }

    /// Merge two routers.
    /// With `openapi`, `other` is a child for transform ordering: its transforms run
    /// before this router's own transforms, on the final complete document.
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
            openapi_transforms: self.openapi_transforms.with_child(other.openapi_transforms),
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
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations,
            #[cfg(feature = "openapi")]
            openapi_transforms: self.openapi_transforms,
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
            openapi_transforms: self.openapi_transforms,
            router: self.router.fallback(handler),
        }
    }

    /// Handle unmatched requests with a `Service` rather than a handler.
    ///
    /// Useful for delegating to something that is already a tower service — serving a
    /// single-page application's `index.html` with `ServeFile`, say, or forwarding to a
    /// proxy — where [`fallback`](Self::fallback) would need a wrapper handler.
    ///
    /// ```rust,ignore
    /// use gotcha::GotchaRouter;
    /// use gotcha::axum::{body::Body, extract::Request, response::Response};
    ///
    /// let router: GotchaRouter<()> = GotchaRouter::default()
    ///     .fallback_service(tower::service_fn(|_: Request| async {
    ///         Ok::<_, std::convert::Infallible>(Response::new(Body::from("not found")))
    ///     }));
    /// ```
    pub fn fallback_service<Svc, ResBody>(self, service: Svc) -> Self
    where
        Svc: Service<Request, Response = axum::http::Response<ResBody>, Error = Infallible> + Clone + Send + Sync + 'static,
        Svc::Future: Send + 'static,
        ResBody: axum::body::HttpBody<Data = axum::body::Bytes> + Send + 'static,
        ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Self {
            #[cfg(feature = "openapi")]
            operations: self.operations,
            #[cfg(feature = "openapi")]
            openapi_transforms: self.openapi_transforms,
            router: self.router.fallback_service(service),
        }
    }

    /// Customize the generated OpenAPI spec before it is served at `/openapi.json`.
    ///
    /// The transform receives the fully-generated [`oas::OpenAPIV3`] (with every route's
    /// operation already filled in) and returns the spec to serve, so you can set the
    /// title/version, add servers, security schemes, components, and so on.
    ///
    /// Repeated calls append transforms in registration order. At assembly, child subtrees
    /// run in `nest`/`merge` insertion order, then this router's own transforms run, even if
    /// registered before its children. Each callback runs exactly once, never per request.
    /// `merge` treats its argument as a child, so grouping routers can change precedence.
    ///
    /// Every callback receives the complete document, including prefixed paths and collected
    /// components. `info`, `components`, and top-level `security` are global, including when
    /// set by a child. For route-local security, edit the relevant operation's `security`
    /// using its final path. Later writes win; maps and lists are not implicitly merged.
    /// Edit their entries to preserve unrelated values instead of replacing the whole field.
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
        self.openapi_transforms.push(Box::new(transform));
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
                let openapi_spec = self.openapi_transforms.apply(crate::openapi::generate_openapi(self.operations));
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
    fn canonicalize_nested_path_handles_join_boundaries() {
        for (prefix, child, expected) in [
            ("/api", "/hello", "/api/hello"),
            ("/api/", "/hello", "/api/hello"),
            ("/api", "/", "/api"),
            ("/api/", "/", "/api/"),
            ("/api", "/hello/", "/api/hello/"),
            ("/api/", "/hello/", "/api/hello/"),
        ] {
            assert_eq!(canonicalize_nested_path(prefix, child), expected, "{prefix} + {child}");
        }
    }

    #[test]
    fn canonicalize_nested_path_preserves_significant_empty_segments() {
        for (prefix, child, expected) in [
            ("/api", "/v1//hello", "/api/v1//hello"),
            ("/api//", "/hello", "/api//hello"),
            ("/api", "//hello", "/api//hello"),
            ("/api/", "//hello", "/api/hello"),
        ] {
            assert_eq!(canonicalize_nested_path(prefix, child), expected, "{prefix} + {child}");
        }
    }

    #[test]
    fn canonicalize_nested_path_composes_across_nesting_levels() {
        let items = canonicalize_nested_path("/v1/", "/items/{id}");
        assert_eq!(canonicalize_nested_path("/tenants/{tenant}", &items), "/tenants/{tenant}/v1/items/{id}");

        let root = canonicalize_nested_path("/v1", "/");
        assert_eq!(canonicalize_nested_path("/api/", &root), "/api/v1");

        let root_with_slash = canonicalize_nested_path("/v1/", "/");
        assert_eq!(canonicalize_nested_path("/api", &root_with_slash), "/api/v1/");
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
