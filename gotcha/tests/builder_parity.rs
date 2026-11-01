//! Every [`GotchaApp`] method has a builder equivalent. The tests below describe one application
//! with both APIs, overriding every trait method on one side and calling its builder equivalent on
//! the other, and check that the two behave the same. MIGRATION.md lists where using the two still
//! differs. The `droppen_*` tests port the shape of a large trait application to the builder: a
//! custom configuration loader, state built from the configuration, an embedded router with extra
//! services and layers, and its own listener with connect info.
//!
//! | `GotchaApp`                                  | `Gotcha`                                       | test                                   |
//! | -------------------------------------------- | ---------------------------------------------- | -------------------------------------- |
//! | `type State`, `type Config`                  | `Gotcha<S, C>`                                 | every test                             |
//! | `config()`                                   | `load_config`                                  | `startup_stages_match`                 |
//! | `config_error_policy()`                      | `config_error_policy`                          | `config_errors_follow_the_same_policy` |
//! | `logger()`                                   | `logger`, `with_default_logger`, `default_logger` | `startup_stages_match`, `builder_logger.rs`, `default_logger.rs` |
//! | `state()`                                    | `from_state_fn`                                | `startup_stages_match`, `failing_stages_match` |
//! | `routes()`                                   | route methods, `routes`, `method_route`, `fallback_service` | `assembled_routers_match` |
//! | `extractor_configs()`                        | `extractor_configs`, `extractor_config`        | `assembled_routers_match`              |
//! | `finish_router()`                            | `finish_router`, `try_finish_router`           | `assembled_routers_match`, `failing_router_assembly_matches` |
//! | `build_router()`                             | `into_router`                                  | `assembled_routers_match`, `configuration_without_serde_assembles` |
//! | `openapi_endpoints()`                        | `openapi_endpoints`, `with_openapi`            | `documents_match`                      |
//! | `openapi_info()`                             | `openapi_info`                                 | `documents_match`                      |
//! | `document_extractor_errors()`                | `document_extractor_errors`                    | `documents_match`                      |
//! | `openapi_document()`                         | `into_openapi` on a freshly built application  | `documents_match`                      |
//! | `metrics_endpoint()`                         | `metrics`                                      | `assembled_routers_match`              |
//! | `tasks()`                                    | `try_tasks`, `tasks`                           | `startup_stages_match`, `failing_stages_match`, `shutdown_lifecycles_match` |
//! | `shutdown_signal()`                          | `shutdown_signal`                              | `shutdown_lifecycles_match`            |
//! | `http_shutdown_timeout()`                    | `http_shutdown_timeout`                        | `shutdown_lifecycles_match`            |
//! | `task_shutdown_timeout()`                    | `task_shutdown_timeout`                        | `shutdown_lifecycles_match`            |
//! | `on_shutdown()`                              | `on_shutdown`                                  | `shutdown_lifecycles_match`            |
//! | `run()`, `bind()`                            | `run`, `bind`                                  | `shutdown_lifecycles_match`            |

use std::convert::Infallible;
use std::future::{Future, IntoFuture};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::extract::Request;
use gotcha::axum::http::StatusCode;
use gotcha::axum::middleware::{self, Next};
use gotcha::axum::response::Response;
use gotcha::axum::routing::MethodFilter;
use gotcha::axum::Router;
use gotcha::{
    ConfigErrorPolicy, ConfigWrapper, ErrorBody, ExtractorConfigs, Gotcha, GotchaApp, GotchaContext, GotchaError, GotchaResult, GotchaRouter, Json, JsonConfig,
    Schematic, ServerConfig, State,
};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tower::ServiceExt;

type Events = Arc<Mutex<Vec<&'static str>>>;
type Context = GotchaContext<AppState, Settings>;

/// How long both drain deadlines are. The defaults are 30 seconds, so finishing well within the
/// test timeout shows the overrides were applied.
const DRAIN: Duration = Duration::from_millis(200);

#[derive(Clone, Deserialize)]
struct Settings {
    label: String,
}

#[derive(Clone)]
struct AppState {
    label: String,
    port: u16,
    events: Events,
}

/// What a test varies; both descriptions of the application read the same options.
#[derive(Clone, Default)]
struct Options {
    events: Events,
    bound: Arc<Mutex<Option<u16>>>,
    stop: Arc<Notify>,
    fail_config: bool,
    /// `"state"` or `"tasks"`: the stage that returns an error.
    fail_at: Option<&'static str>,
    fallback: bool,
    #[cfg_attr(not(feature = "openapi"), allow(dead_code))]
    hide_extractor_errors: bool,
}

fn record(events: &Events, event: &'static str) {
    events.lock().unwrap().push(event);
}

fn events(options: &Options) -> Vec<&'static str> {
    options.events.lock().unwrap().clone()
}

async fn load(options: Options) -> GotchaResult<ConfigWrapper<Settings>> {
    record(&options.events, "config");
    if options.fail_config {
        return Err(GotchaError::message("config"));
    }
    Ok(ConfigWrapper {
        app: Settings { label: "loaded".into() },
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
    })
}

fn fallback() -> ConfigWrapper<Settings> {
    ConfigWrapper {
        app: Settings { label: "fallback".into() },
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
    }
}

fn policy(options: &Options) -> ConfigErrorPolicy<Settings> {
    if options.fallback {
        ConfigErrorPolicy::Fallback(fallback)
    } else {
        ConfigErrorPolicy::Strict
    }
}

async fn init_state(config: &ConfigWrapper<Settings>, options: Options) -> GotchaResult<AppState> {
    *options.bound.lock().unwrap() = Some(config.server.port);
    record(&options.events, "state");
    if options.fail_at == Some("state") {
        return Err(GotchaError::message("state"));
    }
    Ok(AppState {
        label: config.label.clone(),
        port: config.server.port,
        events: options.events,
    })
}

#[derive(Deserialize, Schematic)]
struct Item {
    name: String,
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn whoami(State(context): State<Context>) -> String {
    format!("{}@{}", context.state.label, context.state.port)
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn create(Json(item): Json<Item>) -> String {
    item.name
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn replace() -> String {
    "replaced".into()
}

/// Records the request, then never answers and ignores the shutdown token.
async fn hang(State(context): State<Context>) -> String {
    record(&context.state.events, "request");
    std::future::pending().await
}

async fn mark_business(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert("x-layer", "business".parse().unwrap());
    response
}

/// The routes both descriptions register through `routes`.
fn routes(router: GotchaRouter<Context>) -> GotchaRouter<Context> {
    let router = router
        .get("/whoami", whoami)
        .post("/items", create)
        .layer(middleware::from_fn(mark_business))
        .get("/hang", hang);
    #[cfg(feature = "openapi")]
    let router = router.openapi(|mut spec| {
        spec.info.description = Some("described both ways".into());
        spec
    });
    router
}

fn spa() -> impl tower::Service<Request, Response = Response, Error = Infallible, Future = impl Future + Send> + Clone + Send + Sync + 'static {
    tower::service_fn(|_: Request| async { Ok::<_, Infallible>(Response::new(Body::from("index.html"))) })
}

/// One function both descriptions share, as applications share their rejection configurations.
fn configs() -> ExtractorConfigs {
    ExtractorConfigs::default().set(JsonConfig::default().error_handler(|_| (StatusCode::UNPROCESSABLE_ENTITY, ErrorBody::new("invalid item"))))
}

/// Mounts a service built from the application state, as a proxy closing over a registry would be.
fn finish(router: Router, context: &Context) -> Router {
    record(&context.state.events, "router");
    let label = context.state.label.clone();
    router.nest_service(
        "/proxy",
        tower::service_fn(move |_: Request| {
            let label = label.clone();
            async move { Ok::<_, Infallible>(Response::new(Body::from(format!("proxied {label}")))) }
        }),
    )
}

async fn register(scheduler: &mut gotcha::TaskScheduler<AppState, Settings>, events: Events, fail: bool) -> GotchaResult<()> {
    tokio::task::yield_now().await; // Registration may await, as the trait method can.
    record(&events, "tasks");
    scheduler.interval("hang", Duration::from_secs(3600), |context| async move {
        record(&context.state.events, "task started");
        std::future::pending::<()>().await;
    });
    if fail {
        return Err(GotchaError::message("tasks"));
    }
    Ok(())
}

async fn signal(stop: Arc<Notify>, events: Events) {
    stop.notified().await;
    record(&events, "signal");
}

#[cfg(feature = "openapi")]
fn info() -> gotcha::oas::Info {
    gotcha::cargo_info!()
}

/// The application described with the trait: every method is overridden.
struct TraitApp(Options);

impl GotchaApp for TraitApp {
    type State = AppState;
    type Config = Settings;

    fn logger(&self) -> GotchaResult<()> {
        record(&self.0.events, "logger");
        Ok(())
    }

    async fn config(&self) -> GotchaResult<ConfigWrapper<Settings>> {
        load(self.0.clone()).await
    }

    fn config_error_policy(&self) -> ConfigErrorPolicy<Settings> {
        policy(&self.0)
    }

    async fn state(&self, config: &ConfigWrapper<Settings>) -> GotchaResult<AppState> {
        init_state(config, self.0.clone()).await
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        routes(router).method_route("/items/{id}", MethodFilter::PUT, replace).fallback_service(spa())
    }

    fn extractor_configs(&self) -> ExtractorConfigs {
        configs()
    }

    fn finish_router(&self, router: Router, context: &Context) -> Router {
        finish(router, context)
    }

    async fn tasks(&self, scheduler: &mut gotcha::TaskScheduler<AppState, Settings>) -> GotchaResult<()> {
        register(scheduler, self.0.events.clone(), self.0.fail_at == Some("tasks")).await
    }

    async fn shutdown_signal(&self) {
        signal(self.0.stop.clone(), self.0.events.clone()).await
    }

    fn http_shutdown_timeout(&self) -> Duration {
        DRAIN
    }

    fn task_shutdown_timeout(&self) -> Duration {
        DRAIN
    }

    async fn on_shutdown(&self, context: &Context) {
        record(&context.state.events, "cleanup");
    }

    #[cfg(feature = "openapi")]
    fn openapi_endpoints(&self) -> Option<gotcha::OpenApiEndpoints> {
        Some(gotcha::OpenApiEndpoints::default())
    }

    #[cfg(feature = "openapi")]
    fn openapi_info(&self) -> gotcha::oas::Info {
        info()
    }

    #[cfg(feature = "openapi")]
    fn document_extractor_errors(&self) -> bool {
        !self.0.hide_extractor_errors
    }

    #[cfg(feature = "prometheus")]
    fn metrics_endpoint(&self) -> Option<gotcha::MetricsEndpoint> {
        Some(gotcha::MetricsEndpoint::default())
    }
}

/// The same application described with the builder.
fn builder(options: Options) -> Gotcha<AppState, Settings> {
    let state_options = options.clone();
    let config_options = options.clone();
    let logger_events = options.events.clone();
    let app = Gotcha::from_state_fn(move |config: Arc<ConfigWrapper<Settings>>| async move { init_state(&config, state_options).await })
        .logger(move || {
            record(&logger_events, "logger");
            Ok(())
        })
        .load_config(move || load(config_options))
        .config_error_policy(policy(&options))
        .routes(routes)
        .method_route("/items/{id}", MethodFilter::PUT, replace)
        .fallback_service(spa())
        .extractor_configs(configs())
        .finish_router(finish)
        .shutdown_signal(signal(options.stop.clone(), options.events.clone()))
        .http_shutdown_timeout(DRAIN)
        .on_shutdown(|context| async move { record(&context.state.events, "cleanup") });
    let app = {
        let (events, fail) = (options.events.clone(), options.fail_at == Some("tasks"));
        app.try_tasks(move |scheduler| Box::pin(register(scheduler, events, fail)))
            .task_shutdown_timeout(DRAIN)
    };
    #[cfg(feature = "openapi")]
    let app = app
        .openapi_endpoints(Some(gotcha::OpenApiEndpoints::default()))
        .openapi_info(info())
        .document_extractor_errors(!options.hide_extractor_errors);
    #[cfg(feature = "prometheus")]
    let app = app.metrics(gotcha::MetricsEndpoint::default());
    app
}

async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), future).await.expect("operation timed out")
}

async fn http_get(address: SocketAddr, path: &str, headers: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n{headers}\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

/// Status, the business-layer marker, and body of an in-memory request.
async fn call(router: &Router, method: &str, path: &str, body: &str) -> (StatusCode, Option<String>, String) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let layer = response.headers().get("x-layer").map(|value| value.to_str().unwrap().to_owned());
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, layer, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn wait_for(options: &Options, event: &'static str) {
    within(async {
        while !events(options).contains(&event) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
}

#[tokio::test]
async fn startup_stages_match() {
    let mut observed = Vec::new();
    for trait_api in [true, false] {
        let options = Options::default();
        let server = if trait_api {
            TraitApp(options.clone()).bind().await.unwrap()
        } else {
            builder(options.clone()).bind().await.unwrap()
        };
        let port = options.bound.lock().unwrap().unwrap();
        assert_eq!(port, server.local_addr().port(), "state sees the port chosen for port 0");
        assert_ne!(port, 0);
        drop(server);
        observed.push(events(&options));
    }
    let expected = vec!["logger", "config", "state", "router", "tasks"];
    assert_eq!(observed, [expected.clone(), expected], "trait, then builder");
}

#[tokio::test]
async fn config_errors_follow_the_same_policy() {
    for trait_api in [true, false] {
        let start = |options: Options| async move {
            if trait_api {
                TraitApp(options).bind().await
            } else {
                builder(options).bind().await
            }
        };
        let strict = Options {
            fail_config: true,
            ..Default::default()
        };
        let Err(error) = start(strict.clone()).await else {
            panic!("a strict policy returns the loader error");
        };
        assert!(matches!(error, GotchaError::Message(message) if message == "config"));
        assert_eq!(events(&strict), ["logger", "config"], "nothing starts after a strict loader error");

        let lenient = Options {
            fail_config: true,
            fallback: true,
            ..Default::default()
        };
        let server = start(lenient.clone()).await.unwrap();
        let address = server.local_addr();
        let stop = lenient.stop.clone();
        let serving = tokio::spawn(server.serve_with_shutdown(async move { stop.notified().await }));
        let response = within(http_get(address, "/whoami", "")).await;
        assert!(response.ends_with(&format!("fallback@{}", address.port())), "{response}");
        lenient.stop.notify_one();
        within(serving).await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn failing_stages_match() {
    for stage in ["state", "tasks"] {
        let mut observed = Vec::new();
        for trait_api in [true, false] {
            let options = Options {
                fail_at: Some(stage),
                ..Default::default()
            };
            let result = if trait_api {
                TraitApp(options.clone()).bind().await
            } else {
                builder(options.clone()).bind().await
            };
            let Err(error) = result else {
                panic!("a failing {stage} stops startup");
            };
            assert!(matches!(error, GotchaError::Message(message) if message == stage));
            let port = options.bound.lock().unwrap().unwrap();
            std::net::TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|error| panic!("a failing {stage} releases the listener: {error}"));
            observed.push(events(&options));
        }
        assert_eq!(observed[0], observed[1], "trait, then builder");
        assert_eq!(observed[0].last(), Some(&stage), "nothing runs after the failure, no task starts");
    }
}

#[tokio::test]
async fn failing_router_assembly_matches() {
    let config = || ConfigWrapper {
        app: (),
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
    };
    struct FailingAssembly;
    impl GotchaApp for FailingAssembly {
        type State = ();
        type Config = ();
        fn logger(&self) -> GotchaResult<()> {
            Ok(())
        }
        async fn config(&self) -> GotchaResult<ConfigWrapper<()>> {
            Ok(ConfigWrapper {
                app: (),
                server: ServerConfig {
                    host: "127.0.0.1".into(),
                    port: 0,
                },
            })
        }
        async fn state(&self, _: &ConfigWrapper<()>) -> GotchaResult<()> {
            Ok(())
        }
        fn routes(&self, router: GotchaRouter<GotchaContext<(), ()>>) -> GotchaRouter<GotchaContext<(), ()>> {
            router
        }
        async fn build_router(&self, _: GotchaContext<(), ()>) -> GotchaResult<Router> {
            Err(GotchaError::message("router"))
        }
    }
    let builder = Gotcha::from_context(GotchaContext::new((), config())).try_finish_router(|_, _| Err(GotchaError::message("router")));
    for result in [FailingAssembly.bind().await, builder.bind().await] {
        assert!(matches!(result, Err(GotchaError::Message(message)) if message == "router"));
    }
}

#[tokio::test]
async fn assembled_routers_match() {
    let context = || {
        let events = Events::default();
        GotchaContext::new(
            AppState {
                label: "memory".into(),
                port: 1234,
                events,
            },
            ConfigWrapper {
                app: Settings { label: "memory".into() },
                server: ServerConfig::default(),
            },
        )
    };
    let trait_router = TraitApp(Options::default()).build_router(context()).await.unwrap();
    let builder_router = builder(Options::default()).into_router(context()).unwrap();

    let mut requests = vec![
        ("GET", "/whoami", ""),
        ("POST", "/items", r#"{"name":"pen"}"#),
        ("POST", "/items", "not json"),
        ("PUT", "/items/1", ""),
        ("GET", "/proxy/status", ""),
        ("GET", "/missing", ""),
    ];
    if cfg!(feature = "openapi") {
        requests.push(("GET", "/scalar", ""));
    }
    let business = Some("business".to_owned());
    let expected = vec![
        (StatusCode::OK, business.clone(), "memory@1234".to_owned()),
        (StatusCode::OK, business.clone(), "pen".to_owned()),
        (StatusCode::UNPROCESSABLE_ENTITY, business, r#"{"error":"invalid item"}"#.to_owned()),
        // Registered after `layer`, so the business layer does not cover it.
        (StatusCode::OK, None, "replaced".to_owned()),
        (StatusCode::OK, None, "proxied memory".to_owned()),
        (StatusCode::OK, None, "index.html".to_owned()),
    ];
    for (router, name) in [(&trait_router, "trait"), (&builder_router, "builder")] {
        let mut responses = Vec::new();
        for (method, path, body) in &requests {
            responses.push(call(router, method, path, body).await);
        }
        if cfg!(feature = "openapi") {
            let (status, _, body) = responses.pop().unwrap();
            assert_eq!(status, StatusCode::OK, "{name} serves the documentation UI");
            assert!(body.contains("/openapi.json"), "{name}: {body}");
        }
        assert_eq!(responses, expected, "{name}");
        #[cfg(feature = "prometheus")]
        {
            // The recorder is process-wide, so only the endpoint itself is compared.
            let (status, _, body) = call(router, "GET", "/metrics", "").await;
            assert_eq!(status, StatusCode::OK, "{name}");
            assert!(body.contains("axum_http_requests_total"), "{name}: {body}");
        }
    }
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn documents_match() {
    let context = || {
        GotchaContext::new(
            AppState {
                label: "docs".into(),
                port: 0,
                events: Events::default(),
            },
            ConfigWrapper {
                app: Settings { label: "docs".into() },
                server: ServerConfig::default(),
            },
        )
    };
    let mut response_counts = Vec::new();
    for hide_extractor_errors in [false, true] {
        let options = || Options {
            hide_extractor_errors,
            ..Default::default()
        };
        let app = TraitApp(options());
        let exported = serde_json::to_value(app.openapi_document()).unwrap();
        assert_eq!(
            serde_json::to_value(app.openapi_document()).unwrap(),
            exported,
            "the trait export is repeatable"
        );
        // A builder is consumed by its export; building it again is the repeatable equivalent.
        for _ in 0..2 {
            assert_eq!(serde_json::to_value(builder(options()).into_openapi()).unwrap(), exported);
        }
        for router in [app.build_router(context()).await.unwrap(), builder(options()).into_router(context()).unwrap()] {
            let (status, _, served) = call(&router, "GET", "/openapi.json", "").await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&served).unwrap(),
                exported,
                "served and exported documents agree"
            );
        }
        assert_eq!(exported["info"]["title"], env!("CARGO_PKG_NAME"), "openapi_info is applied");
        assert_eq!(exported["info"]["description"], "described both ways", "route transforms run after it");
        for (path, method) in [("/whoami", "get"), ("/items", "post"), ("/items/{id}", "put")] {
            assert!(exported["paths"][path][method].is_object(), "{method} {path} in {exported}");
        }
        response_counts.push(exported["paths"]["/items"]["post"]["responses"].as_object().unwrap().len());
    }
    assert!(
        response_counts[0] > response_counts[1],
        "document_extractor_errors(false) drops the rejection responses: {response_counts:?}"
    );
}

#[tokio::test]
async fn shutdown_lifecycles_match() {
    let mut observed = Vec::new();
    for (trait_api, use_run) in [(true, true), (true, false), (false, true), (false, false)] {
        let options = Options::default();
        let serving = match (trait_api, use_run) {
            (true, true) => tokio::spawn(TraitApp(options.clone()).run()),
            (true, false) => tokio::spawn(TraitApp(options.clone()).bind().await.unwrap().serve()),
            (false, true) => tokio::spawn(builder(options.clone()).run()),
            (false, false) => tokio::spawn(builder(options.clone()).bind().await.unwrap().serve()),
        };
        wait_for(&options, "task started").await;
        wait_for(&options, "state").await;
        let port = options.bound.lock().unwrap().unwrap();
        let address: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let client = tokio::spawn(http_get(address, "/hang", ""));
        wait_for(&options, "request").await;
        options.stop.notify_one();
        // Both deadlines are far below their 30-second defaults, and cleanup runs after them.
        within(serving).await.unwrap().unwrap();
        within(client).await.unwrap();
        observed.push(events(&options));
    }
    let expected = vec!["logger", "config", "state", "router", "tasks", "task started", "request", "signal", "cleanup"];
    assert_eq!(observed, vec![expected; 4], "trait run, trait bind, builder run, builder bind");
}

#[tokio::test]
async fn configuration_without_serde_assembles() {
    #[derive(Clone)]
    struct Runtime {
        deployment: &'static str,
    }
    struct App;
    impl GotchaApp for App {
        type State = ();
        type Config = Runtime;
        async fn state(&self, _: &ConfigWrapper<Runtime>) -> GotchaResult<()> {
            Ok(())
        }
        fn routes(&self, router: GotchaRouter<GotchaContext<(), Runtime>>) -> GotchaRouter<GotchaContext<(), Runtime>> {
            router.get(
                "/",
                |State(context): State<GotchaContext<(), Runtime>>| async move { context.config.deployment },
            )
        }
    }
    let context = || {
        GotchaContext::new(
            (),
            ConfigWrapper {
                app: Runtime { deployment: "edge" },
                server: ServerConfig::default(),
            },
        )
    };
    let builder = Gotcha::from_context(context()).get(
        "/",
        |State(context): State<GotchaContext<(), Runtime>>| async move { context.config.deployment },
    );
    for router in [App.build_router(context()).await.unwrap(), builder.into_router(context()).unwrap()] {
        assert_eq!(call(&router, "GET", "/", "").await, (StatusCode::OK, None, "edge".to_owned()));
    }
}

/// A port of a large trait application: getdroppen/droppen `server-rs` implements `config` (a
/// custom loader with overrides), `state`, and `routes`, then adds an embedded-asset proxy, auth
/// and host layers, and its own listener and shutdown around `build_router`.
mod droppen {
    use super::*;
    use gotcha::axum::extract::ConnectInfo;
    use gotcha::config::ConfigBuilder;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Clone, Deserialize)]
    struct DroppenConfig {
        public_name: String,
        auth_token: String,
    }

    #[derive(Clone)]
    struct DroppenState {
        public_url: String,
        token: Arc<String>,
        sessions: Arc<AtomicUsize>,
    }

    type DroppenContext = GotchaContext<DroppenState, DroppenConfig>;

    /// Files and environment first, then the overrides the real loader applies for its flags.
    async fn load(path: PathBuf) -> GotchaResult<ConfigWrapper<DroppenConfig>> {
        let mut config: ConfigWrapper<DroppenConfig> = ConfigBuilder::framework_defaults().file(path).env("DROPPEN_PARITY").build()?;
        config.server.host = "127.0.0.1".into();
        config.server.port = 0;
        Ok(config)
    }

    /// Stands in for opening the database: asynchronous, and it needs the address clients use.
    async fn init(config: Arc<ConfigWrapper<DroppenConfig>>) -> GotchaResult<DroppenState> {
        tokio::task::yield_now().await;
        Ok(DroppenState {
            public_url: format!("http://{}:{}/{}", config.server.host, config.server.port, config.public_name),
            token: Arc::new(config.auth_token.clone()),
            sessions: Arc::default(),
        })
    }

    #[cfg_attr(feature = "openapi", gotcha::api)]
    async fn session(State(context): State<DroppenContext>) -> String {
        let sessions = context.state.sessions.fetch_add(1, Ordering::SeqCst);
        format!("{} #{sessions}", context.state.public_url)
    }

    fn embed_proxy() -> impl tower::Service<Request, Response = Response, Error = Infallible, Future = impl Future + Send> + Clone + Send + Sync + 'static {
        tower::service_fn(|request: Request| async move { Ok::<_, Infallible>(Response::new(Body::from(format!("embedded {}", request.uri().path())))) })
    }

    async fn require_token(State(token): State<Arc<String>>, request: Request, next: Next) -> Response {
        let expected = format!("Bearer {token}");
        if request.headers().get("authorization").and_then(|value| value.to_str().ok()) != Some(&expected) {
            let mut response = Response::new(Body::from("unauthorized"));
            *response.status_mut() = StatusCode::UNAUTHORIZED;
            return response;
        }
        next.run(request).await
    }

    /// Refuses requests without a peer address, as an IP allow-list does.
    async fn require_peer(ConnectInfo(peer): ConnectInfo<SocketAddr>, request: Request, next: Next) -> Response {
        let mut response = next.run(request).await;
        response.headers_mut().insert("x-peer", peer.ip().to_string().parse().unwrap());
        response
    }

    async fn served_by(request: Request, next: Next) -> Response {
        let mut response = next.run(request).await;
        response.headers_mut().insert("x-served-by", "droppen".parse().unwrap());
        response
    }

    /// The documented API: what the trait's `routes` registered.
    fn api(path: PathBuf) -> Gotcha<DroppenState, DroppenConfig> {
        let app = Gotcha::from_state_fn(init)
            .load_config(move || load(path))
            .nest("/api", GotchaRouter::default().get("/session", session))
            .fallback(|| async { (StatusCode::NOT_FOUND, "no such API route") });
        #[cfg(feature = "openapi")]
        let app = app.with_openapi();
        app
    }

    /// Everything the application's own `prepare()` added around `build_router`.
    fn application(path: PathBuf) -> Gotcha<DroppenState, DroppenConfig> {
        api(path)
            .fallback_service(embed_proxy())
            .finish_router(|router, context| router.layer(middleware::from_fn_with_state(context.state.token.clone(), require_token)))
            .app_layer(middleware::from_fn(served_by))
    }

    fn settings() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("droppen.toml");
        std::fs::write(&path, "public_name = 'box'\nauth_token = 'secret'\n").unwrap();
        (dir, path)
    }

    const AUTH: &str = "authorization: Bearer secret\r\n";

    #[tokio::test]
    async fn droppen_shaped_application_runs_on_the_builder() {
        let (_dir, path) = settings();
        let cleaned = Arc::new(AtomicBool::new(false));
        let hook = cleaned.clone();
        let server = application(path)
            .on_shutdown(move |_| async move { hook.store(true, Ordering::SeqCst) })
            .bind()
            .await
            .unwrap();
        let address = server.local_addr();
        assert_ne!(address.port(), 0);
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(server.serve_with_shutdown(async move {
            let _ = stopped.await;
        }));

        let response = within(http_get(address, "/api/session", AUTH)).await;
        assert!(response.starts_with("HTTP/1.0 200"), "{response}");
        assert!(response.contains("x-served-by: droppen"), "{response}");
        assert!(
            response.ends_with(&format!("http://127.0.0.1:{}/box #0", address.port())),
            "state saw the bound port: {response}"
        );
        let response = within(http_get(address, "/assets/app.js", AUTH)).await;
        assert!(response.ends_with("embedded /assets/app.js"), "{response}");
        let response = within(http_get(address, "/api/session", "")).await;
        assert!(response.starts_with("HTTP/1.0 401"), "{response}");
        assert!(response.contains("x-served-by: droppen"), "the host layer covers rejected requests: {response}");

        stop.send(()).unwrap();
        within(serving).await.unwrap().unwrap();
        assert!(cleaned.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn droppen_shaped_tests_drive_the_router_in_memory() {
        let (_dir, path) = settings();
        // The test flow: load, build state, adjust it, then assemble without a listener.
        let mut config = load(path.clone()).await.unwrap();
        config.app.public_name = "test".into();
        let state = init(Arc::new(config.clone())).await.unwrap();
        state.sessions.store(41, Ordering::SeqCst);
        let router = application(path).into_router(GotchaContext::new(state, config)).unwrap();

        let get = |path: &'static str, token: bool| {
            let mut request = Request::builder().uri(path);
            if token {
                request = request.header("authorization", "Bearer secret");
            }
            router.clone().oneshot(request.body(Body::empty()).unwrap())
        };
        let response = get("/api/session", true).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-served-by"], "droppen");
        assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), "http://127.0.0.1:0/test #41");
        assert_eq!(get("/api/session", false).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        let response = get("/index.html", true).await.unwrap();
        assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), "embedded /index.html");
    }

    /// The application's own `prepare()`: install the logger, load the configuration, build the
    /// state before binding (so it sees the configured port), bind, and serve the assembled router
    /// with connect info, which its peer guard reads, after replacing the API's fallback with the
    /// embedded assets. `bind().serve()` serves without connect info on either API, so the guard
    /// there would answer 500: an application whose middleware reads `ConnectInfo` serves the
    /// router itself.
    #[tokio::test]
    async fn droppen_shaped_server_binds_its_own_listener() {
        let (_dir, path) = settings();
        gotcha::default_logger().unwrap();
        let config = load(path.clone()).await.unwrap();
        let state = init(Arc::new(config.clone())).await.unwrap();
        let token = state.token.clone();
        let listener = tokio::net::TcpListener::bind((config.server.host.as_str(), config.server.port)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = api(path.clone())
            .into_router(GotchaContext::new(state, config))
            .unwrap()
            // Replaces the fallback the API registered.
            .fallback_service(embed_proxy())
            .layer(middleware::from_fn_with_state(token, require_token))
            .layer(middleware::from_fn(require_peer))
            .layer(middleware::from_fn(served_by));
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(
            gotcha::axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    let _ = stopped.await;
                })
                .into_future(),
        );

        let response = within(http_get(address, "/api/session", AUTH)).await;
        assert!(response.ends_with("http://127.0.0.1:0/box #0"), "the state saw the configured port: {response}");
        assert!(
            response.contains("x-served-by: droppen") && response.contains("x-peer: 127.0.0.1"),
            "{response}"
        );
        let response = within(http_get(address, "/assets/app.js", AUTH)).await;
        assert!(response.ends_with("embedded /assets/app.js"), "{response}");
        let response = within(http_get(address, "/api/session", "")).await;
        assert!(response.starts_with("HTTP/1.1 401") || response.starts_with("HTTP/1.0 401"), "{response}");
        stop.send(()).unwrap();
        within(serving).await.unwrap().unwrap();

        // Exporting the document needs no server.
        #[cfg(feature = "openapi")]
        {
            let document = serde_json::to_value(api(path).into_openapi()).unwrap();
            assert!(document["paths"]["/api/session"]["get"].is_object(), "{document}");
        }
    }
}
