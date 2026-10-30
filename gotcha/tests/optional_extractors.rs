#![cfg(feature = "openapi")]
#![allow(dead_code)]

use std::collections::HashMap;

use either::Either;
use gotcha::axum::http::Method;
use gotcha::{api, openapi::generate_openapi, Json, Operable, ParameterProvider, Path, Query, Schematic};
use oas::ParameterIn;
use serde_json::Value;

#[derive(Schematic, serde::Deserialize)]
struct Payload {
    name: String,
}

#[derive(Schematic)]
struct Filter {
    limit: u32,
}

#[derive(Schematic)]
struct ConnectionPath {
    user_id: String,
    connection_id: Option<String>,
}

#[api]
async fn maybe_create(_body: Option<Json<Payload>>) -> String {
    String::new()
}

fn operable<H, T>(_: H) -> &'static Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().unwrap()
}

#[test]
fn optional_json_body_is_documented_as_not_required() {
    let routes = HashMap::from([(("/items".to_owned(), Method::POST), operable(maybe_create))]);
    let spec: Value = serde_json::to_value(generate_openapi(routes)).unwrap();
    let body = &spec["paths"]["/items"]["post"]["requestBody"];
    assert_eq!(body["required"], false, "an Option<Json<T>> body is optional");
    assert_eq!(
        body["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/Payload",
        "the body schema is still documented"
    );
}

#[test]
fn optional_multipart_body_is_documented_as_not_required() {
    match <Option<gotcha::axum::extract::multipart::Multipart> as ParameterProvider>::generate("/upload".to_owned()) {
        Either::Right(body) => assert_eq!(body.required, Some(false)),
        Either::Left(_) => panic!("Multipart should generate a request body, not parameters"),
    }
}

#[test]
fn optional_path_extractor_keeps_path_parameters_required() {
    match <Option<Path<u32>> as ParameterProvider>::generate("/users/{user_id}".to_owned()) {
        Either::Left(params) => {
            assert_eq!(params.len(), 1);
            assert!(matches!(&params[0]._in, ParameterIn::Path));
            assert_eq!(params[0].required, Some(true), "OpenAPI requires every path parameter to be required");
        }
        Either::Right(_) => panic!("Path<u32> should generate parameters, not a request body"),
    }
}

#[test]
fn path_struct_option_field_stays_required() {
    match <Path<ConnectionPath> as ParameterProvider>::generate("/users/{user_id}/connections/{connection_id}".to_owned()) {
        Either::Left(params) => {
            assert_eq!(params.iter().map(|param| param.name.as_str()).collect::<Vec<_>>(), ["user_id", "connection_id"]);
            for param in &params {
                assert!(matches!(&param._in, ParameterIn::Path));
                assert_eq!(param.required, Some(true), "path parameter '{}' must be required", param.name);
            }
        }
        Either::Right(_) => panic!("Path<ConnectionPath> should generate parameters, not a request body"),
    }
}

#[test]
fn optional_query_extractor_still_relaxes_query_parameters() {
    match <Option<Query<Filter>> as ParameterProvider>::generate("/items".to_owned()) {
        Either::Left(params) => {
            assert_eq!(params.len(), 1);
            assert!(matches!(&params[0]._in, ParameterIn::Query));
            assert_eq!(params[0].required, Some(false), "query parameters may be omitted");
        }
        Either::Right(_) => panic!("Query<Filter> should generate parameters, not a request body"),
    }
}
