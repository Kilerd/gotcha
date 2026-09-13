#![cfg(feature = "openapi")]

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{Method, Request, StatusCode};
use gotcha::{api, ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Path};
use serde_json::Value;
use tower::ServiceExt;

#[api]
async fn hello() -> String {
    "hello".into()
}
#[api]
async fn create() -> String {
    "created".into()
}
#[api]
async fn health() -> String {
    "healthy".into()
}
#[api]
async fn item(Path((tenant, id)): Path<(String, u32)>) -> String {
    format!("{tenant}:{id}")
}

type Context = GotchaContext<(), EmptyConfig>;
struct NestedApp {
    prefixes: &'static [&'static str],
    child_path: &'static str,
    parameters: bool,
}
impl GotchaApp for NestedApp {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        let mut child = if self.parameters {
            GotchaRouter::default().get(self.child_path, item)
        } else {
            GotchaRouter::default().get(self.child_path, hello).post(self.child_path, create)
        };
        for prefix in self.prefixes.iter().rev() {
            child = GotchaRouter::default().nest(prefix, child);
        }
        router.get("/health", health).merge(child)
    }
}

async fn request(router: &gotcha::axum::Router, method: Method, path: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(Request::builder().method(method).uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec();
    (status, body)
}

async fn assemble(app: NestedApp) -> (gotcha::axum::Router, Value) {
    let router = app
        .build_router(Context {
            state: (),
            config: ConfigWrapper::default(),
        })
        .await
        .unwrap();
    let (status, bytes) = request(&router, Method::GET, "/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    (router, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn nested_document_paths_match_http_routes_at_slash_boundaries() {
    // Expected paths come from actual Axum routing semantics, including the significant
    // trailing slash on a root route and intentional empty segments away from the join.
    let cases: &[(&[&str], &str, &str, &str)] = &[
        (&["/api"], "/hello", "/api/hello", "/api//hello"),
        (&["/api/"], "/hello", "/api/hello", "/api//hello"),
        (&["/api"], "/", "/api", "/api/"),
        (&["/api/"], "/", "/api/", "/api"),
        (&["/api"], "/hello/", "/api/hello/", "/api/hello"),
        (&["/api/"], "/hello/", "/api/hello/", "/api/hello"),
        (&["/api"], "/v1//hello", "/api/v1//hello", "/api/v1/hello"),
        (&["/api//"], "/hello", "/api//hello", "/api/hello"),
        (&["/api"], "//hello", "/api//hello", "/api/hello"),
        (&["/api/"], "//hello", "/api/hello", "/api//hello"),
        (&["/api", "/v1"], "/hello", "/api/v1/hello", "/api//v1//hello"),
        (&["/api/", "/v1/"], "/hello", "/api/v1/hello", "/api//v1//hello"),
        (&["/api", "/v1"], "/", "/api/v1", "/api/v1/"),
        (&["/api/", "/v1/"], "/", "/api/v1/", "/api/v1"),
    ];
    for &(prefixes, child_path, expected, missing) in cases {
        let (router, spec) = assemble(NestedApp {
            prefixes,
            child_path,
            parameters: false,
        })
        .await;
        let paths = spec["paths"].as_object().unwrap();
        for (method, operation, body) in [(Method::GET, "get", "hello"), (Method::POST, "post", "created")] {
            let (status, bytes) = request(&router, method, expected).await;
            assert_eq!(status, StatusCode::OK, "{prefixes:?} + {child_path} should serve {expected}");
            assert_eq!(bytes, body.as_bytes());
            assert!(
                paths.get(expected).and_then(|path| path.get(operation)).is_some(),
                "missing {operation} {expected}: {paths:?}"
            );
        }
        assert_eq!(
            request(&router, Method::GET, missing).await.0,
            StatusCode::NOT_FOUND,
            "unexpected alias {missing}"
        );
        assert!(!paths.contains_key(missing), "document must not advertise {missing}");
        assert_eq!(paths.len(), 2, "only the nested path and the parent health route should be documented");
        assert!(paths["/health"].get("get").is_some());
        assert_eq!(request(&router, Method::GET, "/health").await.0, StatusCode::OK);
    }
}

#[tokio::test]
async fn multi_level_nesting_preserves_path_parameter_names_and_order() {
    let (router, spec) = assemble(NestedApp {
        prefixes: &["/tenants/{tenant}", "/v1/"],
        child_path: "/items/{id}",
        parameters: true,
    })
    .await;
    let expected = "/tenants/{tenant}/v1/items/{id}";
    let (status, bytes) = request(&router, Method::GET, "/tenants/acme/v1/items/42").await;
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
    assert_eq!(spec["paths"].as_object().unwrap().len(), 2);
    assert_eq!(request(&router, Method::GET, "/tenants/acme//v1/items/42").await.0, StatusCode::NOT_FOUND);
}
