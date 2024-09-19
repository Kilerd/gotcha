use std::convert::Infallible;
use std::marker::PhantomData;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;

pub use async_trait::async_trait;
use axum::extract::Request;
pub use axum::extract::{Json, Path, Query, State};
use axum::handler::Handler;
pub use axum::response::IntoResponse as Responder;
use axum::response::Response;
pub use axum::routing::{delete, get, patch, post, put};
use axum::routing::{MethodFilter, MethodRouter, Route};
use axum::serve::IncomingStream;
use axum::Router;
pub use either::Either;
pub use gotcha_core::{ParameterProvider, Schematic};
pub use gotcha_macro::*;
use log::info;
use oas::{Info, OpenAPIV3, PathItem, Tag};
pub use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use tower_layer::Layer;
use tower_service::Service;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};
pub use {axum, inventory, oas, tracing};

pub use crate::config::GotchaConfigLoader;
pub use crate::message::{Message, Messager};
pub use crate::openapi::Operable;
use crate::state::ExtendableState;

pub use axum_macros::debug_handler;
#[cfg(feature = "prometheus")]
pub mod prometheus {
    pub use axum_prometheus::metrics::*;
}

mod config;
pub mod message;
pub mod openapi;
pub mod task;

pub mod state;

pub struct GotchaApp<State = (), const DONE: bool = false, const HAS_STATE: bool = false> {
    api_endpoint: Option<String>,
    openapi_spec: OpenAPIV3,
    tasks: Vec<Box<dyn Fn()>>,

    pub app: Router<State>,
}

macro_rules! implement_method {
    ($method:expr, $fn_name: tt ) => {
        pub fn $fn_name<H: Handler<T, State>, T: 'static>(self, path: &str, handler: H) -> Self {
            self.method_route(path, $method, handler)
        }
    };
}

pub fn try_init_logger() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
    info!("logger has been initialized");
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

impl<State> GotchaApp<State, false>
where
    State: Clone + Send + Sync + 'static,
{
    pub fn new() -> GotchaApp<State, false> {
        GotchaApp {
            api_endpoint: None,
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
            tasks: vec![],
            app: Router::new(),
        }
    }

    // todo default service

    pub fn api_endpoint(self, path: impl Into<String>) -> Self {
        Self {
            api_endpoint: Some(path.into()),
            ..self
        }
    }

    pub fn route(self, path: &str, method_router: MethodRouter<State>) -> Self {
        Self {
            api_endpoint: self.api_endpoint,
            openapi_spec: self.openapi_spec,
            tasks: self.tasks,
            app: self.app.route(path, method_router),
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
            api_endpoint: self.api_endpoint,
            openapi_spec: self.openapi_spec,
            tasks: self.tasks,
            app: self.app.route(path, router),
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

    pub fn layer<L>(self, layer: L) -> GotchaApp<State>
    where
        L: Layer<Route> + Clone + Send + 'static,
        L::Service: Service<Request> + Clone + Send + 'static,
        <L::Service as Service<Request>>::Response: Responder + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        GotchaApp {
            app: self.app.layer(layer),
            api_endpoint: self.api_endpoint,
            openapi_spec: self.openapi_spec,
            tasks: self.tasks,
        }
    }
    pub fn data(self, data: State) -> GotchaApp<()> {
        GotchaApp {
            api_endpoint: self.api_endpoint,
            openapi_spec: self.openapi_spec,
            tasks: self.tasks,
            app: self.app.with_state(data),
        }
    }

    pub fn task<Task, TaskRet>(mut self, t: Task) -> Self
    where
        Task: (Fn() -> TaskRet) + 'static,
        TaskRet: std::future::Future<Output = ()> + Send + 'static,
    {
        self.tasks.push(Box::new(move || {
            tokio::spawn(t());
        }));

        self
    }
}

impl GotchaApp<(), false> {
    pub fn done(self) -> GotchaApp<(), true> {
        let app = self;
        let router = app.app;
        let apiv3 = app.openapi_spec;
        let apiv3_2 = apiv3.clone();
        let router1: Router<()> = router
            .route(
                app.api_endpoint.as_deref().unwrap_or("/openapi.json"),
                axum::routing::get(|| async move { Json(apiv3.clone()) }),
            )
            .route("/redoc", axum::routing::get(openapi::openapi_html))
            .route("/scalar", axum::routing::get(openapi::scalar_html));

        GotchaApp {
            api_endpoint: app.api_endpoint,
            openapi_spec: apiv3_2,
            tasks: app.tasks,
            app: router1,
        }
    }
}

impl<R> GotchaApp<(), true>
where
    for<'a> Router<()>: Service<IncomingStream<'a>, Response = R, Error = Infallible> + Send + 'static,
    for<'a> <Router<()> as Service<IncomingStream<'a>>>::Future: Send,
    R: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
    R::Future: Send,
{
    pub async fn serve(self, addr: &str, port: u16) {
        #[cfg(feature = "prometheus")]
        use axum_prometheus::PrometheusMetricLayer;
        #[cfg(feature = "prometheus")]
        let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

        let app: Router<_> = self.app;

        #[cfg(feature = "prometheus")]
        let app = app.route("/metrics", get(|| async move { metric_handle.render() })).layer(prometheus_layer);

        let addr = SocketAddrV4::new(Ipv4Addr::from_str(addr).unwrap(), port);
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn pass() {
        let t = trybuild::TestCases::new();
        t.pass("tests/pass/*.rs");
    }
}
