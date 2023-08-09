use gotcha::{get, post, put, patch, delete, head, Operable, Schematic};
use oas::{Parameter, Schema};
use actix_web::Responder;

#[get("/hello-world")]
async fn handler() -> String {
    "Hello world".to_string()
}

#[post("/hello-world")]
async fn post_handler() -> String {
    "Hello world".to_string()
}

#[put("/hello-world")]
async fn put_handler() -> String {
    "Hello world".to_string()
}

#[patch("/hello-world")]
async fn patch_handler() -> String {
    "Hello world".to_string()
}

#[delete("/hello-world")]
async fn delete_handler() -> String {
    "Hello world".to_string()
}

#[head("/hello-world")]
async fn head_handler() -> String {
    "Hello world".to_string()
}

#[get("/hello-world")]
async fn handler_with_impl_response() -> impl Responder {
    "Hello world".to_string()
}

fn main() {
    let operation = handler.generate();
    assert!(operation.operation_id == Some("handler".to_string()));
    assert!(operation.description == None);
    assert!(operation.deprecated == Some(false));
    assert!(operation.tags.is_none());
}