use std::convert::Infallible;

use axum::extract::Request;
use axum::handler::Handler;
pub use axum::response::IntoResponse as Responder;
use axum::routing::{MethodFilter, MethodRouter, Route};
use axum::Router;
use oas::{Info, OpenAPIV3, PathItem, Tag};
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
    pub(crate) openapi_spec: OpenAPIV3,
    pub(crate) router: Router<State>,
}

impl<State: Clone + Send + Sync + 'static> GotchaRouter<State> {
    pub fn new() -> Self {
        Self {
            openapi_spec: OpenAPIV3 {
                openapi: "3.0.0".to_string(),
                info: Info {
                    title: "".to_string(),
                    description: None,
                    terms_of_service: None,
                    contact: None,
                    license: None,
                    version: "".to_string(),
                },
                servers: None,
                paths: Default::default(),
                components: None,
                security: None,
                tags: Some(vec![]),
                external_docs: None,
                extras: None,
            },
            router: Router::new(),
        }
    }
    pub fn route(self, path: &str, method_router: MethodRouter<State>) -> Self {
        Self {
            openapi_spec: self.openapi_spec,
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
            if let Some(added_tags) = &operation.tags {
                added_tags.iter().for_each(|tag| {
                    if let Some(tags) = &mut self.openapi_spec.tags {
                        if !tags.iter().any(|each| each.name.eq(tag)) {
                            tags.push(Tag::new(tag, None))
                        }
                    }
                })
            }
            let entry = self.openapi_spec.paths.entry(path.to_string()).or_insert_with(|| PathItem {
                _ref: None,
                summary: None,
                description: None,
                get: None,
                put: None,
                post: None,
                delete: None,
                options: None,
                head: None,
                patch: None,
                trace: None,
                servers: None,
                parameters: None,
            });
            match method {
                MethodFilter::GET => entry.get = Some(operation),
                MethodFilter::POST => entry.post = Some(operation),
                MethodFilter::PUT => entry.put = Some(operation),
                MethodFilter::DELETE => entry.delete = Some(operation),
                MethodFilter::HEAD => entry.head = Some(operation),
                MethodFilter::OPTIONS => entry.options = Some(operation),
                MethodFilter::PATCH => entry.patch = Some(operation),
                MethodFilter::TRACE => entry.trace = Some(operation),
                _ => {}
            };
        }

        let router = MethodRouter::new().on(method, handler);

        Self {
            openapi_spec: self.openapi_spec,
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

    pub fn layer<L>(self, layer: L) -> Self
    where
        L: Layer<Route> + Clone + Send + 'static,
        L::Service: Service<Request> + Clone + Send + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        Self {
            openapi_spec: self.openapi_spec,
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
