#![cfg(feature = "openapi")]

use gotcha::axum::{
    body::{to_bytes, Body, Bytes},
    http::{Request, StatusCode},
    response::{Html, IntoResponse, Response},
};
use gotcha::{api, ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Json, Path, Responsible, Schematic, WithStatus};
use serde::Serialize;
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Serialize, Schematic)]
struct Record {
    id: u32,
}

// A type may now describe both its data and its own HTTP response without coherence conflicts.
impl IntoResponse for Record {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
impl Responsible for Record {
    fn response() -> gotcha::oas::Responses {
        gotcha::response::response::<Self>(200, "application/json", "record")
    }
}

#[derive(Serialize, Schematic)]
struct Problem {
    message: String,
}

#[derive(Serialize, Schematic)]
struct DeclaredProblem {
    code: u32,
}

#[api(responses(response(status = 409, body = "DeclaredProblem")), drop_default)]
async fn declared() -> Response {
    (StatusCode::CONFLICT, Json(DeclaredProblem { code: 42 })).into_response()
}

#[api]
async fn create() -> WithStatus<Record, 201> {
    WithStatus::new(Record { id: 7 })
}

#[api]
async fn missing() -> Result<Json<Record>, WithStatus<Json<Problem>, 404>> {
    Err(WithStatus::new(Json(Problem { message: "missing".into() })))
}

#[api(
    responses(
        response(status = 404, body = "Problem", description = "Missing record"),
        response(status = 409, body = "Problem", description = "Duplicate record")
    ),
    drop_default
)]
async fn lookup(Path(id): Path<u32>) -> Result<Json<Record>, (StatusCode, Json<Problem>)> {
    match id {
        7 => Ok(Json(Record { id })),
        0 => Err((StatusCode::CONFLICT, Json(Problem { message: "duplicate".into() }))),
        _ => Err((StatusCode::NOT_FOUND, Json(Problem { message: "missing".into() }))),
    }
}

#[api(responses(response(status = 200, body = "String", content_type = "text/plain", description = "Plain greeting")))]
async fn text() -> String {
    "hello".into()
}
#[api]
async fn binary() -> Bytes {
    Bytes::from_static(b"\x00\x01\x02")
}
#[api]
async fn html() -> Html<&'static str> {
    Html("<p>hello</p>")
}
#[api]
async fn empty() -> WithStatus<Json<Record>, 204> {
    WithStatus::new(Json(Record { id: 9 }))
}

#[api(responses(response(status = 200, body = "String", content_type = "text/csv")), drop_default)]
async fn csv() -> Response {
    Response::builder().header("content-type", "text/csv").body(Body::from("id\n7\n")).unwrap()
}

struct App;
impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;
    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }
    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        router
            .get("/declared", declared)
            .get("/created", create)
            .get("/missing", missing)
            .get("/lookup/{id}", lookup)
            .get("/text", text)
            .get("/binary", binary)
            .get("/html", html)
            .get("/empty", empty)
            .get("/csv", csv)
    }
}

async fn request(router: &gotcha::axum::Router, path: &str) -> (u16, String, Vec<u8>) {
    let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status().as_u16();
    let media = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
        .unwrap_or_default();
    (status, media, to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec())
}

#[tokio::test]
async fn response_contracts_match_http_and_collect_explicit_components() {
    let router = App
        .build_router(GotchaContext {
            state: (),
            config: ConfigWrapper::default(),
        })
        .await
        .unwrap();
    let (_, _, body) = request(&router, "/openapi.json").await;
    let spec: Value = serde_json::from_slice(&body).unwrap();
    for (path, documented_path, expected_status, expected_media, expected_body) in [
        ("/declared", "/declared", 409, "application/json", br#"{"code":42}"#.as_slice()),
        ("/created", "/created", 201, "application/json", br#"{"id":7}"#.as_slice()),
        ("/missing", "/missing", 404, "application/json", br#"{"message":"missing"}"#.as_slice()),
        ("/lookup/7", "/lookup/{id}", 200, "application/json", br#"{"id":7}"#.as_slice()),
        ("/lookup/0", "/lookup/{id}", 409, "application/json", br#"{"message":"duplicate"}"#.as_slice()),
        ("/lookup/1", "/lookup/{id}", 404, "application/json", br#"{"message":"missing"}"#.as_slice()),
        ("/text", "/text", 200, "text/plain", b"hello".as_slice()),
        ("/binary", "/binary", 200, "application/octet-stream", b"\x00\x01\x02".as_slice()),
        ("/html", "/html", 200, "text/html", b"<p>hello</p>".as_slice()),
        ("/empty", "/empty", 204, "", b"".as_slice()),
        ("/csv", "/csv", 200, "text/csv", b"id\n7\n".as_slice()),
    ] {
        let (status, media, body) = request(&router, path).await;
        assert_eq!(
            (status, media.as_str(), body.as_slice()),
            (expected_status, expected_media, expected_body),
            "{path}"
        );
        let response = &spec["paths"][documented_path]["get"]["responses"][status.to_string()];
        assert!(response.is_object(), "documented status for {path}");
        if media.is_empty() {
            assert!(response["content"].is_null());
        } else {
            assert!(response["content"][media].is_object());
        }
    }
    assert!(spec["paths"]["/created"]["get"]["responses"]["200"].is_null());
    assert!(spec["paths"]["/missing"]["get"]["responses"]["default"].is_null());
    let lookup = &spec["paths"]["/lookup/{id}"]["get"]["responses"];
    assert!(lookup["default"].is_null());
    assert_eq!(lookup["409"]["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/Problem");
    assert_eq!(spec["paths"]["/text"]["get"]["responses"]["200"]["description"], "Plain greeting");
    assert_eq!(
        spec["paths"]["/binary"]["get"]["responses"]["200"]["content"]["application/octet-stream"]["schema"],
        json!({"type":"string","format":"binary"})
    );
    assert!(spec["components"]["schemas"]["Record"].is_object());
    assert!(spec["components"]["schemas"]["Problem"].is_object());
    assert_eq!(
        spec["paths"]["/declared"]["get"]["responses"]["409"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/DeclaredProblem"
    );
    assert!(
        spec["components"]["schemas"]["DeclaredProblem"].is_object(),
        "raw Response cannot infer this component"
    );
}
