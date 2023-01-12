use actix_service::IntoServiceFactory;
use actix_web::body::MessageBody;
use actix_web::dev::ServiceResponse;
use actix_web::dev::Transform;
pub use actix_web::web::Data;
pub use actix_web::App;
pub use actix_web::HttpServer;
pub use actix_web::Responder;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    http, web,
};
pub use async_trait::async_trait;
use gotcha_lib::{GotchaOperationObject, Operation};
pub use gotcha_macro::get;
use std::{collections::HashMap, sync::Arc};

pub mod wrapper {
    pub use gotcha_lib;
}
pub mod cli;
mod config;
pub use cli::GotchaCli;
pub use tracing;

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
    pub fn wrap<M, B>(
        self,
        mw: M,
    ) -> GotchaApp<
        impl ServiceFactory<
            ServiceRequest,
            Config = (),
            Response = ServiceResponse<B>,
            Error = actix_web::Error,
            InitError = (),
        >,
    >
    where
        M: Transform<
                T::Service,
                ServiceRequest,
                Response = ServiceResponse<B>,
                Error = actix_web::Error,
                InitError = (),
            > + 'static,
        B: MessageBody,
    {
        let inner = self.inner.wrap(mw);
        GotchaApp {
            inner,
            api_endpoint: self.api_endpoint,
            paths: self.paths,
        }
    }

    pub fn default_service<F, U>(self, svc: F) -> Self
    where
        F: IntoServiceFactory<U, ServiceRequest>,
        U: ServiceFactory<
                ServiceRequest,
                Config = (),
                Response = ServiceResponse,
                Error = actix_web::Error,
            > + 'static,
        U::InitError: std::fmt::Debug,
    {
        let inner = self.inner.default_service(svc);

        GotchaApp {
            inner,
            api_endpoint: self.api_endpoint,
            paths: self.paths,
        }
    }

    pub fn api_endpoint(mut self, path: impl Into<String>) -> Self {
        self.api_endpoint = Some(path.into());
        self
    }
    pub fn data<U: 'static>(self, ext: U) -> Self {
        let ext_data = web::Data::new(ext);
        Self {
            inner: self.inner.app_data(ext_data),
            ..self
        }
    }
    pub fn done(self) -> App<T> {
        // todo add swagger api
        // init messager
        self.data(Messager {}).inner
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

#[cfg(test)]
mod tests {}
