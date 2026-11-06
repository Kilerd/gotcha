#![cfg(feature = "openapi")]

use gotcha::axum::{
    body::Body,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use gotcha::{api, Gotcha, Json, Responsible, Schematic, WithStatus};
use serde::Serialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use tower::ServiceExt;

#[derive(Serialize, Schematic)]
struct Record {
    id: u32,
}

#[derive(Serialize, Schematic)]
struct Problem {
    message: String,
}

#[derive(Serialize, Schematic)]
struct ValidationProblem {
    field: String,
}

/// An error type that documents its body as a runtime status, like an application-wide error.
struct ApiError;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::NOT_FOUND, Json(Problem { message: "missing".into() })).into_response()
    }
}

impl Responsible for ApiError {
    // `response()` documents a `default` entry; the shorthand reads this to accept any status.
    const HAS_DEFAULT: bool = true;

    fn response() -> gotcha::oas::Responses {
        gotcha::response::default_response::<Problem>("application/json", "error response")
    }
}

/// An error type with two fixed statuses and a different body for each.
struct MultiError;

impl IntoResponse for MultiError {
    fn into_response(self) -> Response {
        (StatusCode::NOT_FOUND, Json(Problem { message: "missing".into() })).into_response()
    }
}

impl Responsible for MultiError {
    const STATUSES: gotcha::response::Statuses = gotcha::response::Statuses::of(&[404, 422]);

    fn response() -> gotcha::oas::Responses {
        let mut responses = gotcha::response::response::<Problem>(404, "application/json", "missing");
        gotcha::response::merge_responses(
            &mut responses,
            gotcha::response::response::<ValidationProblem>(422, "application/json", "invalid"),
        );
        responses
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

#[api(id = "validate", errors(404 = "Missing record"))]
async fn multi_error_shorthand() -> Result<Json<Record>, MultiError> {
    Ok(Json(Record { id: 7 }))
}

#[api(id = "validate", errors(response(status = 404, body = "Problem", description = "Missing record")))]
async fn multi_error_long_form() -> Result<Json<Record>, MultiError> {
    Ok(Json(Record { id: 7 }))
}

// The error list runs first, so the `responses` shorthand may reword a status it declares.
#[api(id = "conflict", errors(409 = "Duplicate record"), responses(409 = "Already exists"))]
async fn error_then_response_shorthand() -> Result<Json<Record>, ApiError> {
    Ok(Json(Record { id: 7 }))
}

#[api(
    id = "conflict",
    errors(response(status = 409, body = "Problem", description = "Duplicate record")),
    responses(response(status = 409, body = "Problem", description = "Already exists"))
)]
async fn error_then_response_long_form() -> Result<Json<Record>, ApiError> {
    Ok(Json(Record { id: 7 }))
}

#[api(id = "no_content", responses(204 = "No content"))]
async fn no_content_shorthand() -> WithStatus<Json<Record>, 204> {
    WithStatus::new(Json(Record { id: 7 }))
}

#[api(id = "no_content", responses(response(status = 204, description = "No content")))]
async fn no_content_long_form() -> WithStatus<Json<Record>, 204> {
    WithStatus::new(Json(Record { id: 7 }))
}

// `macro_rules!` passes substituted fragments inside invisible (`None`-delimited) groups, which
// the shorthand parser has to look through.
macro_rules! expr_status {
    ($status:expr, $description:expr) => {
        #[api(id = "fragment_expr", responses($status = $description))]
        async fn expr_status_handler() -> Json<Record> {
            Json(Record { id: 7 })
        }
    };
}
expr_status!(200, "Stored");

macro_rules! literal_response_status {
    ($status:literal, $description:expr) => {
        #[api(id = "fragment_literal_response", responses($status = $description))]
        async fn literal_response_status_handler() -> WithStatus<Json<Record>, 201> {
            WithStatus::new(Json(Record { id: 7 }))
        }
    };
}
literal_response_status!(201, "Created");

macro_rules! literal_status {
    ($status:literal, $description:expr) => {
        #[api(id = "fragment_literal", errors($status = $description))]
        async fn literal_status_handler() -> Result<Json<Record>, ApiError> {
            Ok(Json(Record { id: 7 }))
        }
    };
}
literal_status!(404, "Missing record");

macro_rules! expr_description {
    ($status:expr, $description:expr) => {
        #[api(id = "fragment_description", errors($status = $description))]
        async fn expr_description_handler() -> Result<Json<Record>, ApiError> {
            Ok(Json(Record { id: 7 }))
        }
    };
}
expr_description!(409, "Duplicate record");

/// Both spellings of the `read` operation as a whole document, components included.
fn document(shorthand: bool) -> Value {
    let document = if shorthand {
        Gotcha::new().get("/items", read_shorthand).into_openapi()
    } else {
        Gotcha::new().get("/items", read_long_form).into_openapi()
    };
    serde_json::to_value(document).unwrap()
}

#[test]
fn a_description_only_override_matches_restating_the_body() {
    // Compare the whole document, components included: the shorthand must emit neither more
    // nor fewer schemas than the long form.
    assert_eq!(document(true), document(false));
    assert_eq!(
        document(true)["paths"]["/items"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Record"
    );

    let shorthand = serde_json::to_value(Gotcha::new().post("/items", create_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().post("/items", create_long_form).into_openapi()).unwrap();
    assert_eq!(shorthand, long);
    assert_eq!(shorthand["paths"]["/items"]["post"]["responses"]["201"]["description"], "Record stored");

    // A plain `Responsible` success type needs its `STATUSES` list; the rest is identical.
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", plain_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", plain_long_form).into_openapi()).unwrap();
    assert_eq!(shorthand, long);
    assert_eq!(shorthand["paths"]["/items"]["get"]["responses"]["200"]["description"], "The plain record");
}

#[test]
fn an_error_shorthand_takes_the_body_the_error_type_documents() {
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", lookup_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", lookup_long_form).into_openapi()).unwrap();
    assert_eq!(shorthand, long);
    let response = &shorthand["paths"]["/items"]["get"]["responses"]["404"];
    assert_eq!(response["description"], "Missing record");
    assert_eq!(response["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/Problem");
    // The error's runtime status is documented at the fixed status, not as a default.
    assert!(shorthand["paths"]["/items"]["get"]["responses"].get("default").is_none());
}

#[test]
fn an_error_shorthand_registers_only_the_components_its_entry_references() {
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", multi_error_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", multi_error_long_form).into_openapi()).unwrap();
    // `MultiError::response()` also documents a 422 with `ValidationProblem`; the 404 entry
    // does not reference it, so the shorthand must not add it to `components.schemas`.
    assert_eq!(shorthand, long);
    let schemas = shorthand["components"]["schemas"].as_object().expect("components with schemas");
    assert!(schemas.contains_key("Problem"), "{schemas:?}");
    assert!(schemas.contains_key("Record"), "{schemas:?}");
    assert!(!schemas.contains_key("ValidationProblem"), "{schemas:?}");
}

#[test]
fn shorthand_entries_mix_with_explicit_responses() {
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", mixed_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", mixed_long_form).into_openapi()).unwrap();
    assert_eq!(shorthand, long);
    let operation = &shorthand["paths"]["/items"]["get"];
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
    assert_eq!(shorthand, long);
    let operation = &shorthand["paths"]["/items"]["get"];
    assert_eq!(operation["responses"]["404"]["description"], "Missing record");
    assert_eq!(operation["responses"]["409"]["description"], "Duplicate record");
}

#[test]
fn a_response_shorthand_may_reword_a_status_the_error_list_declares() {
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", error_then_response_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", error_then_response_long_form).into_openapi()).unwrap();
    assert_eq!(shorthand, long);
    assert_eq!(shorthand["paths"]["/items"]["get"]["responses"]["409"]["description"], "Already exists");
}

#[test]
fn a_bodyless_status_is_reworded_without_gaining_content() {
    let shorthand = serde_json::to_value(Gotcha::new().get("/items", no_content_shorthand).into_openapi()).unwrap();
    let long = serde_json::to_value(Gotcha::new().get("/items", no_content_long_form).into_openapi()).unwrap();
    assert_eq!(shorthand, long);
    let response = &shorthand["paths"]["/items"]["get"]["responses"]["204"];
    assert_eq!(response["description"], "No content");
    assert!(response.get("content").is_none(), "{response:?}");
}

#[test]
fn with_status_declares_only_the_statuses_it_documents() {
    assert!(<WithStatus<Json<Record>, 201> as Responsible>::STATUSES.contains(201));
    assert!(<WithStatus<Infallible, 204> as Responsible>::STATUSES.contains(204));
    assert!(<WithStatus<Infallible, 201> as Responsible>::STATUSES.is_empty());
    // A runtime status around an empty inner contract documents nothing either.
    assert!(!<(StatusCode, Infallible) as Responsible>::HAS_DEFAULT);
    assert!(<WithStatus<(StatusCode, Infallible), 201> as Responsible>::STATUSES.is_empty());
}

#[test]
fn macro_fragments_expand_the_shorthand() {
    let document = serde_json::to_value(
        Gotcha::new()
            .get("/expr", expr_status_handler)
            .get("/literal-response", literal_response_status_handler)
            .get("/literal", literal_status_handler)
            .get("/description", expr_description_handler)
            .into_openapi(),
    )
    .unwrap();
    assert_eq!(document["paths"]["/expr"]["get"]["responses"]["200"]["description"], "Stored");
    assert_eq!(document["paths"]["/literal-response"]["get"]["responses"]["201"]["description"], "Created");
    assert_eq!(document["paths"]["/literal"]["get"]["responses"]["404"]["description"], "Missing record");
    assert_eq!(document["paths"]["/description"]["get"]["responses"]["409"]["description"], "Duplicate record");
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
