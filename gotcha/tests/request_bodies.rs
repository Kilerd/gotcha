#![cfg(feature = "openapi")]
#![allow(dead_code)]

use std::collections::HashMap;

use gotcha::axum::http::Method;
use gotcha::openapi::Binary;
use gotcha::{api, openapi::generate_openapi, Form, Json, Multipart, Operable, Path, Query, Schematic};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Schematic, Deserialize)]
struct JsonBody {
    name: String,
}

#[derive(Schematic, Deserialize)]
struct FormBody {
    name: String,
}

#[derive(Schematic, Deserialize)]
struct Filter {
    page: u32,
}

// Handlers without a request_body declaration keep their inferred documents; the test below
// compares the whole document against the shape produced before declarations existed.

#[api]
async fn json_only(_body: Json<JsonBody>) -> String {
    String::new()
}

#[api]
async fn form_only(_body: Form<FormBody>) -> String {
    String::new()
}

#[api]
async fn multipart_only(_body: Multipart) -> String {
    String::new()
}

#[api]
async fn path_query_json(_path: Path<u32>, _query: Query<Filter>, _body: Json<JsonBody>) -> String {
    String::new()
}

#[api]
async fn no_body() -> String {
    String::new()
}

/// A file upload with a title.
#[derive(Schematic)]
struct UploadForm {
    file: Binary,
    title: String,
}

#[derive(Schematic)]
struct DeclaredBody {
    message: String,
}

#[derive(Schematic, Deserialize)]
struct ReplacedBody {
    value: u32,
}

#[api(request_body(content_type = "multipart/form-data", body = "UploadForm", description = "File upload", required = true))]
async fn upload(_form: Multipart) -> String {
    String::new()
}

#[api(request_body(body = "DeclaredBody"))]
async fn replace_json(_path: Path<u32>, _query: Query<Filter>, _input: Json<ReplacedBody>) -> String {
    String::new()
}

#[api(
    request_body(content_type = "application/json", body = "DeclaredBody", description = "Payload", required = false),
    request_body(content_type = "multipart/form-data", body = "UploadForm", description = "Payload", required = false)
)]
async fn multi_content_type() -> String {
    String::new()
}

fn operable<H, T>(_: H) -> &'static Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().unwrap()
}

fn document(routes: HashMap<(String, Method), &'static Operable>) -> Value {
    serde_json::to_value(generate_openapi(routes)).unwrap()
}

fn one(operable: &'static Operable, path: &str) -> Value {
    document(HashMap::from([((path.to_owned(), Method::POST), operable)]))
}

#[test]
fn multipart_forms_document_their_fields() {
    let spec = one(operable(upload), "/upload");
    let body = &spec["paths"]["/upload"]["post"]["requestBody"];
    assert_eq!(body["description"], "File upload");
    assert_eq!(body["required"], true);
    assert_eq!(
        body["content"]["multipart/form-data"]["schema"]["$ref"], "#/components/schemas/UploadForm",
        "the declared form type is referenced as a component"
    );

    let form = &spec["components"]["schemas"]["UploadForm"];
    assert_eq!(
        form["properties"]["file"],
        json!({"type": "string", "contentMediaType": "application/octet-stream"}),
        "binary fields use the OpenAPI 3.2 contentMediaType form, not format: binary"
    );
    assert_eq!(form["properties"]["title"], json!({"type": "string"}));
    assert_eq!(form["required"], json!(["file", "title"]));
}

#[test]
fn a_declaration_replaces_the_body_inferred_from_extractors() {
    let spec = one(operable(replace_json), "/items/{id}");
    let operation = &spec["paths"]["/items/{id}"]["post"];
    assert_eq!(
        operation["requestBody"]["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/DeclaredBody",
        "the declared body wins over Json<ReplacedBody>"
    );
    assert_eq!(operation["requestBody"]["required"], true, "required defaults to true");
    assert!(operation["requestBody"].get("description").is_none(), "description is optional");
    // Parameters that do not come from the body are still documented.
    assert_eq!(
        operation["parameters"],
        json!([
            {"in": "path", "name": "id", "required": true, "schema": {"type": "integer"}},
            {"in": "query", "name": "page", "required": true, "schema": {"type": "integer"}}
        ])
    );
    let schemas = &spec["components"]["schemas"];
    assert!(schemas.get("DeclaredBody").is_some(), "the declared body is a component: {schemas}");
    assert!(
        schemas.get("ReplacedBody").is_none(),
        "the replaced extractor type must not be registered: {schemas}"
    );
}

#[test]
fn several_media_types_share_one_request_body() {
    let spec = one(operable(multi_content_type), "/multi");
    let body = &spec["paths"]["/multi"]["post"]["requestBody"];
    assert_eq!(body["description"], "Payload");
    assert_eq!(body["required"], false);
    assert_eq!(body["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/DeclaredBody");
    assert_eq!(body["content"]["multipart/form-data"]["schema"]["$ref"], "#/components/schemas/UploadForm");
}

#[test]
fn handlers_without_a_declaration_keep_their_inferred_documents() {
    let routes = HashMap::from([
        (("/json".to_owned(), Method::POST), operable(json_only)),
        (("/form".to_owned(), Method::POST), operable(form_only)),
        (("/upload".to_owned(), Method::POST), operable(multipart_only)),
        (("/items/{id}".to_owned(), Method::POST), operable(path_query_json)),
        (("/empty".to_owned(), Method::POST), operable(no_body)),
    ]);
    // Captured after automatic extractor rejection responses were added, so this snapshot also
    // pins those. Any other change to these handlers' documents fails the comparison; the
    // documents are compared semantically, as `serde_json::Value`s.
    let before: Value = serde_json::from_str(include_str!("fixtures/request_bodies_unchanged.json")).unwrap();
    assert_eq!(document(routes), before);
}

#[test]
fn malformed_request_body_declarations_do_not_compile() {
    trybuild::TestCases::new().compile_fail("tests/fail/request_bodies/*.rs");
}
