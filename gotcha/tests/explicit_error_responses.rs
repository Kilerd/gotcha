#![cfg(feature = "openapi")]

use gotcha::axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use gotcha::{api, ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Json, Path, Schematic, WithStatus};
use serde::Serialize;
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Serialize, Schematic)]
struct User {
    id: u32,
}

#[derive(Serialize, Schematic)]
struct ErrorBody {
    message: String,
}

// No Responsible or Schematic impl: only the wire body needs a schema.
#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("user not found")]
    NotFound,
    #[error("user already exists")]
    Conflict,
    #[error("internal error")]
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(ErrorBody { message: self.to_string() })).into_response()
    }
}

type ApiResult<T> = std::result::Result<T, ApiError>;

#[api(errors(
    response(status = 404, body = "ErrorBody", description = "Missing user"),
    response(status = 409, body = "ErrorBody", description = "Duplicate user"),
    response(status = 500, body = "ErrorBody", description = "Internal failure")
))]
async fn create(Path(id): Path<u32>) -> ApiResult<WithStatus<Json<User>, 201>> {
    match id {
        7 => Ok(WithStatus::new(Json(User { id }))),
        0 => Err(ApiError::Conflict),
        1 => Err(ApiError::NotFound),
        _ => Err(ApiError::Internal),
    }
}

#[api(errors(response(status = 404, body = "ErrorBody")))]
async fn lookup(Path(id): Path<u32>) -> Result<Json<User>, ApiError> {
    if id == 7 {
        Ok(Json(User { id }))
    } else {
        Err(ApiError::NotFound)
    }
}

#[api(
    errors(response(status = 409, body = "ErrorBody")),
    responses(response(status = 409, body = "ErrorBody", description = "Operation-specific conflict"))
)]
async fn dynamic() -> std::result::Result<(StatusCode, Json<User>), ApiError> {
    Ok((StatusCode::ACCEPTED, Json(User { id: 7 })))
}

struct App;
impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        router.get("/create/{id}", create).get("/lookup/{id}", lookup).get("/dynamic", dynamic)
    }
}

async fn request(router: &gotcha::axum::Router, path: &str) -> (u16, Value) {
    let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status().as_u16();
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn handlers_declare_their_own_errors_without_an_error_response_trait() {
    let router = App
        .build_router(GotchaContext {
            state: (),
            config: ConfigWrapper::default(),
        })
        .await
        .unwrap();
    let (_, spec) = request(&router, "/openapi.json").await;
    let responses = |path: &str| spec["paths"][path]["get"]["responses"].as_object().unwrap();
    assert_eq!(
        responses("/create/{id}").keys().map(String::as_str).collect::<Vec<_>>(),
        ["201", "404", "409", "500"]
    );
    assert_eq!(responses("/lookup/{id}").keys().map(String::as_str).collect::<Vec<_>>(), ["200", "404"]);
    assert_eq!(responses("/dynamic").keys().map(String::as_str).collect::<Vec<_>>(), ["409", "default"]);
    assert_eq!(responses("/create/{id}")["409"]["description"], "Duplicate user");
    assert_eq!(responses("/dynamic")["409"]["description"], "Operation-specific conflict");

    for (path, operation, status, body, schema) in [
        ("/create/7", "/create/{id}", 201, json!({"id": 7}), "User"),
        ("/create/1", "/create/{id}", 404, json!({"message": "user not found"}), "ErrorBody"),
        ("/create/0", "/create/{id}", 409, json!({"message": "user already exists"}), "ErrorBody"),
        ("/create/9", "/create/{id}", 500, json!({"message": "internal error"}), "ErrorBody"),
        ("/lookup/1", "/lookup/{id}", 404, json!({"message": "user not found"}), "ErrorBody"),
        ("/dynamic", "/dynamic", 202, json!({"id": 7}), "User"),
    ] {
        assert_eq!(request(&router, path).await, (status, body), "{path}");
        let declared = responses(operation)
            .get(&status.to_string())
            .unwrap_or_else(|| &responses(operation)["default"]);
        assert_eq!(
            declared["content"]["application/json"]["schema"]["$ref"],
            format!("#/components/schemas/{schema}")
        );
    }
    let schemas = spec["components"]["schemas"].as_object().unwrap();
    assert_eq!(schemas.keys().map(String::as_str).collect::<Vec<_>>(), ["ErrorBody", "User"]);
    assert_eq!(schemas["ErrorBody"]["properties"]["message"]["type"], "string");
}

#[test]
fn explicit_errors_preserve_the_other_response_bounds() {
    trybuild::TestCases::new().compile_fail("tests/fail/error_responses/*.rs");
}
