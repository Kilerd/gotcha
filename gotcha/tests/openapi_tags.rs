#![cfg(feature = "openapi")]
#![allow(dead_code)]

use std::collections::HashMap;

use gotcha::axum::http::Method;
use gotcha::{api, openapi::generate_openapi, Operable};
use serde_json::{json, Value};

#[api(group = "users")]
async fn list_users() -> String {
    String::new()
}

#[api(group = "users")]
async fn get_user() -> String {
    String::new()
}

#[api(group = "orders")]
async fn list_orders() -> String {
    String::new()
}

#[api]
async fn health() -> String {
    String::new()
}

fn operable<H, T>(_: H) -> &'static Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().unwrap()
}

fn document(routes: &[(&str, &'static Operable)]) -> Value {
    let routes: HashMap<_, _> = routes.iter().map(|(path, operable)| ((path.to_string(), Method::GET), *operable)).collect();
    serde_json::to_value(generate_openapi(routes)).unwrap()
}

#[test]
fn document_declares_each_group_once_in_operation_order() {
    // Operations are ordered by path, so first-seen order is deterministic regardless of the
    // order the routes are registered in.
    let spec = document(&[
        ("/users/{id}", operable(get_user)),
        ("/orders", operable(list_orders)),
        ("/health", operable(health)),
        ("/users", operable(list_users)),
    ]);
    assert_eq!(spec["tags"], json!([{"name": "orders"}, {"name": "users"}]));
    for (path, group) in [("/orders", "orders"), ("/users", "users"), ("/users/{id}", "users")] {
        assert_eq!(spec["paths"][path]["get"]["tags"], json!([group]), "operation '{path}' keeps its tag");
    }
}

#[test]
fn document_without_grouped_operations_has_no_tags_array() {
    let spec = document(&[("/health", operable(health))]);
    assert!(spec.get("tags").is_none(), "an ungrouped document must not declare an empty tags array");
    assert!(spec["paths"]["/health"]["get"].get("tags").is_none());
}
