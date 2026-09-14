#![cfg(feature = "openapi")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{Request, StatusCode};
use gotcha::{api, ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter};
use tower::ServiceExt;

#[api]
async fn hello() -> String {
    "hello".into()
}

type Context = GotchaContext<(), EmptyConfig>;
struct App(Arc<AtomicUsize>);

impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn openapi_endpoints(&self) -> Option<gotcha::OpenApiEndpoints> {
        Some(gotcha::OpenApiEndpoints::default())
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        let child_calls = self.0.clone();
        let merged_calls = self.0.clone();
        let parent_calls = self.0.clone();
        router
            .get("/root", hello)
            .nest(
                "/api",
                GotchaRouter::default().get("/child", hello).openapi(move |mut spec| {
                    child_calls.fetch_add(1, Ordering::SeqCst);
                    // Children receive the assembled document, including routes outside their subtree.
                    assert!(spec.paths.contains_key("/root"));
                    assert!(spec.paths.contains_key("/merged"));
                    spec.paths.get_mut("/api/child").unwrap().get.as_mut().unwrap().security = Some(vec![]);
                    spec.info.title = "Child".into();
                    spec
                }),
            )
            .merge(GotchaRouter::default().get("/merged", hello).openapi(move |mut spec| {
                merged_calls.fetch_add(1, Ordering::SeqCst);
                spec.info.version = "2.0.0".into();
                spec
            }))
            .openapi(move |mut spec| {
                parent_calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(spec.info.title, "Child");
                spec.info.title = "Complete API".into();
                spec
            })
    }
}

#[tokio::test]
async fn trait_assembly_transforms_the_complete_document_once_before_serving() {
    let calls = Arc::new(AtomicUsize::new(0));
    let router = App(calls.clone())
        .build_router(Context {
            state: (),
            config: ConfigWrapper::default(),
        })
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);

    for _ in 0..2 {
        let response = router
            .clone()
            .oneshot(Request::builder().uri("/openapi.json").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(spec["info"]["title"], "Complete API");
        assert_eq!(spec["info"]["version"], "2.0.0");
        assert_eq!(spec["paths"]["/api/child"]["get"]["security"], serde_json::json!([]));
        assert!(spec["paths"]["/root"]["get"]["security"].is_null());
        assert!(spec["paths"]["/merged"]["get"]["security"].is_null());
        assert!(spec["security"].is_null());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3, "serving documentation must not rerun transforms");
}
