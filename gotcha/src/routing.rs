//! Method routers that keep handlers and their OpenAPI descriptors together.
//!
//! ```rust,no_run
//! use gotcha::{routing::{get, post}, GotchaRouter};
//!
//! async fn list() -> String { "items".into() }
//! async fn create() -> String { "created".into() }
//!
//! let router: GotchaRouter<()> = GotchaRouter::default()
//!     .route("/items", get(list).merge(post(create)));
//! ```
//!
//! Enable `openapi` and annotate handlers with `#[gotcha::api]` to include their operations
//! in the document. Unannotated handlers still work but have no generated operation.
//! Use `route_raw` on a Gotcha router or builder to attach a native Axum `MethodRouter`.

use std::convert::Infallible;

use axum::extract::Request;
use axum::handler::Handler;
pub use axum::routing::MethodFilter;
use axum::routing::{MethodRouter as AxumMethodRouter, Route};
use tower_layer::Layer;
use tower_service::Service;

#[cfg(feature = "openapi")]
use {crate::Operable, axum::http::Method, std::collections::HashMap};

/// HTTP handlers for one path, together with their deferred OpenAPI descriptors.
///
/// Compose methods before attaching a path with [`GotchaRouter::route`](crate::GotchaRouter::route).
/// Cloning, merging, and layering retain the descriptors. Schemas are collected only when the
/// complete application is assembled. Duplicate methods follow Axum's conflict rules.
///
/// A native Axum method router cannot be converted here: its erased handlers no longer expose
/// their descriptor identities. Use `route_raw` for native services and other Axum-specific APIs.
#[derive(Clone)]
pub struct MethodRouter<State = ()> {
    pub(crate) router: AxumMethodRouter<State>,
    #[cfg(feature = "openapi")]
    pub(crate) operations: HashMap<Method, &'static Operable>,
}

impl<State: Clone + Send + Sync + 'static> Default for MethodRouter<State> {
    fn default() -> Self {
        Self::new()
    }
}

impl<State: Clone + Send + Sync + 'static> MethodRouter<State> {
    /// Create an empty method router.
    pub fn new() -> Self {
        Self {
            router: AxumMethodRouter::new(),
            #[cfg(feature = "openapi")]
            operations: HashMap::new(),
        }
    }

    /// Register a handler for one or more methods, including `GET.or(POST)` filters.
    ///
    /// Documents each explicitly selected OpenAPI method when the handler has `#[api]`.
    /// Axum's implicit HEAD handling for GET is retained, but does not add a HEAD operation.
    /// CONNECT has no OpenAPI 3.0 operation field and is left undocumented.
    pub fn on<H, T>(mut self, filter: MethodFilter, handler: H) -> Self
    where
        H: Handler<T, State>,
        T: 'static,
    {
        self.router = self.router.on(filter, handler);
        #[cfg(feature = "openapi")]
        if let Some(operable) = extract_operable::<H, T, State>() {
            self.operations.extend(documented_methods(filter).map(|method| (method, operable)));
        }
        self
    }

    /// Merge disjoint methods and their descriptors. Overlapping handlers panic as in Axum.
    pub fn merge(mut self, other: Self) -> Self {
        self.router = self.router.merge(other.router);
        #[cfg(feature = "openapi")]
        self.operations.extend(other.operations);
        self
    }

    /// Apply middleware to the current handlers, preserving their documentation.
    /// Middleware does not infer changes to response schemas or security requirements.
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: crate::Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        self.router = self.router.layer(layer);
        self
    }
}

/// Create a method router for one or more methods, retaining an annotated handler's metadata.
pub fn on<H, T, State>(filter: MethodFilter, handler: H) -> MethodRouter<State>
where
    H: Handler<T, State>,
    T: 'static,
    State: Clone + Send + Sync + 'static,
{
    MethodRouter::new().on(filter, handler)
}

macro_rules! method {
    ($name:ident, $filter:ident) => {
        #[doc = concat!("Create a method router for `", stringify!($filter), "`, retaining an annotated handler's metadata.")]
        pub fn $name<H, T, State>(handler: H) -> MethodRouter<State>
        where
            H: Handler<T, State>,
            T: 'static,
            State: Clone + Send + Sync + 'static,
        {
            on(MethodFilter::$filter, handler)
        }

        impl<State: Clone + Send + Sync + 'static> MethodRouter<State> {
            #[doc = concat!("Add a `", stringify!($filter), "` handler and its OpenAPI descriptor, when annotated.")]
            pub fn $name<H, T>(self, handler: H) -> Self
            where
                H: Handler<T, State>,
                T: 'static,
            {
                self.on(MethodFilter::$filter, handler)
            }
        }
    };
}

method!(get, GET);
method!(post, POST);
method!(put, PUT);
method!(patch, PATCH);
method!(delete, DELETE);
method!(head, HEAD);
method!(options, OPTIONS);
method!(trace, TRACE);

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

#[cfg(feature = "openapi")]
fn documented_methods(filter: MethodFilter) -> impl Iterator<Item = Method> {
    [
        (MethodFilter::GET, Method::GET),
        (MethodFilter::POST, Method::POST),
        (MethodFilter::PUT, Method::PUT),
        (MethodFilter::PATCH, Method::PATCH),
        (MethodFilter::DELETE, Method::DELETE),
        (MethodFilter::HEAD, Method::HEAD),
        (MethodFilter::OPTIONS, Method::OPTIONS),
        (MethodFilter::TRACE, Method::TRACE),
    ]
    .into_iter()
    // Axum keeps contains() private; OR leaves the filter unchanged exactly for its members.
    .filter_map(move |(candidate, method)| (filter.or(candidate) == filter).then_some(method))
}

#[cfg(all(test, feature = "openapi"))]
mod tests {
    use super::*;

    #[test]
    fn method_filters_expand_to_explicit_openapi_methods() {
        for method in [
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::HEAD,
            Method::OPTIONS,
            Method::TRACE,
        ] {
            assert_eq!(
                documented_methods(MethodFilter::try_from(method.clone()).unwrap()).collect::<Vec<_>>(),
                [method]
            );
        }
        assert_eq!(
            documented_methods(MethodFilter::GET.or(MethodFilter::POST).or(MethodFilter::HEAD)).collect::<Vec<_>>(),
            [Method::GET, Method::POST, Method::HEAD]
        );
        assert_eq!(documented_methods(MethodFilter::CONNECT).count(), 0);
        assert_eq!(
            documented_methods(MethodFilter::CONNECT.or(MethodFilter::PATCH)).collect::<Vec<_>>(),
            [Method::PATCH]
        );
    }
}
