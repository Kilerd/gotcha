use std::collections::HashMap;
use std::convert::Infallible;

use axum::extract::Request;
use axum::handler::Handler;
pub use axum::response::IntoResponse as Responder;
use axum::routing::{MethodFilter, MethodRouter, Route};
use axum::Router;
use http::Method;
use oas::Operation;
use tower_layer::Layer;
use tower_service::Service;
use tracing::info;

use crate::Operable;

macro_rules! implement_method {
    ($method:expr, $fn_name: tt ) => {
        pub fn $fn_name<H: Handler<T, State>, T: 'static>(self, path: &str, handler: H) -> Self {
            self.method_route(path, $method, handler)
        }
    };
}

pub struct GotchaRouter<State = ()> {
    pub(crate) operations: HashMap<(String, Method), Operation>,
    pub(crate) router: Router<State>,
}

impl<State: Clone + Send + Sync + 'static> GotchaRouter<State> {
    pub fn new() -> Self {
        Self {
            operations: Default::default(),
            router: Router::new(),
        }
    }
    pub fn route(self, path: &str, method_router: MethodRouter<State>) -> Self {
        Self {
            operations: self.operations,
            router: self.router.route(path, method_router),
        }
    }

    pub fn method_route<H, T>(mut self, path: &str, method: MethodFilter, handler: H) -> Self
    where
        H: Handler<T, State>,
        T: 'static,
    {
        let handle_operable = extract_operable::<H, T, State>();
        if let Some(operable) = handle_operable {
            info!("generating openapi spec for {}[{}]", &operable.type_name, &path);
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

    pub fn nest(self, path: &str, router: Self) -> Self {
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
            operations: self.operations.into_iter().chain(operations).collect(),
            router: self.router.nest(path, router.router),
        }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            operations: self.operations.into_iter().chain(other.operations).collect(),
            router: self.router.merge(other.router),
        }
    }

    pub fn layer<L>(self, layer: L) -> Self
    where
        L: Layer<Route> + Clone + Send + 'static,
        L::Service: Service<Request> + Clone + Send + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        Self {
            operations: self.operations,
            router: self.router.layer(layer),
        }
    }
}

#[doc(hidden)]
pub fn extract_operable<H, T, State>() -> Option<&'static Operable>
where
    H: Handler<T, State>,
    T: 'static,
{
    let handle_name = std::any::type_name::<H>();
    inventory::iter::<Operable>.into_iter().find(|it| it.type_name.eq(handle_name))
}
