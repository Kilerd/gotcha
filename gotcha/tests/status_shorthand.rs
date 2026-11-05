#![cfg(feature = "openapi")]

use gotcha::axum::{
    body::Body,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use gotcha::{api, Gotcha, Json, Responsible, Schematic, WithStatus};
use serde::Serialize;
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Serialize, Schematic)]
struct Record {
    id: u32,
}

#[derive(Serialize, Schematic)]
struct Problem {
    message: String,
}

/// An error type that documents its body as a runtime status, like an application-wide error.
struct ApiError;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::NOT_FOUND, Json(Problem { message: "missing".into() })).into_response()
    }
}

impl Responsible for ApiError {
    fn response() -> gotcha::oas::Responses {
        gotcha::response::default_response::<Problem>("application/json", "error response")
    }
}

/// A success type that documents itself instead of relying on `Json` or `WithStatus`.
struct PlainRecord(Record);

impl IntoResponse for PlainRecord {
    fn into_response(self) -> Response {
        Json(self.0).into_response()
    }
}

impl Responsible for PlainRecord {
    const STATUSES: gotcha::response::Statuses = gotcha::response::Statuses::of(&[200]);

    fn response() -> gotcha::oas::Responses {
        gotcha::response::response::<Record>(200, "application/json", "plain record")
    }
}

// The same operation written both ways. The shorthand must produce the long form's document.
#[api(id = "read", responses(200 = "The stored record"))]
async fn read_shorthand() -> Json<Record> {
    Json(Record { id: 7 })
}

#[api(id = "read", responses(response(status = 200, body = "Record", description = "The stored record")))]
async fn read_long_form() -> Json<Record> {
    Json(Record { id: 7 })
}

#[api(id = "create", responses(201 = "Record stored"))]
async fn create_shorthand() -> WithStatus<Json<Record>, 201> {
    WithStatus::new(Json(Record { id: 7 }))
}

#[api(id = "create", responses(response(status = 201, body = "Record", description = "Record stored")))]
async fn create_long_form() -> WithStatus<Json<Record>, 201> {
    WithStatus::new(Json(Record { id: 7 }))
}

#[api(id = "lookup", errors(404 = "Missing record"))]
async fn lookup_shorthand() -> Result<Json<Record>, ApiError> {
    Err(ApiError)
}

#[api(id = "lookup", errors(response(status = 404, body = "Problem", description = "Missing record")))]
async fn lookup_long_form() -> Result<Json<Record>, ApiError> {
    Err(ApiError)
}

#[api(id = "mixed", responses(200 = "The stored record", response(status = 409, body = "Problem", description = "Duplicate")))]
async fn mixed_shorthand() -> Result<Json<Record>, ApiError> {
    Ok(Json(Record { id: 7 }))
}

#[api(
    id = "mixed",
    responses(
        response(status = 200, body = "Record", description = "The stored record"),
        response(status = 409, body = "Problem", description = "Duplicate")
    )
)]
async fn mixed_long_form() -> Result<Json<Record>, ApiError> {
    Ok(Json(Record { id: 7 }))
}

#[api(id = "plain", responses(200 = "The plain record"))]
async fn plain_shorthand() -> PlainRecord {
    PlainRecord(Record { id: 7 })
}

#[api(id = "plain", responses(response(status = 200, body = "Record", description = "The plain record")))]
async fn plain_long_form() -> PlainRecord {
    PlainRecord(Record { id: 7 })
}

#[api(
    id = "mixed_errors",
    errors(404 = "Missing record", response(status = 409, body = "Problem", description = "Duplicate record"))
)]
async fn mixed_errors_shorthand() -> Result<Json<Record>, ApiError> {
    Ok(Json(Record { id: 7 }))
}

#[api(
    id = "mixed_errors",
    errors(
        response(status = 404, body = "Problem", description = "Missing record"),
        response(status = 409, body = "Problem", description = "Duplicate record")
    )
)]
async fn mixed_errors_long_form() -> Result<Json<Record>, ApiError> {
    Ok(Json(Record { id: 7 }))
}

/// Both spellings of one operation, as a JSON value.
fn operation(shorthand: bool) -> Value {
    let document = if shorthand {
        Gotcha::new().get("/items", read_shorthand).into_openapi()
    } else {
        Gotcha::new().get("/items", read_long_form).into_openapi()
    };
    serde_json::to_value(document).unwrap()["paths"]["/items"]["get"].clone()
}

#[test]
fn a_description_only_override_matches_restating_the_body() {
    assert_eq!(operation(true), operation(false));
    assert_eq!(
        operation(true)["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Record"
    );

    let shorthand = Gotcha::new().post("/items", create_shorthand).into_openapi();
    let long = Gotcha::new().post("/items", create_long_form).into_openapi();
    let shorthand = serde_json::to_value(shorthand).unwrap();
    let long = serde_json::to_value(long).unwrap();
    assert_eq!(shorthand["paths"]["/items"]["post"], long["paths"]["/items"]["post"]);
    assert_eq!(shorthand["paths"]["/items"]["post"]["responses"]["201"]["description"], "Record stored");

    // A plain `Responsible` success type needs its `STATUSES` list; the rest is identical.
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", plain_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", plain_long_form).into_openapi()).unwrap();
    assert_eq!(shorthand["paths"]["/items"]["get"], long["paths"]["/items"]["get"]);
    assert_eq!(shorthand["paths"]["/items"]["get"]["responses"]["200"]["description"], "The plain record");
}

#[test]
fn an_error_shorthand_takes_the_body_the_error_type_documents() {
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", lookup_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", lookup_long_form).into_openapi()).unwrap();
    assert_eq!(shorthand["paths"]["/items"]["get"], long["paths"]["/items"]["get"]);
    let response = &shorthand["paths"]["/items"]["get"]["responses"]["404"];
    assert_eq!(response["description"], "Missing record");
    assert_eq!(response["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/Problem");
    // The error's runtime status is documented at the fixed status, not as a default.
    assert!(shorthand["paths"]["/items"]["get"]["responses"].get("default").is_none());
}

#[test]
fn shorthand_entries_mix_with_explicit_responses() {
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", mixed_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", mixed_long_form).into_openapi()).unwrap();
    let operation = &shorthand["paths"]["/items"]["get"];
    assert_eq!(operation, &long["paths"]["/items"]["get"]);
    assert_eq!(operation["responses"]["200"]["description"], "The stored record");
    assert_eq!(operation["responses"]["409"]["description"], "Duplicate");
    // Neither spelling declares the error branch, so the inferred default stays.
    assert_eq!(
        operation["responses"]["default"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Problem"
    );
}

#[test]
fn shorthand_entries_mix_with_explicit_errors() {
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", mixed_errors_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", mixed_errors_long_form).into_openapi()).unwrap();
    let operation = &shorthand["paths"]["/items"]["get"];
    assert_eq!(operation, &long["paths"]["/items"]["get"]);
    assert_eq!(operation["responses"]["404"]["description"], "Missing record");
    assert_eq!(operation["responses"]["409"]["description"], "Duplicate record");
}

#[tokio::test]
async fn the_shorthand_changes_the_document_and_not_the_handler() {
    let router = Gotcha::new()
        .get("/items", read_shorthand)
        .into_router(gotcha::GotchaContext::new(gotcha::EmptyState::default(), gotcha::ConfigWrapper::default()))
        .unwrap();
    let response = router.oneshot(Request::builder().uri("/items").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = gotcha::axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({"id": 7}));
}

#[test]
fn status_shorthand_failures_are_compile_errors() {
    trybuild::TestCases::new().compile_fail("tests/fail/status_shorthand/*.rs");
}
