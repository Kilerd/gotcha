use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    Responder, http,
};
use gotcha_lib::{GotchaOperationObject, Operation};
use gotcha_macro::get;
use std::{collections::HashMap, hash::Hash};

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
        GotchaApp { inner: self, paths: HashMap::new(), api_endpoint: None }
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
        let uri_map = self.paths.entry(uri).or_insert_with(||HashMap::new());
        uri_map.insert(method, operation_object);
        self.inner = self.inner.service(factory);
        self
    }

    pub fn api_endpoint(mut self, path: impl Into<String>) -> Self {
        self.api_endpoint = Some(path.into());
        self
    }
}

/// test
#[get("/", group="pest")]
pub async fn hello_world() -> impl Responder {
    "hello world"
}

#[cfg(test)]
mod tests {
    use actix_web::App;

    use super::*;



    #[test]
    fn should_add_to_app() {
        let app = App::new().into_gotcha().service(hello_world);
        assert_eq!(1, app.paths.len());
    
    }
}
