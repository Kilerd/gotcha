use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;

pub use async_trait::async_trait;
use axum::extract::{FromRef, Request};
pub use axum::extract::{Json, Path, Query, State};
use axum::handler::Handler;
pub use axum::response::IntoResponse as Responder;
pub use axum::routing::{delete, get, patch, post, put};
use axum::routing::{MethodFilter, MethodRouter, Route};
use axum::Router;
pub use axum_macros::debug_handler;
pub use config::ConfigWrapper;
pub use either::Either;
pub use gotcha_core::{ParameterProvider, Schematic};
pub use gotcha_macro::*;
use oas::{Info, OpenAPIV3, PathItem, Tag};
pub use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tower_layer::Layer;
use tower_service::Service;
use tracing::info;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};
pub use {axum, inventory, oas, tracing};

pub use crate::config::GotchaConfigLoader;
pub use crate::message::{Message, Messager};
pub use crate::openapi::Operable;
mod config;
pub mod message;
pub mod openapi;
pub mod state;
pub mod task;

#[cfg(feature = "prometheus")]
pub mod prometheus {
    pub use axum_prometheus::metrics::*;
}

macro_rules! implement_method {
    ($method:expr, $fn_name: tt ) => {
        pub fn $fn_name<H: Handler<T, State>, T: 'static>(self, path: &str, handler: H) -> Self {
            self.method_route(path, $method, handler)
        }
    };
}

#[derive(Clone)]
pub struct GotchaContext<State, Config: Clone + Serialize + for<'de> Deserialize<'de>> {
    pub config: ConfigWrapper<Config>,
    pub state: State,
}

impl<State, Config: Clone + Serialize + for<'de> Deserialize<'de>> FromRef<GotchaContext<State, Config>> for ConfigWrapper<Config> {
    fn from_ref(context: &GotchaContext<State, Config>) -> Self {
        context.config.clone()
    }
}

pub struct GotchaRouter<State = ()> {
    openapi_spec: OpenAPIV3,
    router: Router<State>,
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

#[async_trait]
pub trait GotchaApp: Sized {
    type State: Clone + Send + Sync + 'static;
    type Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de>;

    fn logger(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env())
            .try_init()?;
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>>;
    async fn state(&self) -> Result<Self::State, Box<dyn std::error::Error>>;

    async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        self.logger()?;
        info!("logger has been initialized");
        let config: ConfigWrapper<Self::Config> = GotchaConfigLoader::load::<ConfigWrapper<Self::Config>>(std::env::var("GOTCHA_ACTIVE_PROFILE").ok());
        let state = self.state().await?;
        let context = GotchaContext { config: config.clone(), state };
        let router = GotchaRouter::<GotchaContext<Self::State, Self::Config>>::new();
        let router = self.routes(router);

        let GotchaRouter {
            openapi_spec,
            router: raw_router,
        } = router;

        let router = raw_router
            .with_state(context)
            .route("/openapi.json", axum::routing::get(|| async move { Json(openapi_spec.clone()) }))
            .route("/redoc", axum::routing::get(openapi::openapi_html))
            .route("/scalar", axum::routing::get(openapi::scalar_html));

        let addr = SocketAddrV4::new(Ipv4Addr::from_str(&config.basic.host)?, config.basic.port);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, router).await?;
        Ok(())
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

#[cfg(test)]
mod test {
    #[test]
    fn pass() {
        let t = trybuild::TestCases::new();
        t.pass("tests/pass/*.rs");
    }
}
