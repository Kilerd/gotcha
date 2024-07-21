use std::convert::Infallible;
use std::marker::PhantomData;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;
pub use async_trait::async_trait;
use axum::body::Body;
use axum::extract::Request;
use axum::{Router, ServiceExt};
use axum::routing::{MethodRouter, Route};
pub use cli::GotchaCli;
pub use gotcha_core::*;
pub use gotcha_macro::*;
use http::Method;
use oas::{OpenAPIV3};
use tower_layer::Layer;
use tower_service::Service;
use tracing_subscriber::Layer as TracingLayer;
pub use {oas, tracing, axum};

pub use crate::message::{Message, Messager};

pub use axum::response::IntoResponse as Responder;

pub use axum::extract::{Path, Query, Json, State};
use axum::response::Response;
pub use axum::routing::{get, post, put, delete, patch};
use axum::serve::IncomingStream;
use serde::de::DeserializeOwned;
use crate::config::GotchaConfigLoader;
use crate::state::ExtendableState;

pub mod cli;
mod config;
pub mod message;
pub mod openapi;
pub mod task;

pub mod state;

pub struct GotchaApp<State, Config: DeserializeOwned, Data = (), const DONE: bool = false, const HAS_STATE: bool = false>
where
    Config: for<'de> serde::Deserialize<'de>,
{
    api_endpoint: Option<String>,
    openapi_spec: Option<OpenAPIV3>,
    tasks: Vec<Box<dyn Fn()>>,

    data: Data,
    pub app: Router<State>,

    config: PhantomData<Config>,
}


impl<State, Config, Data> GotchaApp<State, Config, Data, false>
where
    State: Clone + Send + Sync + 'static,
    Config: for<'de> serde::Deserialize<'de>,
{
    pub fn new() -> GotchaApp<State, Config, (), false> {
        GotchaApp {
            api_endpoint: None,
            openapi_spec: None,
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
        let router = app.app;
        let data = app.data;
        let router1 = router.with_state(data.clone());
        GotchaApp {
            api_endpoint: app.api_endpoint,
            openapi_spec: app.openapi_spec,
            tasks: app.tasks,
            data,
            app: router1,
            config: app.config,
        }
    }
}


impl<Config, Data, R> GotchaApp<(), Config, Data, true>
where
    // State: Clone + Send + Sync + 'static,
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
