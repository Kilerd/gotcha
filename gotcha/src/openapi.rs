use actix_web::dev::Payload;
use actix_web::web::{Data, Json, Path};
use actix_web::{FromRequest, HttpRequest, HttpResponse};
use http::Method;
use oas::{OpenAPIV3, Operation, Parameter, ParameterIn, Responses};
use std::collections::BTreeMap;

pub trait ApiObject {
    fn location() -> Option<ParameterIn>;
    fn name() -> Option<&'static str>;
    fn required() -> bool;
    fn type_() -> &'static str;
    fn generate() -> Option<Vec<Parameter>> {
        todo!()
    }
}

impl ApiObject for i32 {
    fn location() -> Option<ParameterIn> {
        None
    }

    fn name() -> Option<&'static str> {
        None
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "number"
    }
}

impl<T: ApiObject> ApiObject for Option<T> {
    fn location() -> Option<ParameterIn> {
        T::location()
    }

    fn name() -> Option<&'static str> {
        T::name()
    }

    fn required() -> bool {
        false
    }

    fn type_() -> &'static str {
        T::type_()
    }
}

struct MyRequest {
    name: String,
    fav_number: i32,
}

impl ApiObject for MyRequest {
    fn location() -> Option<ParameterIn> {
        None
    }
    fn name() -> Option<&'static str> {
        Some("MyRequest")
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }
}

impl<T> ApiObject for Path<T> {
    fn location() -> Option<ParameterIn> {
        Some(ParameterIn::Path)
    }
    fn name() -> Option<&'static str> {
        todo!()
    }

    fn required() -> bool {
        todo!()
    }

    fn type_() -> &'static str {
        todo!()
    }
}

pub(crate) async fn openapi_handler(spec: Data<OpenAPIV3>) -> Json<OpenAPIV3> {
    Json(spec.get_ref().clone())
}

pub(crate) async fn openapi_html() -> HttpResponse {
    HttpResponse::Ok().body(include_str!("../statics/redoc.html"))
}
