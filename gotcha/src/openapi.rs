use actix_web::web::{Data, Json};
use actix_web::HttpResponse;
use oas::OpenAPIV3;

pub(crate) async fn openapi_handler(spec: Data<OpenAPIV3>) -> Json<OpenAPIV3> {
    Json(spec.get_ref().clone())
}

pub(crate) async fn openapi_html() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=UTF-8")
        .body(include_str!("../statics/redoc.html"))
}
