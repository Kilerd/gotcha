use std::collections::BTreeMap;
use actix_web::HttpResponse;
use actix_web::web::{Data, Json};
use http::Method;
use oas::{OpenAPIV3, Operation, Parameter, Responses};
use gotcha_core::ApiObject;

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


pub(crate) async fn openapi_handler(spec: Data<OpenAPIV3>) -> Json<OpenAPIV3> {
    Json(spec.get_ref().clone())
}

pub(crate) async fn openapi_html() -> HttpResponse {
    HttpResponse::Ok().body( include_str!("../statics/redoc.html"))
}