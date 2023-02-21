use gotcha_core::{Operable, Schematic, ParameterProvider};
use gotcha_macro::{get, post, put, patch, delete, head, Schematic};
use oas::{Parameter, Schema, Convertible};
use actix_web::Responder;
use actix_web::web::{Path, Query, Json, Data};
use serde::Deserialize;
#[derive(Clone)]
pub struct AppData {

}

#[derive(Clone, Deserialize, Schematic)]
pub struct PathArgs {
    id: String
}

#[derive(Clone, Deserialize, Schematic)]
pub struct QueryArgs {
    page: i32
}
#[derive(Clone, Deserialize, Schematic)]
pub struct RequestPayload {
    name: String,
    id: i32
}


#[post("/resources/{id}")]
async fn post_handler(tuple_path: Path<(i32, )>, struct_path: Query<QueryArgs>, payload:Json<RequestPayload>, data:Data<AppData>) -> impl Responder {
    "Hello world".to_string()
}

fn main() {
    let operation = post_handler.generate();
    assert!(operation.operation_id == Some("post_handler".to_string()));
    assert!(operation.description == None);
    assert!(operation.deprecated == Some(false));
    assert!(operation.tags.is_none());
}