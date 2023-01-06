use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    http, web,
};

use gotcha_lib::{GotchaOperationObject, Operation};
pub use gotcha_macro::get;
use serde::de::DeserializeOwned;
use std::{collections::HashMap, sync::Arc};

pub use actix_web::App;
pub use actix_web::HttpServer;
pub use actix_web::Responder;

pub use async_trait::async_trait;

pub mod wrapper {
    pub use gotcha_lib;
}

pub mod cli;
pub use cli::GotchaCli;

trait ApiObject {
    fn name() -> &'static str;
    fn required() -> bool;
    fn type_() -> &'static str;
}

impl ApiObject for String {
    fn name() -> &'static str {
        unimplemented!()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }
}
impl ApiObject for i32 {
    fn name() -> &'static str {
        "integer"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "integer"
    }
}

struct MyRequest {
    name: String,
    fav_number: i32,
}

impl ApiObject for MyRequest {
    fn name() -> &'static str {
        "MyRequest"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }
}

pub struct GotchaApp<T> {
    api_endpoint: Option<String>,
    paths: HashMap<String, HashMap<http::Method, GotchaOperationObject>>,
    inner: actix_web::App<T>,
}

pub trait GotchaAppWrapperExt<T> {
    type Wrapper;
    fn into_gotcha(self) -> Self::Wrapper;
}

impl<T> GotchaAppWrapperExt<T> for actix_web::App<T> {
    type Wrapper = GotchaApp<T>;

    fn into_gotcha(self) -> Self::Wrapper {
        GotchaApp {
            inner: self,
            paths: HashMap::new(),
            api_endpoint: None,
        }
    }
}

impl<T> GotchaApp<T>
where
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
{
    pub fn service<F>(mut self, factory: F) -> Self
    where
        F: Operation + actix_web::dev::HttpServiceFactory + 'static,
    {
        let uri = factory.uri().to_string();
        let method = factory.method();
        let operation_object = factory.generate_gotcha_operation_object();
        let uri_map = self.paths.entry(uri).or_insert_with(|| HashMap::new());
        uri_map.insert(method, operation_object);
        self.inner = self.inner.service(factory);
        self
    }

    pub fn api_endpoint(mut self, path: impl Into<String>) -> Self {
        self.api_endpoint = Some(path.into());
        self
    }

    pub fn done(self) -> App<T> {
        // todo add swagger api
        // init messager
        let messager = web::Data::new(Messager {});
        self.inner.app_data(messager)
    }
}

pub struct Messager {}

pub type MessagerWrapper = web::Data<Messager>;

impl Messager {
    pub async fn send<T: Message>(self: Arc<Self>, msg: T) -> T::Output {
        msg.handle(self).await
    }
    pub async fn spawn<T>(self: Arc<Self>, msg: T) -> ()
    where
        T: Message + 'static,
        T::Output: Send,
    {
        tokio::spawn(msg.handle(self));
    }
}

#[async_trait]
pub trait Message {
    type Output;
    async fn handle(self, messager: Arc<Messager>) -> Self::Output;
}

pub struct GotchaConfig<T> {
    data: T,
}
impl<T: DeserializeOwned> GotchaConfig<T> {
    pub fn new() -> GotchaConfig<T> {
        let run_mode = std::env::var("RUN_MODE").ok();

        let mut s = config::Config::builder()
            // Start off by merging in the "default" configuration file
            .add_source(config::File::with_name("configurations/application"));

        if let Some(environment) = run_mode {
            s = s.add_source(
                config::File::with_name(&format!("configurations/application_{}", environment))
                    .required(false),
            )
        }

        s = s.add_source(config::Environment::with_prefix("APP"));

        let b = s.build().unwrap();
        let ret = b.try_deserialize().unwrap();
        GotchaConfig { data: ret }
    }
}

#[cfg(test)]
mod tests {}
