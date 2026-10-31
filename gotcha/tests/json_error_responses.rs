//! Per-extractor rejection defaults, overrides, and OpenAPI components.

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{Request, StatusCode};
use gotcha::axum::response::IntoResponse;
use gotcha::{
    ConfigWrapper, Cookie, CookieParam, EmptyConfig, ExtractorConfigs, Form, GotchaApp, GotchaContext, GotchaError, GotchaResult, GotchaRouter, Header,
    HeaderParam, HttpError, Json, JsonConfig, Multipart, Path, Query, Rejected, TypedHeader, Valid, ValidConfig, Validate,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Debug, Deserialize, Serialize)]
struct Item {
    id: u32,
}

#[derive(Debug, Deserialize)]
struct Search {
    page: u32,
}

#[derive(Debug, Deserialize)]
struct FormPayload {
    name: String,
}

#[derive(Debug, Deserialize, Serialize, gotcha::Validate)]
#[validate(crate = "gotcha::validator")]
struct Validated {
    #[validate(length(min = 1))]
    name: String,
}

#[derive(Debug, Deserialize, gotcha::Validate)]
#[validate(crate = "gotcha::validator")]
struct Address {
    #[validate(length(min = 1, max = 64))]
    city: String,
}

#[derive(Debug, Deserialize, gotcha::Validate)]
#[validate(crate = "gotcha::validator")]
struct LineItem {
    #[validate(length(min = 1))]
    name: String,
}

#[derive(Debug, Deserialize, gotcha::Validate)]
#[validate(crate = "gotcha::validator", schema(function = "at_least_one_item"))]
struct Order {
    #[validate(nested)]
    address: Address,
    #[validate(nested)]
    items: Vec<LineItem>,
}

fn at_least_one_item(order: &Order) -> Result<(), gotcha::validator::ValidationError> {
    if order.items.is_empty() {
        Err(gotcha::validator::ValidationError::new("required").with_message("at least one item is required".into()))
    } else {
        Ok(())
    }
}

struct RequestId(String);

impl HeaderParam for RequestId {
    const NAME: &'static str = "x-request-id";

    fn parse(raw: &str) -> Result<Self, String> {
        Ok(Self(raw.to_string()))
    }
}

struct Session(String);

impl CookieParam for Session {
    const NAME: &'static str = "session";

    fn parse(raw: &str) -> Result<Self, String> {
        Ok(Self(raw.to_string()))
    }
}

async fn echo_json(Json(item): Json<Item>) -> Json<Item> {
    Json(item)
}

async fn read_item(Path(id): Path<u32>) -> String {
    id.to_string()
}

async fn search(Query(query): Query<Search>) -> String {
    query.page.to_string()
}

async fn form(Form(payload): Form<FormPayload>) -> String {
    payload.name
}

async fn upload(_multipart: Multipart) -> String {
    "uploaded".to_string()
}

async fn request_id(Header(id): Header<RequestId>) -> String {
    id.0
}

async fn session(Cookie(session): Cookie<Session>) -> String {
    session.0
}

async fn host(TypedHeader(host): TypedHeader<gotcha::headers::Host>) -> String {
    host.to_string()
}

async fn validate(Valid(Json(payload)): Valid<Json<Validated>>) -> Json<Validated> {
    Json(payload)
}

async fn validate_order(Valid(Json(order)): Valid<Json<Order>>) -> String {
    format!("{}:{}", order.address.city, order.items.len())
}

async fn validate_list(Valid(Json(items)): Valid<Json<Vec<Validated>>>) -> String {
    items.len().to_string()
}

async fn validate_nested_list(Valid(Json(items)): Valid<Json<Vec<Vec<Validated>>>>) -> String {
    items.len().to_string()
}

async fn optional(body: Option<Json<Item>>) -> String {
    format!("{:?}", body.map(|Json(item)| item.id))
}

async fn optional_path(id: Option<Path<u32>>) -> String {
    format!("{:?}", id.map(|Path(id)| id))
}

/// Axum's documented per-handler pattern, with the original rejection still available.
async fn per_handler(body: Result<Json<Item>, Rejected<gotcha::axum::extract::rejection::JsonRejection>>) -> String {
    match body {
        Ok(Json(item)) => format!("ok:{}", item.id),
        Err(rejected) => format!("rejected:{}", rejected.rejection().body_text()),
    }
}

type Context = GotchaContext<(), EmptyConfig>;

fn context() -> Context {
    GotchaContext::new((), ConfigWrapper::default())
}

struct App;
impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        router
            .post("/json", echo_json)
            .get("/items/{id}", read_item)
            .get("/search", search)
            .post("/form", form)
            .post("/multipart", upload)
            .get("/header", request_id)
            .get("/cookie", session)
            .get("/typed-header", host)
            .post("/valid", validate)
            .post("/valid-order", validate_order)
            .post("/valid-list", validate_list)
            .post("/valid-nested-list", validate_nested_list)
            .post("/optional", optional)
            .get("/optional-path", optional_path)
            .post("/per-handler", per_handler)
    }
}

async fn app() -> gotcha::axum::Router {
    App.build_router(context()).await.unwrap()
}

/// Status, media type, and raw body of a request run through `router`.
async fn send(router: &gotcha::axum::Router, request: Request<Body>) -> (StatusCode, String, Vec<u8>) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .map(|value| value.to_str().unwrap().split(';').next().unwrap().to_string())
        .unwrap_or_default();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec();
    (status, content_type, body)
}

fn request(method: &str, uri: &str, content_type: Option<&str>, body: &str) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    builder.body(Body::from(body.to_owned())).unwrap()
}

fn assert_json_error(response: (StatusCode, String, Vec<u8>), expected_status: StatusCode) {
    let (status, content_type, body) = response;
    assert_eq!(status, expected_status);
    assert_eq!(content_type, "application/json");
    let body: Value = serde_json::from_slice(&body).expect("the default rejection body is JSON");
    assert!(body["error"].as_str().is_some_and(|message| !message.is_empty()), "{body}");
}

#[tokio::test]
async fn json_defaults_keep_axums_status_with_a_json_body() {
    let router = app().await;
    // Malformed JSON keeps Axum's 400.
    assert_json_error(
        send(&router, request("POST", "/json", Some("application/json"), "not json")).await,
        StatusCode::BAD_REQUEST,
    );
    // A missing content type keeps Axum's 415.
    assert_json_error(
        send(&router, request("POST", "/json", None, r#"{"id":1}"#)).await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
    );
    // A shape mismatch keeps Axum's 422.
    assert_json_error(
        send(&router, request("POST", "/json", Some("application/json"), r#"{"id":"one"}"#)).await,
        StatusCode::UNPROCESSABLE_ENTITY,
    );
}

#[tokio::test]
async fn other_provider_defaults_keep_axums_status_with_a_json_body() {
    let router = app().await;
    assert_json_error(send(&router, request("GET", "/items/not-a-number", None, "")).await, StatusCode::BAD_REQUEST);
    assert_json_error(
        send(&router, request("GET", "/search?page=not-a-number", None, "")).await,
        StatusCode::BAD_REQUEST,
    );
    // Form: missing content type is 415, a shape mismatch is 422.
    assert_json_error(
        send(&router, request("POST", "/form", None, "name=ada")).await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
    );
    assert_json_error(
        send(&router, request("POST", "/form", Some("application/x-www-form-urlencoded"), "other=1")).await,
        StatusCode::UNPROCESSABLE_ENTITY,
    );
    // Multipart without a boundary is 400.
    assert_json_error(send(&router, request("POST", "/multipart", None, "")).await, StatusCode::BAD_REQUEST);
    // TypedHeader, Header, and Cookie are 400.
    assert_json_error(send(&router, request("GET", "/typed-header", None, "")).await, StatusCode::BAD_REQUEST);
    assert_json_error(send(&router, request("GET", "/header", None, "")).await, StatusCode::BAD_REQUEST);
    assert_json_error(send(&router, request("GET", "/cookie", None, "")).await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn valid_default_is_422_with_the_error_body() {
    let router = app().await;

    // A malformed body is a Json rejection: Axum's status with the JSON error body.
    assert_json_error(
        send(&router, request("POST", "/valid", Some("application/json"), "not json")).await,
        StatusCode::BAD_REQUEST,
    );

    // A validation failure uses the same ErrorBody shape as every other provider.
    let (status, content_type, body) = send(&router, request("POST", "/valid", Some("application/json"), r#"{"name":""}"#)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(content_type, "application/json");
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error": "validation failed", "fields": {"name": ["length must be at least 1"]}})
    );
}

#[tokio::test]
async fn valid_default_flattens_nested_list_and_struct_level_errors() {
    let router = app().await;

    let (status, content_type, body) = send(
        &router,
        request(
            "POST",
            "/valid-order",
            Some("application/json"),
            r#"{"address":{"city":""},"items":[{"name":""},{"name":"ok"}]}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(content_type, "application/json");
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({
            "error": "validation failed",
            "fields": {
                "address.city": ["length must be at least 1 and at most 64"],
                "items[0].name": ["length must be at least 1"]
            }
        })
    );

    // Struct-level (schema) validations keep validator's `__all__` key.
    let (status, _, body) = send(
        &router,
        request("POST", "/valid-order", Some("application/json"), r#"{"address":{"city":"Paris"},"items":[]}"#),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error": "validation failed", "fields": {"__all__": ["at least one item is required"]}})
    );
}

#[tokio::test]
async fn valid_default_drops_the_internal_key_for_top_level_collections() {
    let router = app().await;

    let (status, content_type, body) = send(&router, request("POST", "/valid-list", Some("application/json"), r#"[{"name":""}]"#)).await;
    assert_eq!((status, content_type.as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "application/json"));
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error": "validation failed", "fields": {"[0].name": ["length must be at least 1"]}})
    );

    let (status, _, body) = send(
        &router,
        request("POST", "/valid-nested-list", Some("application/json"), r#"[[{"name":"ok"}],[{"name":""}]]"#),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error": "validation failed", "fields": {"[1][0].name": ["length must be at least 1"]}})
    );
}

#[tokio::test]
async fn successful_requests_are_unchanged() {
    let router = app().await;
    let (status, content_type, body) = send(&router, request("POST", "/json", Some("application/json"), r#"{"id":7}"#)).await;
    assert_eq!((status, content_type.as_str()), (StatusCode::OK, "application/json"));
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({"id": 7}));
    assert_eq!(send(&router, request("GET", "/items/7", None, "")).await.2, b"7");
    assert_eq!(send(&router, request("GET", "/search?page=3", None, "")).await.2, b"3");
    assert_eq!(
        send(&router, request("POST", "/form", Some("application/x-www-form-urlencoded"), "name=ada"))
            .await
            .2,
        b"ada"
    );
    assert_eq!(send(&router, request("GET", "/typed-header", None, "")).await.0, StatusCode::BAD_REQUEST);
    let host = Request::builder()
        .uri("/typed-header")
        .header("host", "example.com")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&router, host).await.2, b"example.com");
    let header = Request::builder().uri("/header").header("x-request-id", "abc").body(Body::empty()).unwrap();
    assert_eq!(send(&router, header).await.2, b"abc");
    let cookie = Request::builder().uri("/cookie").header("cookie", "session=xyz").body(Body::empty()).unwrap();
    assert_eq!(send(&router, cookie).await.2, b"xyz");
}

#[tokio::test]
async fn optional_extractors_still_extract_none() {
    let router = app().await;
    assert_eq!(send(&router, request("POST", "/optional", None, "")).await.2, b"None");
    assert_eq!(send(&router, request("GET", "/optional-path", None, "")).await.2, b"None");
}

#[tokio::test]
async fn result_handlers_can_still_match_the_original_rejection() {
    let router = app().await;
    let (status, _, body) = send(&router, request("POST", "/per-handler", Some("application/json"), r#"{"id":7}"#)).await;
    assert_eq!((status, body.as_slice()), (StatusCode::OK, b"ok:7".as_slice()));

    let (status, _, body) = send(&router, request("POST", "/per-handler", Some("application/json"), "not json")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).starts_with("rejected:"));
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(gotcha::Schematic))]
struct JsonProblem {
    code: u32,
    detail: String,
}

struct CustomApp;
impl GotchaApp for CustomApp {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn extractor_configs(&self) -> ExtractorConfigs {
        ExtractorConfigs::default()
            .set(JsonConfig::default().error_handler(|rejection| {
                (
                    StatusCode::BAD_REQUEST,
                    JsonProblem {
                        code: 7,
                        detail: rejection.body_text(),
                    },
                )
            }))
            .set(ValidConfig::default().error_handler(|errors| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    JsonProblem {
                        code: errors.field_errors().len() as u32,
                        detail: "validation".to_string(),
                    },
                )
            }))
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        // A nested router still sees the application's extractor configs.
        router.nest(
            "/api",
            GotchaRouter::default()
                .post("/json", echo_json)
                .get("/items/{id}", read_item)
                .post("/valid", validate),
        )
    }
}

#[tokio::test]
async fn overriding_one_provider_changes_only_that_provider() {
    let router = CustomApp.build_router(context()).await.unwrap();

    let (status, content_type, body) = send(&router, request("POST", "/api/json", Some("application/json"), "not json")).await;
    assert_eq!((status, content_type.as_str()), (StatusCode::BAD_REQUEST, "application/json"));
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], 7);
    assert!(body["detail"].as_str().unwrap().contains("JSON"));

    // The Path provider still uses its default.
    assert_json_error(
        send(&router, request("GET", "/api/items/not-a-number", None, "")).await,
        StatusCode::BAD_REQUEST,
    );

    // The Valid provider is configured independently for validation failures.
    let (status, content_type, body) = send(&router, request("POST", "/api/valid", Some("application/json"), r#"{"name":""}"#)).await;
    assert_eq!((status, content_type.as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "application/json"));
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({"code": 1, "detail": "validation"}));

    // Success is unaffected.
    let (status, _, body) = send(&router, request("POST", "/api/json", Some("application/json"), r#"{"id":7}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({"id": 7}));
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(gotcha::Schematic))]
struct FlattenedProblem {
    error: String,
    fields: std::collections::BTreeMap<String, Vec<String>>,
}

struct FlatteningApp;

impl GotchaApp for FlatteningApp {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn extractor_configs(&self) -> ExtractorConfigs {
        ExtractorConfigs::default().set(ValidConfig::default().error_handler(|errors: &gotcha::validator::ValidationErrors| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                FlattenedProblem {
                    error: "custom validation".to_string(),
                    fields: gotcha::validation::flatten_errors(errors),
                },
            )
        }))
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        router.post("/valid-order", validate_order)
    }
}

#[tokio::test]
async fn valid_override_receives_the_validation_errors_and_can_flatten_them() {
    let router = FlatteningApp.build_router(context()).await.unwrap();
    let (status, content_type, body) = send(
        &router,
        request(
            "POST",
            "/valid-order",
            Some("application/json"),
            r#"{"address":{"city":""},"items":[{"name":""}]}"#,
        ),
    )
    .await;
    assert_eq!((status, content_type.as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "application/json"));
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({
            "error": "custom validation",
            "fields": {
                "address.city": ["length must be at least 1 and at most 64"],
                "items[0].name": ["length must be at least 1"]
            }
        })
    );
}

#[cfg(feature = "openapi")]
#[test]
fn custom_error_bodies_are_openapi_components() {
    // Builder API: the configured body is registered even without a route declaring it.
    let document = gotcha::Gotcha::new()
        .extractor_config(JsonConfig::default().error_handler(|rejection| {
            (
                StatusCode::BAD_REQUEST,
                JsonProblem {
                    code: 7,
                    detail: rejection.body_text(),
                },
            )
        }))
        .into_openapi();
    let document = serde_json::to_value(document).unwrap();
    let problem = &document["components"]["schemas"]["JsonProblem"];
    assert_eq!(problem["type"], "object");
    assert_eq!(problem["properties"]["code"]["type"], "integer");
    assert_eq!(problem["properties"]["detail"]["type"], "string");

    // Trait API: the same configs feed document generation.
    let document = serde_json::to_value(CustomApp.openapi_document()).unwrap();
    assert!(document["components"]["schemas"]["JsonProblem"].is_object());
}

#[tokio::test]
async fn extractors_outside_assembly_fall_back_to_defaults() {
    let router = gotcha::axum::Router::new().route("/echo", gotcha::axum::routing::post(echo_json));
    let response = router.oneshot(request("POST", "/echo", Some("application/json"), "not json")).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(serde_json::from_slice::<Value>(&body).unwrap()["error"].is_string());
}

#[tokio::test]
async fn http_error_and_gotcha_error_render_json_bodies() {
    for (error, status) in [
        (HttpError::bad_request("bad"), StatusCode::BAD_REQUEST),
        (HttpError::not_found("missing"), StatusCode::NOT_FOUND),
        (HttpError::conflict("duplicate"), StatusCode::CONFLICT),
        (HttpError::internal("failed"), StatusCode::INTERNAL_SERVER_ERROR),
    ] {
        let response = error.clone().into_response();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["content-type"], "application/json");
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({"error": error.message}));
    }

    let response = GotchaError::message("database is at /var/secret").into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!({"error": "internal server error"}));
    assert!(!String::from_utf8_lossy(&body).contains("/var/secret"));
}

#[cfg(feature = "openapi")]
mod documents {
    use gotcha::{
        api,
        axum::extract::{Form as AxumForm, Multipart as AxumMultipart},
        axum_extra::TypedHeader as AxumTypedHeader,
        Schematic,
    };

    use super::*;

    #[derive(Debug, Deserialize, Serialize, Schematic)]
    struct Payload {
        id: u32,
    }

    #[derive(Debug, Deserialize, Schematic)]
    struct Filter {
        page: u32,
    }

    /// Read a payload.
    #[api(id = "read_payload")]
    async fn read_gotcha(Path(id): Path<u32>, Query(filter): Query<Filter>) -> Json<Payload> {
        Json(Payload { id: id + filter.page })
    }

    /// Read a payload.
    #[api(id = "read_payload")]
    async fn read_axum(
        gotcha::axum::extract::Path(id): gotcha::axum::extract::Path<u32>, gotcha::axum::extract::Query(filter): gotcha::axum::extract::Query<Filter>,
    ) -> gotcha::axum::Json<Payload> {
        gotcha::axum::Json(Payload { id: id + filter.page })
    }

    /// Write a payload.
    #[api(id = "write_payload")]
    async fn write_gotcha(Json(payload): Json<Payload>) -> Json<Payload> {
        Json(payload)
    }

    /// Write a payload.
    #[api(id = "write_payload")]
    async fn write_axum(gotcha::axum::extract::Json(payload): gotcha::axum::extract::Json<Payload>) -> gotcha::axum::Json<Payload> {
        gotcha::axum::Json(payload)
    }

    /// Fail with the framework error body.
    #[api]
    async fn failing() -> Result<Json<Payload>, HttpError> {
        Err(HttpError::bad_request("bad"))
    }

    /// Submit a form.
    #[api(id = "submit_form")]
    async fn form_gotcha(Form(payload): Form<Filter>) -> String {
        payload.page.to_string()
    }

    /// Submit a form.
    #[api(id = "submit_form")]
    async fn form_axum(AxumForm(payload): AxumForm<Filter>) -> String {
        payload.page.to_string()
    }

    /// Upload a body.
    #[api(id = "upload")]
    async fn upload_gotcha(_body: Multipart) -> String {
        "uploaded".to_string()
    }

    /// Upload a body.
    #[api(id = "upload")]
    async fn upload_axum(_body: AxumMultipart) -> String {
        "uploaded".to_string()
    }

    /// Send a host header.
    #[api(id = "host")]
    async fn host_gotcha(TypedHeader(host): TypedHeader<gotcha::headers::Host>) -> String {
        host.to_string()
    }

    /// Send a host header.
    #[api(id = "host")]
    async fn host_axum(AxumTypedHeader(host): AxumTypedHeader<gotcha::headers::Host>) -> String {
        host.to_string()
    }

    #[test]
    fn gotcha_extractors_document_exactly_like_axum_originals() {
        let ours = GotchaRouter::<()>::default()
            .get("/items/{id}", read_gotcha)
            .post("/write", write_gotcha)
            .post("/form", form_gotcha)
            .post("/upload", upload_gotcha)
            .get("/host", host_gotcha)
            .into_openapi();
        let axum = GotchaRouter::<()>::default()
            .get("/items/{id}", read_axum)
            .post("/write", write_axum)
            .post("/form", form_axum)
            .post("/upload", upload_axum)
            .get("/host", host_axum)
            .into_openapi();
        assert_eq!(
            serde_json::to_string(&ours).unwrap(),
            serde_json::to_string(&axum).unwrap(),
            "the wrappers must document exactly like Axum's originals"
        );

        let document = serde_json::to_value(ours).unwrap();
        assert_eq!(document["paths"]["/items/{id}"]["get"]["parameters"][0]["name"], "id");
        assert_eq!(document["paths"]["/items/{id}"]["get"]["parameters"][1]["in"], "query");
        assert_eq!(
            document["paths"]["/write"]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/Payload"
        );
        assert_eq!(
            document["paths"]["/form"]["post"]["requestBody"]["content"]["application/x-www-form-urlencoded"]["schema"]["$ref"],
            "#/components/schemas/Filter"
        );
        assert!(document["paths"]["/upload"]["post"]["requestBody"]["content"]["multipart/form-data"].is_object());
        assert_eq!(document["paths"]["/host"]["get"]["parameters"][0]["in"], "header");
    }

    #[test]
    fn error_body_component_documents_optional_fields() {
        let document = serde_json::to_value(GotchaRouter::<()>::default().get("/failing", failing).into_openapi()).unwrap();
        let error_body = &document["components"]["schemas"]["ErrorBody"];
        assert_eq!(error_body["type"], "object");
        assert_eq!(error_body["required"], json!(["error"]));
        assert_eq!(error_body["properties"]["error"]["type"], "string");

        // `skip_serializing_if = "Option::is_none"` documents an optional object with no null branch.
        let fields = &error_body["properties"]["fields"];
        assert!(fields.get("anyOf").is_none(), "no null branch: {fields}");
        assert_eq!(fields["type"], "object");
        assert_eq!(fields["additionalProperties"]["type"], "array");
        assert_eq!(fields["additionalProperties"]["items"]["type"], "string");

        // Documents that never reference ErrorBody do not gain the component.
        let plain = serde_json::to_value(GotchaRouter::<()>::default().get("/read", read_gotcha).into_openapi()).unwrap();
        assert!(
            plain["components"]["schemas"].get("ErrorBody").is_none(),
            "unrelated documents are unchanged: {}",
            plain["components"]["schemas"]
        );
    }
}
