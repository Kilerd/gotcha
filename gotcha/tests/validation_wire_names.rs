//! `Valid` reports validation errors under serde's wire names, end to end through a request.

use std::collections::HashMap;

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{Request, StatusCode};
use gotcha::{
    ConfigWrapper, EmptyConfig, ErrorBody, ExtractorConfigs, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Json, Rejected, Schematic, Valid,
    ValidConfig, ValidRejection, Validate,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Debug, Deserialize, Serialize, Schematic, Validate)]
#[validate(crate = "gotcha::validator")]
#[serde(rename_all = "camelCase")]
struct User {
    #[validate(length(min = 1))]
    first_name: String,
    #[validate(length(min = 1))]
    #[serde(rename = "e-mail")]
    email: String,
}

#[derive(Debug, Deserialize, Serialize, Schematic, Validate)]
#[validate(crate = "gotcha::validator")]
#[serde(rename_all = "kebab-case")]
struct Address {
    #[validate(length(min = 1))]
    zip_code: String,
}

#[derive(Debug, Deserialize, Serialize, Schematic, Validate)]
#[validate(crate = "gotcha::validator")]
#[serde(rename_all = "camelCase")]
struct LineItem {
    #[validate(length(min = 1))]
    item_name: String,
}

#[derive(Debug, Deserialize, Serialize, Schematic, Validate)]
#[validate(crate = "gotcha::validator")]
#[serde(rename_all = "camelCase")]
struct Create {
    #[validate(nested)]
    address: Address,
    #[validate(nested)]
    items: Vec<LineItem>,
    #[validate(nested)]
    maybe: Option<LineItem>,
    #[validate(nested)]
    entries: HashMap<String, LineItem>,
}

#[derive(Debug, Deserialize, Serialize, Schematic, Validate)]
#[validate(crate = "gotcha::validator")]
#[serde(rename_all = "camelCase")]
struct Envelope {
    #[validate(length(min = 1))]
    order_id: String,
    #[serde(flatten)]
    #[validate(nested)]
    address: Address,
}

#[derive(Debug, Deserialize, Serialize, Schematic, Validate)]
#[validate(crate = "gotcha::validator", schema(function = "reject_everything"))]
#[serde(rename_all = "camelCase")]
struct Whole {
    #[validate(length(min = 1))]
    display_name: String,
}

/// Only the deserialize direction names the input, and that is the side errors must match.
#[derive(Debug, Deserialize, Serialize, Schematic, Validate)]
#[allow(dead_code)]
#[validate(crate = "gotcha::validator")]
#[serde(rename_all(serialize = "PascalCase", deserialize = "camelCase"))]
struct DeserializeOnly {
    #[validate(length(min = 1))]
    first_name: String,
    #[validate(length(min = 1))]
    #[serde(rename(serialize = "EMAIL", deserialize = "e-mail"))]
    email: String,
}

/// `#[serde(skip_serializing)]` only affects output: the field is still read from the request,
/// so its errors must use the input name. Its type is never referenced by the derive, so the
/// type does not need `Schematic`.
#[derive(Debug, Deserialize, Serialize)]
struct Token(String);

#[derive(Debug, Deserialize, Serialize, Schematic, Validate)]
#[validate(crate = "gotcha::validator")]
#[serde(rename_all = "camelCase")]
struct UpdateAccount {
    #[validate(length(min = 8))]
    #[serde(skip_serializing)]
    new_password: String,
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    legacy: Token,
}

fn reject_everything(_: &Whole) -> Result<(), gotcha::validator::ValidationError> {
    Err(gotcha::validator::ValidationError::new("some_custom_rule"))
}

async fn create_user(Valid(Json(user)): Valid<Json<User>>) -> Json<User> {
    Json(user)
}

async fn create(Valid(Json(create)): Valid<Json<Create>>) -> String {
    create.address.zip_code
}

async fn flatten(Valid(Json(envelope)): Valid<Json<Envelope>>) -> String {
    envelope.order_id
}

async fn whole(Valid(Json(whole)): Valid<Json<Whole>>) -> String {
    whole.display_name
}

async fn deserialize_only(Valid(Json(payload)): Valid<Json<DeserializeOnly>>) -> String {
    payload.first_name
}

async fn update_account(Valid(Json(account)): Valid<Json<UpdateAccount>>) -> String {
    account.new_password
}

async fn create_users(Valid(Json(users)): Valid<Json<Vec<User>>>) -> String {
    users.len().to_string()
}

/// Axum's per-handler pattern: the rejection carries the same remapped errors.
async fn rejected(body: Result<Valid<Json<User>>, Rejected<ValidRejection>>) -> String {
    match body {
        Ok(_) => "ok".to_string(),
        Err(rejected) => match rejected.rejection() {
            ValidRejection::Invalid(errors) => {
                let fields = gotcha::validation::flatten_errors(errors);
                fields.keys().cloned().collect::<Vec<_>>().join(",")
            }
            ValidRejection::Json(_) => "json".to_string(),
        },
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
            .post("/users", create_user)
            .post("/create", create)
            .post("/flatten", flatten)
            .post("/whole", whole)
            .post("/deserialize-only", deserialize_only)
            .post("/account", update_account)
            .post("/users-list", create_users)
            .post("/rejected", rejected)
    }
}

/// A custom handler reads the `ValidationErrors` directly, so its output proves they were
/// remapped before it ran.
struct CustomHandlerApp;

impl GotchaApp for CustomHandlerApp {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn extractor_configs(&self) -> ExtractorConfigs {
        ExtractorConfigs::default().set(ValidConfig::default().error_handler(|errors: &gotcha::validator::ValidationErrors| {
            let mut keys: Vec<String> = errors.errors().keys().map(ToString::to_string).collect();
            keys.sort();
            (StatusCode::UNPROCESSABLE_ENTITY, ErrorBody::new(keys.join(",")))
        }))
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        router.post("/users", create_user)
    }
}

async fn app() -> gotcha::axum::Router {
    App.build_router(context()).await.unwrap()
}

async fn send(router: &gotcha::axum::Router, path: &str, body: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn rename_all_and_field_rename_use_wire_names() {
    let (status, body) = send(&app().await, "/users", r#"{"firstName":"","e-mail":""}"#).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        json!({
            "error": "validation failed",
            "fields": {
                "firstName": ["length must be at least 1"],
                "e-mail": ["length must be at least 1"]
            }
        })
    );
}

#[tokio::test]
async fn nested_structs_lists_options_and_maps_compose() {
    let (status, body) = send(
        &app().await,
        "/create",
        r#"{"address":{"zip-code":""},"items":[{"itemName":""}],"maybe":{"itemName":""},"entries":{"one":{"itemName":""}}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let fields = &body["fields"];
    assert_eq!(fields["address.zip-code"], json!(["length must be at least 1"]));
    assert_eq!(fields["items[0].itemName"], json!(["length must be at least 1"]));
    assert_eq!(fields["maybe.itemName"], json!(["length must be at least 1"]));
    // Map values are indexed by validator's iteration order and lose their key (see #141).
    let map_key = fields
        .as_object()
        .unwrap()
        .keys()
        .find(|key| key.starts_with("entries[0]."))
        .unwrap_or_else(|| panic!("a map entry must be reported with an index: {fields}"));
    assert_eq!(fields[map_key], json!(["length must be at least 1"]));
}

#[tokio::test]
async fn flattened_struct_fields_lose_their_own_segment() {
    let (status, body) = send(&app().await, "/flatten", r#"{"orderId":"","zip-code":""}"#).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        json!({
            "error": "validation failed",
            "fields": {
                "orderId": ["length must be at least 1"],
                "zip-code": ["length must be at least 1"]
            }
        })
    );
}

#[tokio::test]
async fn struct_level_errors_keep_all() {
    let (status, body) = send(&app().await, "/whole", r#"{"displayName":"ok"}"#).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        json!({"error": "validation failed", "fields": {"__all__": ["failed validation: some_custom_rule"]}})
    );
}

#[tokio::test]
async fn deserialize_only_renames_are_used() {
    let (status, body) = send(&app().await, "/deserialize-only", r#"{"firstName":"","e-mail":""}"#).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        json!({
            "error": "validation failed",
            "fields": {
                "firstName": ["length must be at least 1"],
                "e-mail": ["length must be at least 1"]
            }
        })
    );
}

#[tokio::test]
async fn skip_serializing_fields_still_use_their_input_name() {
    // `skip_serializing` keeps the field in deserialization, so `newPassword` is accepted on the
    // wire and reported back under that name. The `Token` field proves the derive does not
    // require `Schematic` on a field it skips when serializing.
    let (status, body) = send(&app().await, "/account", r#"{"newPassword":"","legacy":"abc"}"#).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        json!({
            "error": "validation failed",
            "fields": {"newPassword": ["length must be at least 8"]}
        })
    );
}

#[tokio::test]
async fn top_level_collections_index_without_the_internal_key() {
    let (status, body) = send(&app().await, "/users-list", r#"[{"firstName":"","e-mail":"x"}]"#).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        json!({"error": "validation failed", "fields": {"[0].firstName": ["length must be at least 1"]}})
    );
}

#[tokio::test]
async fn a_custom_valid_config_handler_sees_wire_names() {
    let router = CustomHandlerApp.build_router(context()).await.unwrap();
    let request = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"firstName":"","e-mail":""}"#))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body, json!({"error": "e-mail,firstName"}));
}

#[tokio::test]
async fn rejected_exposes_remapped_errors() {
    // The handler receives the remapped errors through `Rejected<ValidRejection>` and prints
    // their flattened paths.
    let request = Request::builder()
        .method("POST")
        .uri("/rejected")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"firstName":"","e-mail":""}"#))
        .unwrap();
    let response = app().await.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(bytes.as_ref(), b"e-mail,firstName");
}
