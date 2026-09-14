#![cfg(feature = "openapi")]
#![allow(dead_code)]

use gotcha::axum::http::{Method, StatusCode};
use gotcha::{
    api,
    openapi::{generate_openapi, ParamConstructor},
    Json, Operable, ParameterProvider, Path, Query, Schematic,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Schematic, Serialize, Deserialize)]
struct Envelope<T: Schematic> {
    data: T,
}

#[derive(Schematic, Serialize, Deserialize)]
struct Node {
    label: String,
    children: Vec<Node>,
}

#[derive(Schematic, Deserialize)]
struct Filter {
    limit: Option<u32>,
}

// Deliberately has neither ParameterProvider nor Responsible.
struct Opaque;

/// Replace a tree and describe its accepted result.
#[api(
    id = "replace_tree",
    group = "trees",
    summary = "Replace a tree",
    deprecated,
    security = "token",
    errors(response(status = 409, body = "Envelope<String>")),
    responses(
        response(status = 202, body = "Envelope<Node>"),
        response(status = 409, body = "Envelope<String>", description = "Tree conflict")
    ),
    drop_default
)]
async fn replace_tree(
    _path: Path<u64>, _query: Query<Filter>, #[api(skip)] _opaque: Opaque, _body: Json<Envelope<Node>>,
) -> Result<(StatusCode, Json<Envelope<Node>>), Opaque> {
    unimplemented!()
}

// Static public descriptors need no generated helper variables or lazy initialization.
static PARAMETERS: &[ParamConstructor] = &[
    <Path<u64> as ParameterProvider>::generate,
    <Query<Filter> as ParameterProvider>::generate,
    <Json<Envelope<Node>> as ParameterProvider>::generate,
];
static MANUAL: Operable = Operable {
    type_name: concat!(module_path!(), "::replace_tree"),
    id: "replace_tree",
    group: Some("trees"),
    summary: Some("Replace a tree"),
    description: Some("Replace a tree and describe its accepted result."),
    deprecated: true,
    security: Some("token"),
    parameters: PARAMETERS,
    responses: || {
        let mut responses = gotcha::response::response::<Envelope<Node>>(202, "application/json", "HTTP 202");
        gotcha::response::merge_responses(
            &mut responses,
            gotcha::response::response::<Envelope<String>>(409, "application/json", "Tree conflict"),
        );
        responses
    },
};

fn descriptor() -> &'static Operable {
    gotcha::inventory::iter::<Operable>.into_iter().find(|op| op.id == "replace_tree").unwrap()
}

fn document(op: &'static Operable, path: &str) -> Value {
    let routes = HashMap::from([((path.to_owned(), Method::POST), op)]);
    serde_json::to_value(generate_openapi(routes)).unwrap()
}

#[test]
fn descriptors_build_fresh_path_specific_documents() {
    let op = descriptor();
    let first = document(op, "/trees/{tree_id}");
    assert_eq!(first, document(&MANUAL, "/trees/{tree_id}"), "macro and static public descriptors must agree");
    let operation = &first["paths"]["/trees/{tree_id}"]["post"];
    assert_eq!(operation["parameters"][0]["name"], "tree_id");
    assert_eq!(operation["parameters"][1]["name"], "limit");
    assert_eq!(
        operation["parameters"].as_array().unwrap().len(),
        2,
        "skipped arguments must have no constructor"
    );
    assert_eq!(
        operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Envelope_Node"
    );
    assert_eq!(
        operation["responses"].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(),
        ["202", "409"]
    );
    assert_eq!(operation["responses"]["409"]["description"], "Tree conflict");
    let schemas = &first["components"]["schemas"];
    assert_eq!(schemas["Node"]["properties"]["children"]["items"]["$ref"], "#/components/schemas/Node");
    assert_eq!(schemas["Envelope_String"]["properties"]["data"]["type"], "string");
    assert_eq!(schemas["Envelope_Node"]["properties"]["data"]["$ref"], "#/components/schemas/Node");
    let second = document(op, "/archives/{archive_id}");
    assert_eq!(second["paths"]["/archives/{archive_id}"]["post"]["parameters"][0]["name"], "archive_id");
    assert_eq!(second["components"], first["components"], "each collection must register its own schemas");
    assert_eq!(document(op, "/trees/{tree_id}"), first);
}
