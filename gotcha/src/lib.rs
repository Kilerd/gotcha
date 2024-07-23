use std::convert::Infallible;
use std::marker::PhantomData;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;
pub use async_trait::async_trait;
use axum::body::Body;
use axum::extract::Request;
use axum::{Router, ServiceExt};
use axum::routing::{MethodFilter, MethodRouter, Route};
pub use cli::GotchaCli;
pub use gotcha_macro::*;
use http::Method;
use oas::{Info, OpenAPIV3, Parameter, PathItem, RequestBody, Tag};
use tower_layer::Layer;
use tower_service::Service;
use tracing_subscriber::Layer as TracingLayer;
pub use {oas, tracing, axum};

pub use crate::message::{Message, Messager};

pub use axum::response::IntoResponse as Responder;

pub use axum::extract::{Path, Query, Json, State};
use axum::handler::Handler;
use axum::response::Response;
pub use axum::routing::{post, put, delete, patch};
use axum::serve::IncomingStream;
use cron::TimeUnitSpec;
pub use either::Either;
use serde::de::DeserializeOwned;
use crate::config::GotchaConfigLoader;
use crate::state::ExtendableState;
pub use gotcha_core::ParameterProvider;
pub use once_cell::sync::Lazy;

pub use gotcha_core::Schematic;
pub use inventory;
use log::info;
pub use crate::openapi::Operable;
pub mod cli;
mod config;
pub mod message;
pub mod openapi;
pub mod task;

pub mod state;



pub fn get<H, T, S>(handler: H) -> MethodRouter<S, Infallible>
where
    H: Handler<T, S>,
    T: 'static,
    S: Clone + Send + Sync + 'static,
{
    dbg!(std::any::type_name::<H>());
    let router = MethodRouter::new();

    router.on(MethodFilter::GET, handler)
}

pub struct GotchaApp<State, Config: DeserializeOwned, Data = (), const DONE: bool = false, const HAS_STATE: bool = false>
where
    Config: for<'de> serde::Deserialize<'de>,
{
    api_endpoint: Option<String>,
    openapi_spec: OpenAPIV3,
    tasks: Vec<Box<dyn Fn()>>,

    data: Data,
    pub app: Router<State>,

    config: PhantomData<Config>,
}


macro_rules! implement_method {
    ($method:expr, $fn_name: tt ) => {
        pub fn $fn_name<H: Handler<T, State>, T: 'static>(self, path: &str, handler: H) -> Self {
            self.method_route(path, $method, handler)
        }
    };
}

#[doc(hidden)]
pub fn extract_operable<H, T, State>() -> Option<&'static Operable>
where
    H: Handler<T, State>,
    T: 'static,
{
    let handle_name = std::any::type_name::<H>();
    let handle_operable = inventory::iter::<Operable>.into_iter().find(|it| it.type_name.eq(handle_name));
    handle_operable

}

impl<State, Config, Data> GotchaApp<State, Config, Data, false>
where
    State: Clone + Send + Sync + 'static,
    Config: for<'de> serde::Deserialize<'de>,
{
    pub fn new() -> GotchaApp<State, Config, (), false> {
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
            data: (),
            app: Router::new(),
            config: Default::default(),
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
            data: self.data,
            app: self.app.route(path, method_router),
            config: self.config,
        }
    }

    pub fn method_route<H, T>(mut self, path: &str, method: MethodFilter, handler: H) -> Self
    where
        H: Handler<T, State>,
        T: 'static,
    {

        let handle_operable = extract_operable::<H,T,State>();
        if let Some(operable) = handle_operable {
            info!("generating openapi spec for {}[{}]", &operable.type_name, &path);
            let operation = operable.generate(path.to_string());
            if let Some(added_tags) = &operation.tags {
                added_tags.iter().for_each(|tag| {
                    if let Some(tags) = &mut self.openapi_spec.tags {
                        if tags.iter().find(|each| each.name.eq(tag)).is_none() {
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
            data: self.data,
            app: self.app.route(path, router),
            config: self.config,
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

    pub fn layer<L>(self, layer: L) -> GotchaApp<State, Config, Data>
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
            config: self.config,
            data: self.data,
        }
    }
    pub fn data<Data2, NewPartialData>(self, state: NewPartialData) -> GotchaApp<State, Config, Data2>
    where
        Data: ExtendableState<NewPartialData, Ret=Data2>,
        Data2: Clone,
    {
        let new_state = self.data.extend(state);
        GotchaApp {
            app: self.app,
            api_endpoint: self.api_endpoint,
            openapi_spec: self.openapi_spec,
            tasks: self.tasks,
            config: self.config,
            data: new_state.clone(),
        }
    }

    pub fn task<Task, TaskRet>(mut self, t: Task) -> Self
    where
        Task: (Fn() -> TaskRet) + 'static,
        TaskRet: std::future::Future<Output=()> + Send + 'static,
    {
        self.tasks.push(Box::new(move || {
            tokio::spawn(t());
        }));

        self
    }


    pub fn done(self) -> GotchaApp<(), Config, State, true>
    where

        Data: ExtendableState<Config, Ret=State>,
    {
        let config: Config = GotchaConfigLoader::load(None);

        let app = self.data(config);
        let data = app.data;
        let router = app.app;
        let apiv3 = app.openapi_spec;
        let apiv3_2 = apiv3.clone();
        let router1 = router
            .route(app.api_endpoint.as_deref().unwrap_or("/openapi.json"), get(|| async move { Json(apiv3.clone()) }))
            .route("/redoc", get(openapi::openapi_html))
            .with_state(data.clone());

        GotchaApp {
            api_endpoint: app.api_endpoint,
            openapi_spec: apiv3_2,
            tasks: app.tasks,
            data,
            app: router1,
            config: app.config,
        }
    }
}


impl<Config, Data, R> GotchaApp<(), Config, Data, true>
where
        for<'a> Router<()>: Service<IncomingStream<'a>, Response=R, Error=Infallible> + Send + 'static,
        for<'a> <Router<()> as Service<IncomingStream<'a>>>::Future: Send,
        R: Service<Request, Response=Response, Error=Infallible> + Clone + Send + 'static,
        R::Future: Send,
        Config: for<'de> serde::Deserialize<'de>,
{
    pub async fn serve(self, addr: &str, port: u16) -> () {
        let app: Router<_> = self.app;
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
