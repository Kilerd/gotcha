#![cfg(feature = "openapi")]

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{Request, StatusCode};
use gotcha::{api, ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Path};
use serde_json::Value;
use tower::ServiceExt;

#[api]
async fn item(Path((tenant, id)): Path<(String, u32)>) -> String {
    format!("{tenant}:{id}")
}

type Context = GotchaContext<(), EmptyConfig>;
struct NestedApp;
impl GotchaApp for NestedApp {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn openapi_endpoints(&self) -> Option<gotcha::OpenApiEndpoints> {
        Some(gotcha::OpenApiEndpoints::default())
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        router.nest(
            "/tenants/{tenant}",
            GotchaRouter::default().nest("/v1/", GotchaRouter::default().get("/items/{id}", item)),
        )
    }
}

async fn request(router: &gotcha::axum::Router, path: &str) -> (StatusCode, Vec<u8>) {
    let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec();
    (status, body)
}

#[tokio::test]
async fn multi_level_nesting_preserves_path_parameter_names_and_order() {
    let router = NestedApp
        .build_router(Context {
            state: (),
            config: ConfigWrapper::default(),
        })
        .await
        .unwrap();
    let (status, bytes) = request(&router, "/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    let spec: Value = serde_json::from_slice(&bytes).unwrap();
    let expected = "/tenants/{tenant}/v1/items/{id}";
    let (status, bytes) = request(&router, "/tenants/acme/v1/items/42").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"acme:42");
    let operation = &spec["paths"][expected]["get"];
    let parameters = operation["parameters"].as_array().expect("path parameters are documented");
    assert_eq!(
        parameters.iter().map(|param| param["name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["tenant", "id"]
    );
    for parameter in parameters {
        assert_eq!(parameter["in"], "path");
        assert_eq!(parameter["required"], true);
    }
    assert_eq!(parameters[0]["schema"]["type"], "string");
    assert_eq!(parameters[1]["schema"]["type"], "integer");
    assert_eq!(spec["paths"].as_object().unwrap().len(), 1);
    assert_eq!(request(&router, "/tenants/acme//v1/items/42").await.0, StatusCode::NOT_FOUND);
}
