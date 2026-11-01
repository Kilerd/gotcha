// These tests also cover `GotchaApp`, which stays supported until it is removed.
#![allow(deprecated)]

use gotcha::axum::{
    body::{to_bytes, Body},
    extract::FromRef,
    http::Request,
};
use gotcha::{ConfigWrapper, Gotcha, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, ServerConfig, State};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;

// These consumer types deliberately have no Default or serde implementations.
#[gotcha::state]
#[derive(Clone)]
struct RuntimeState {
    requests: Arc<AtomicUsize>,
}

#[gotcha::config]
#[derive(Clone)]
struct RuntimeConfig {
    label: String,
}

fn context() -> GotchaContext<RuntimeState, RuntimeConfig> {
    GotchaContext::new(
        RuntimeState {
            requests: Arc::new(AtomicUsize::new(7)),
        },
        ConfigWrapper {
            app: RuntimeConfig { label: "ready".into() },
            server: ServerConfig {
                host: "127.0.0.1".into(),
                port: 8123,
            },
        },
    )
}

#[test]
fn cloning_the_context_shares_the_configuration() {
    let context = context();
    let clone = context.clone();
    // axum clones the state per request; the configuration must not be deep-cloned with it.
    assert!(Arc::ptr_eq(&context.config, &clone.config));
    assert_eq!(clone.config.server.port, 8123);
    assert_eq!(clone.config.label, "ready");
}

#[test]
fn cloning_a_context_does_not_require_the_configuration_to_be_clone() {
    struct NotClone;
    let context = GotchaContext::new(
        (),
        ConfigWrapper {
            app: NotClone,
            server: ServerConfig::default(),
        },
    );
    let clone = context.clone();
    assert!(Arc::ptr_eq(&context.config, &clone.config));
}

/// An application's own struct over the context, extracted through `FromRef`.
struct Commands(GotchaContext<RuntimeState, RuntimeConfig>);
impl FromRef<GotchaContext<RuntimeState, RuntimeConfig>> for Commands {
    fn from_ref(context: &GotchaContext<RuntimeState, RuntimeConfig>) -> Self {
        Self(context.clone())
    }
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn read(
    State(state): State<RuntimeState>, State(config): State<RuntimeConfig>, State(wrapper): State<ConfigWrapper<RuntimeConfig>>,
    State(server): State<ServerConfig>, State(commands): State<Commands>,
) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        state.requests.fetch_add(1, Ordering::SeqCst),
        config.label,
        wrapper.label,
        server.port,
        commands.0.config.label
    )
}

struct App;
impl GotchaApp for App {
    type State = RuntimeState;
    type Config = RuntimeConfig;

    async fn state(&self, _: &ConfigWrapper<Self::Config>) -> GotchaResult<Self::State> {
        Ok(context().state)
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/context", read)
    }
}

#[tokio::test]
async fn runtime_context_supports_extractors_and_http_without_loading_bounds() {
    let context = context();
    let requests = context.state.requests.clone();
    let router = App.build_router(context).await.unwrap();
    let response = router.oneshot(Request::builder().uri("/context").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), "7:ready:ready:8123:ready");
    assert_eq!(requests.load(Ordering::SeqCst), 8);
}

#[test]
fn initialized_builders_can_reach_every_serving_entry_point() {
    fn assert_send<T: Send>(_: T) {}
    fn app() -> Gotcha<RuntimeState, RuntimeConfig> {
        Gotcha::from_context(context())
            .state(context().state)
            .config(std::sync::Arc::try_unwrap(context().config).unwrap_or_else(|config| (*config).clone()))
            .get("/context", read)
            .tasks(|scheduler| {
                scheduler.interval("read-config", std::time::Duration::from_secs(60), |context| async move {
                    let _ = context.config.label;
                });
            })
    }
    // Type-check the public startup futures without opening a listener.
    assert_send(app().listen("127.0.0.1:0"));
    assert_send(app().listen_on("127.0.0.1:0".parse().unwrap()));
    assert_send(app().run());
    assert_send(Gotcha::from_state(context().state).run());
}

#[test]
fn config_operations_only_require_the_traits_they_use() {
    #[derive(Clone, serde::Deserialize)]
    struct LoadOnly {
        label: String,
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "label = 'loaded'").unwrap();
    let loaded: ConfigWrapper<LoadOnly> = gotcha::config::Config::builder().file(&path).build().unwrap();
    assert_eq!(loaded.label, "loaded");
    assert_eq!(loaded.server.port, 3000);
    // Explicit loading on the builder needs Deserialize, but neither Serialize nor Default.
    let _ = Gotcha::from_context(GotchaContext::new((), loaded))
        .build_config(|builder| builder.file(&path))
        .unwrap();

    #[derive(serde::Serialize)]
    struct SaveOnly<'a> {
        label: &'a str,
    }
    let label = String::from("borrowed");
    let wrapper = ConfigWrapper {
        app: SaveOnly { label: &label },
        server: ServerConfig::default(),
    };
    let serialized = serde_json::to_value(wrapper).unwrap();
    assert_eq!(serialized["label"], "borrowed");
    assert_eq!(serialized["server"]["port"], 3000);
}
