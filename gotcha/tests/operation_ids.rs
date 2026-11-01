#![cfg(feature = "openapi")]
#![allow(dead_code)]

//! Operations must never share an `operationId`.
//!
//! The id defaults to the handler's function name, so two modules' `list` functions — or one
//! handler mounted at two paths — would otherwise emit the same id. The first operation (the
//! document is sorted by path and method) keeps its id; each later duplicate gets `_2`, `_3`, ...

use std::collections::HashMap;

use gotcha::axum::http::Method;
use gotcha::{api, openapi::generate_openapi, Operable};
use serde_json::Value;

mod users {
    use super::*;

    #[api]
    pub async fn list() -> String {
        String::new()
    }
}

mod books {
    use super::*;

    #[api]
    pub async fn list() -> String {
        String::new()
    }
}

#[api(id = "shared")]
async fn explicit_first() -> String {
    String::new()
}

#[api(id = "shared")]
async fn explicit_second() -> String {
    String::new()
}

#[api(id = "list_2")]
async fn explicit_suffix() -> String {
    String::new()
}

fn operable<H, T>(_: H) -> &'static Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().expect("handler is registered")
}

fn assemble(routes: &[(&str, Method, &'static Operable)]) -> Value {
    let routes: HashMap<_, _> = routes
        .iter()
        .map(|(path, method, operable)| ((path.to_string(), method.clone()), *operable))
        .collect();
    serde_json::to_value(generate_openapi(routes)).unwrap()
}

fn operation_id<'a>(spec: &'a Value, path: &str, method: &str) -> &'a str {
    spec["paths"][path][method]["operationId"].as_str().expect("operation has an id")
}

#[test]
fn duplicate_default_ids_are_suffixed_in_document_order() {
    let spec = assemble(&[("/books", Method::GET, operable(books::list)), ("/users", Method::GET, operable(users::list))]);

    assert_eq!(operation_id(&spec, "/books", "get"), "list");
    assert_eq!(operation_id(&spec, "/users", "get"), "list_2");
}

#[test]
fn one_handler_mounted_twice_gets_distinct_ids() {
    let handler = operable(users::list);
    let spec = assemble(&[("/users", Method::GET, handler), ("/users/{id}", Method::GET, handler)]);

    assert_eq!(operation_id(&spec, "/users", "get"), "list");
    assert_eq!(operation_id(&spec, "/users/{id}", "get"), "list_2");
}

#[test]
fn methods_on_one_path_are_visited_in_document_order() {
    // `PathItem` writes GET before DELETE, so GET keeps the id even though "DELETE" sorts
    // first alphabetically.
    let handler = operable(users::list);
    let spec = assemble(&[("/items", Method::DELETE, handler), ("/items", Method::GET, handler)]);

    assert_eq!(operation_id(&spec, "/items", "get"), "list");
    assert_eq!(operation_id(&spec, "/items", "delete"), "list_2");
}

#[test]
fn duplicate_explicit_ids_are_suffixed_too() {
    let spec = assemble(&[("/a", Method::GET, operable(explicit_first)), ("/b", Method::GET, operable(explicit_second))]);

    assert_eq!(operation_id(&spec, "/a", "get"), "shared");
    assert_eq!(operation_id(&spec, "/b", "get"), "shared_2");
}

#[test]
fn generated_suffixes_do_not_take_an_id_declared_by_another_operation() {
    // `/c` declares `list_2` explicitly. The duplicate at `/b` must not steal that name; it skips
    // to `list_3`, and the explicit id is never renamed.
    let spec = assemble(&[
        ("/a", Method::GET, operable(users::list)),
        ("/b", Method::GET, operable(books::list)),
        ("/c", Method::GET, operable(explicit_suffix)),
    ]);

    assert_eq!(operation_id(&spec, "/a", "get"), "list");
    assert_eq!(operation_id(&spec, "/b", "get"), "list_3");
    assert_eq!(operation_id(&spec, "/c", "get"), "list_2");
}

#[test]
fn already_unique_ids_are_untouched() {
    let spec = assemble(&[("/a", Method::GET, operable(users::list)), ("/c", Method::GET, operable(explicit_suffix))]);

    assert_eq!(operation_id(&spec, "/a", "get"), "list");
    assert_eq!(operation_id(&spec, "/c", "get"), "list_2");
}
