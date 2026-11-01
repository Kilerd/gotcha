//! The opt-in Prometheus endpoint: mounting, rendering, custom paths, and the process-wide recorder.

#![cfg(feature = "prometheus")]
// These tests also cover `GotchaApp`, which stays supported until it is removed.
#![allow(deprecated)]

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{header, Request, StatusCode};
use gotcha::axum::Router;
use gotcha::{ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, MetricsEndpoint};
use tower::ServiceExt;

type Context = GotchaContext<(), EmptyConfig>;

#[derive(Default)]
struct App {
    metrics: Option<MetricsEndpoint>,
    #[cfg(feature = "openapi")]
    openapi: bool,
}

impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        router
            .get("/hello", || async { "hello" })
            .get("/users/{id}", || async { "user" })
            .nest("/nested", GotchaRouter::default().get("/ping", || async { "pong" }))
    }

    fn metrics_endpoint(&self) -> Option<MetricsEndpoint> {
        self.metrics.clone()
    }

    #[cfg(feature = "openapi")]
    fn openapi_endpoints(&self) -> Option<gotcha::OpenApiEndpoints> {
        self.openapi.then(gotcha::OpenApiEndpoints::default)
    }
}

async fn router(app: &App) -> Router {
    app.build_router(GotchaContext::new((), ConfigWrapper::default()))
        .await
        .expect("router assembly succeeds")
}

async fn get(router: &Router, path: &str) -> (StatusCode, String, String) {
    let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, content_type, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn enabled_endpoint_records_business_and_nested_routes() {
    let app = App {
        metrics: Some(MetricsEndpoint::default()),
        ..Default::default()
    };
    let router = router(&app).await;
    assert_eq!(get(&router, "/hello").await.0, StatusCode::OK);
    assert_eq!(get(&router, "/nested/ping").await.0, StatusCode::OK);
    // Custom metrics recorded through the re-export use the same process-wide recorder.
    gotcha::prometheus::counter!("gotcha_custom_events_total").increment(1);

    let (status, content_type, body) = get(&router, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain"), "{content_type}");
    for expected in [
        "axum_http_requests_total",
        r#"endpoint="/hello""#,
        r#"endpoint="/nested/ping""#,
        r#"method="GET""#,
        r#"status="200""#,
        "gotcha_custom_events_total",
    ] {
        assert!(body.contains(expected), "missing {expected} in:\n{body}");
    }
}

#[tokio::test]
async fn unmatched_requests_share_one_label_and_routes_keep_their_templates() {
    let app = App {
        metrics: Some(MetricsEndpoint::default()),
        ..Default::default()
    };
    let router = router(&app).await;

    assert_eq!(get(&router, "/users/42").await.0, StatusCode::OK);
    for index in 0..50 {
        let path = format!("/scanning-attempt-{index}");
        assert_eq!(get(&router, &path).await.0, StatusCode::NOT_FOUND);
    }

    let (status, _, body) = get(&router, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"endpoint="/users/{id}""#), "parameterised routes keep their template:\n{body}");
    assert!(body.contains(r#"endpoint="<unmatched>""#), "{body}");
    for index in 0..50 {
        let leaked = format!(r#"endpoint="/scanning-attempt-{index}""#);
        assert!(!body.contains(&leaked), "unmatched URLs must not become labels:\n{body}");
    }
}

#[tokio::test]
async fn metrics_are_disabled_by_default() {
    let router = router(&App::default()).await;
    assert_eq!(get(&router, "/metrics").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn custom_path_serves_metrics_and_leaves_the_default_path_unmounted() {
    let app = App {
        metrics: Some(MetricsEndpoint {
            path: "/internal/metrics".into(),
        }),
        ..Default::default()
    };
    let router = router(&app).await;
    assert_eq!(get(&router, "/hello").await.0, StatusCode::OK);
    assert_eq!(get(&router, "/internal/metrics").await.0, StatusCode::OK);
    let (status, content_type, body) = get(&router, "/internal/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain"), "{content_type}");
    assert!(body.contains(r#"endpoint="/hello""#), "{body}");
    // The endpoint is mounted after the metric layer, so its own scrapes are not recorded; the
    // second scrape would otherwise report the first.
    assert!(!body.contains(r#"endpoint="/internal/metrics""#), "scrapes must not be measured:\n{body}");
    assert_eq!(get(&router, "/metrics").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_path_fails_assembly() {
    let app = App {
        metrics: Some(MetricsEndpoint { path: "metrics".into() }),
        ..Default::default()
    };
    let error = match app.build_router(GotchaContext::new((), ConfigWrapper::default())).await {
        Ok(_) => panic!("an invalid metrics path must fail assembly"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("metrics endpoint"), "{error}");
}

#[tokio::test]
async fn public_layer_helper_installs_the_metrics_endpoint() {
    let router = Router::new().route("/direct", gotcha::axum::routing::get(|| async { "direct" }));
    let router = MetricsEndpoint::default().layer(router).expect("metrics layer installs");

    assert_eq!(get(&router, "/direct").await.0, StatusCode::OK);
    let (status, content_type, body) = get(&router, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain"), "{content_type}");
    assert!(body.contains(r#"endpoint="/direct""#), "{body}");
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn metrics_path_conflicting_with_enabled_documentation_fails_assembly() {
    for path in ["/openapi.json", "/scalar"] {
        let app = App {
            metrics: Some(MetricsEndpoint { path: path.into() }),
            openapi: true,
        };
        let error = match app.build_router(GotchaContext::new((), ConfigWrapper::default())).await {
            Ok(_) => panic!("{path} conflicts with an enabled documentation endpoint"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("conflicts with an enabled OpenAPI"), "{path}: {error}");
    }

    // Without documentation endpoints the same path is free.
    let app = App {
        metrics: Some(MetricsEndpoint { path: "/openapi.json".into() }),
        openapi: false,
    };
    let router = router(&app).await;
    assert_eq!(get(&router, "/openapi.json").await.0, StatusCode::OK);
}

#[tokio::test]
async fn two_applications_share_the_process_wide_recorder() {
    let first = router(&App {
        metrics: Some(MetricsEndpoint::default()),
        ..Default::default()
    })
    .await;
    let second = router(&App {
        metrics: Some(MetricsEndpoint::default()),
        ..Default::default()
    })
    .await;

    assert_eq!(get(&second, "/hello").await.0, StatusCode::OK);
    for router in [&first, &second] {
        let (status, _, body) = get(router, "/metrics").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(r#"endpoint="/hello""#), "{body}");
    }
}

#[tokio::test]
async fn application_layers_wrap_the_metric_layer_and_extractor_configs_stay_outermost() {
    use gotcha::axum::extract::Request as ExtractRequest;
    use gotcha::axum::middleware::{self, Next};
    use gotcha::axum::response::Response;
    use gotcha::axum::routing::get as axum_get;

    async fn stamp(request: ExtractRequest, next: Next) -> Response {
        let mut response = next.run(request).await;
        response.headers_mut().insert("x-app-layer", "applied".parse().unwrap());
        response
    }

    struct Layered;

    impl GotchaApp for Layered {
        type State = ();
        type Config = EmptyConfig;

        async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
            Ok(())
        }

        fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
            router.post("/echo", |gotcha::Json(value): gotcha::Json<u32>| async move { value.to_string() })
        }

        fn metrics_endpoint(&self) -> Option<MetricsEndpoint> {
            Some(MetricsEndpoint::default())
        }

        fn extractor_configs(&self) -> gotcha::ExtractorConfigs {
            gotcha::ExtractorConfigs::default().set(
                gotcha::JsonConfig::default()
                    .error_handler(|rejection| (StatusCode::BAD_REQUEST, gotcha::ErrorBody::new(format!("custom: {}", rejection.body_text())))),
            )
        }

        fn finish_router(&self, router: Router, _: &Context) -> Router {
            router.route("/late", axum_get(|| async { "late" })).layer(middleware::from_fn(stamp))
        }
    }

    let router = Layered
        .build_router(GotchaContext::new((), ConfigWrapper::default()))
        .await
        .expect("router assembly succeeds");

    // finish_router middleware runs outside the metric layer, so it also covers /metrics.
    let response = router
        .clone()
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("x-app-layer").unwrap(), "applied");

    let response = router
        .clone()
        .oneshot(Request::builder().uri("/late").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), "late");

    // The extractor-config layer stays outermost: the configured rejection handler answers even
    // though the metric layer nested inside it records the request.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/echo")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(String::from_utf8_lossy(&body).starts_with(r#"{"error":"custom: "#), "{body:?}");

    let (_, _, body) = get(&router, "/metrics").await;
    assert!(body.contains(r#"endpoint="/echo""#), "{body}");
    assert!(!body.contains(r#"endpoint="/late""#), "finish_router routes are not measured:\n{body}");
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn openapi_document_has_no_metrics_path_and_documentation_is_not_measured() {
    let app = App {
        metrics: Some(MetricsEndpoint::default()),
        openapi: true,
    };
    let router = router(&app).await;
    let (status, _, document) = get(&router, "/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    let spec: gotcha::serde_json::Value = gotcha::serde_json::from_str(&document).unwrap();
    assert!(!spec["paths"].as_object().unwrap().contains_key("/metrics"));
    // Exporting the document directly never assembles the application, so it neither installs
    // the recorder nor documents the metrics route.
    assert!(!app.openapi_document().paths.contains_key("/metrics"));

    // Documentation endpoints are mounted after the metric layer, so their requests are not
    // recorded. Only this test requests these paths in the test binary.
    assert_eq!(get(&router, "/scalar").await.0, StatusCode::OK);
    let (status, _, body) = get(&router, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    for path in ["/openapi.json", "/scalar"] {
        let series = format!(r#"endpoint="{path}""#);
        assert!(!body.contains(&series), "documentation request {path} must not be measured:\n{body}");
    }
}
