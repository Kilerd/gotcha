//! Route-scoped middleware, nested services, and the context-aware finish hook.

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::extract::Request;
use gotcha::axum::http::{HeaderMap, HeaderValue, Request as HttpRequest, StatusCode};
use gotcha::axum::middleware::{self, Next};
use gotcha::axum::response::Response;
use gotcha::axum::Router;
use gotcha::{ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter};
use tower::ServiceExt;

#[derive(Clone)]
struct State {
    value: Arc<AtomicUsize>,
}

type Context = GotchaContext<State, EmptyConfig>;

#[cfg(feature = "openapi")]
#[gotcha::api]
async fn documented() -> String {
    "documented".into()
}

async fn guarded() -> &'static str {
    "guarded"
}

async fn mark_route(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert("x-route-layer", HeaderValue::from_static("applied"));
    response
}

struct App;

impl GotchaApp for App {
    type State = State;
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<State> {
        Ok(context().state)
    }

    #[cfg(feature = "openapi")]
    fn openapi_endpoints(&self) -> Option<gotcha::OpenApiEndpoints> {
        Some(gotcha::OpenApiEndpoints::default())
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        #[cfg(feature = "openapi")]
        let router = router.get("/documented", documented);
        router
            .get("/guarded", guarded)
            .route_layer(middleware::from_fn(mark_route))
            .fallback(|| async { (StatusCode::NOT_FOUND, "fallback") })
    }

    fn finish_router(&self, router: Router, context: &Context) -> Router {
        let value = context.state.value.clone();
        router.nest_service(
            "/proxy",
            tower::service_fn(move |_: Request| {
                let value = value.clone();
                async move {
                    let current = value.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, Infallible>(Response::new(Body::from(format!("proxied {current}"))))
                }
            }),
        )
    }
}

fn context() -> Context {
    Context::new(
        State {
            value: Arc::new(AtomicUsize::new(7)),
        },
        ConfigWrapper::default(),
    )
}

async fn request(router: &Router, path: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(HttpRequest::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec();
    (status, headers, body)
}

#[tokio::test]
async fn route_layer_covers_matched_routes_but_not_the_fallback() {
    let router = App.build_router(context()).await.unwrap();

    let (status, headers, body) = request(&router, "/guarded").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"guarded");
    assert_eq!(headers.get("x-route-layer").unwrap(), "applied");

    let (status, headers, body) = request(&router, "/missing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, b"fallback");
    assert!(headers.get("x-route-layer").is_none(), "unmatched requests bypass route_layer");
}

#[tokio::test]
async fn finish_hook_mounts_a_service_built_from_state() {
    let context = context();
    let value = context.state.value.clone();
    let router = App.build_router(context).await.unwrap();

    let (status, _, body) = request(&router, "/proxy/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"proxied 7");
    assert_eq!(value.load(Ordering::SeqCst), 8, "the mounted service reads and updates application state");

    assert_eq!(request(&router, "/proxy/status").await.2, b"proxied 8");
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn nested_services_are_opaque_to_the_served_document() {
    let router = App.build_router(context()).await.unwrap();
    let (status, _, body) = request(&router, "/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    let spec: gotcha::serde_json::Value = gotcha::serde_json::from_slice(&body).unwrap();
    let paths = spec["paths"].as_object().unwrap();
    assert!(paths.contains_key("/documented"), "{paths:?}");
    assert!(paths.keys().all(|path| !path.starts_with("/proxy")), "{paths:?}");
}

#[cfg(feature = "openapi")]
#[test]
fn builder_nested_services_stay_out_of_exported_documents() {
    let spec = gotcha::Gotcha::new()
        .get("/guarded", guarded)
        .nest_service(
            "/assets",
            tower::service_fn(|_: Request| async { Ok::<_, Infallible>(Response::new(Body::from("asset"))) }),
        )
        .into_openapi();
    assert!(spec.paths.is_empty(), "nested services are not documented: {:?}", spec.paths.keys());
}
