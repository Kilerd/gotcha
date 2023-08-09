use actix_service::IntoServiceFactory;
use actix_web::{dev::{ServiceFactory, ServiceRequest}};
pub use actix_web;
pub use actix_web::App;
use actix_web::body::MessageBody;
use actix_web::dev::ServiceResponse;
use actix_web::dev::Transform;
pub use actix_web::HttpServer;
pub use actix_web::Responder;
pub use actix_web::web::Data;
pub use async_trait::async_trait;
use http::Method;
use oas::{Info, OpenAPIV3, PathItem, Tag};
pub use oas;
pub use tracing;

pub use cli::GotchaCli;
pub use gotcha_core::*;
pub use gotcha_macro::*;

pub use crate::message::{Messager, Message, MessagerWrapper};
use crate::openapi::{openapi_handler, openapi_html};

pub mod web {
    pub use actix_web::web::{Path, Json, Query, Header};
}
pub mod cli;
mod config;
pub mod message;
pub mod openapi;
pub mod task;

pub struct GotchaApp<T> {
    api_endpoint: Option<String>,
    openapi_spec: OpenAPIV3,
    inner: actix_web::App<T>,
    tasks: Vec<Box<dyn Fn()>>,
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
            api_endpoint: None,
            tasks: vec![],
        }
    }
}

impl<T> GotchaApp<T>
    where
        T: ServiceFactory<ServiceRequest, Config=(), Error=actix_web::Error, InitError=()>,
{
    pub fn service<F>(mut self, factory: F) -> Self
        where
            F: Operable + actix_web::dev::HttpServiceFactory + 'static,
    {
        if factory.should_generate_openapi_spec() {
            let operation_object = factory.generate();
            if let Some(added_tags) = &operation_object.tags {
                added_tags.iter().for_each(|tag| {
                    if let Some(tags) = &mut self.openapi_spec.tags {
                        if tags.iter().find(|each| each.name.eq(tag)).is_none() {
                            tags.push(Tag::new(tag, None))
                        }
                    }
                })
            }
            let mut entry = self.openapi_spec.paths.entry(factory.uri().to_string()).or_insert_with(|| PathItem {
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
            match factory.method() {
                Method::GET => entry.get = Some(operation_object),
                Method::POST => entry.post = Some(operation_object),
                Method::PUT => entry.put = Some(operation_object),
                Method::DELETE => entry.delete = Some(operation_object),
                Method::HEAD => entry.head = Some(operation_object),
                Method::OPTIONS => entry.options = Some(operation_object),
                Method::PATCH => entry.patch = Some(operation_object),
                Method::TRACE => entry.trace = Some(operation_object),
                _ => {}
            };
        }
        Self {
            inner: self.inner.service(factory),
            ..self
        }
    }
    pub fn wrap<M, B>(
        self,
        mw: M,
    ) -> GotchaApp<
        impl ServiceFactory<
            ServiceRequest,
            Config=(),
            Response=ServiceResponse<B>,
            Error=actix_web::Error,
            InitError=(),
        >,
    >
        where
            M: Transform<
                T::Service,
                ServiceRequest,
                Response=ServiceResponse<B>,
                Error=actix_web::Error,
                InitError=(),
            > + 'static,
            B: MessageBody,
    {
        let inner = self.inner.wrap(mw);
        GotchaApp {
            inner,
            api_endpoint: self.api_endpoint,
            openapi_spec: self.openapi_spec,
            tasks: vec![],
        }
    }

    pub fn default_service<F, U>(self, svc: F) -> Self
        where
            F: IntoServiceFactory<U, ServiceRequest>,
            U: ServiceFactory<
                ServiceRequest,
                Config=(),
                Response=ServiceResponse,
                Error=actix_web::Error,
            > + 'static,
            U::InitError: std::fmt::Debug,
    {
        let inner = self.inner.default_service(svc);

        GotchaApp {
            inner,
            api_endpoint: self.api_endpoint,
            openapi_spec: self.openapi_spec,
            tasks: self.tasks,
        }
    }

    pub fn api_endpoint(self, path: impl Into<String>) -> Self {
        Self {
            api_endpoint: Some(path.into()),
            ..self
        }
    }
    pub fn data<U: 'static>(self, ext: U) -> Self {
        let ext_data = actix_web::web::Data::new(ext);
        Self {
            inner: self.inner.app_data(ext_data),
            ..self
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
    pub fn done(self) -> App<T> {
        // todo add swagger api
        // init messager
        let apiv3 = self.openapi_spec.clone();
        let app = self.data(Messager {}).data(apiv3);
        // start task
        for task in app.tasks {
            task();
        }
        let openapi_handler = actix_web::web::resource("/openapi.json").to(openapi_handler);
        let redoc_handler = actix_web::web::resource("/swagger-ui").to(openapi_html);
        app.inner.service(openapi_handler)
            .service(redoc_handler)
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
