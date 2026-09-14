use super::*;
use crate::config::ConfigBuilder;
use crate::{Gotcha, GotchaRouter, State};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn listen_address_precedence_preserves_explicit_default_values() {
    let config = ServerConfig {
        host: "0.0.0.0".into(),
        port: 8080,
    };
    for (host, port, explicit, expected) in [
        (None, None, None, "0.0.0.0:8080"),
        (Some("127.0.0.1"), None, None, "127.0.0.1:8080"),
        (None, Some(3000), None, "0.0.0.0:3000"),
        (Some("127.0.0.1"), Some(3000), None, "127.0.0.1:3000"),
        (None, Some(0), None, "0.0.0.0:0"),
        (Some("::1"), Some(8081), None, "[::1]:8081"),
        (Some("invalid"), Some(3000), Some("127.0.0.1:9090"), "127.0.0.1:9090"),
    ] {
        let options = ListenOptions {
            host: host.map(str::to_owned),
            port,
        };
        assert_eq!(
            options.resolve(&config, explicit.map(|value| value.parse().unwrap())).unwrap(),
            expected.parse::<SocketAddr>().unwrap()
        );
    }
    assert_eq!(
        ListenOptions::default().resolve(&ServerConfig::default(), None).unwrap(),
        "127.0.0.1:3000".parse::<SocketAddr>().unwrap()
    );
    let invalid = ListenOptions {
        host: Some("not-an-ip".into()),
        port: None,
    };
    assert!(matches!(invalid.resolve(&config, None), Err(GotchaError::InvalidAddress(_))));
}

// Loading a configuration no longer requires Serialize or Default.
#[derive(Clone, serde::Deserialize)]
struct Settings {
    label: String,
}

type Events = Arc<Mutex<Vec<&'static str>>>;

struct TraitApp {
    source: ConfigBuilder,
    fallback: bool,
    fail_at: Option<&'static str>,
    events: Events,
    bound: Mutex<Option<SocketAddr>>,
}

impl TraitApp {
    fn new(path: &Path) -> Self {
        Self {
            source: ConfigBuilder::new().file(path),
            fallback: false,
            fail_at: None,
            events: Events::default(),
            bound: Mutex::new(None),
        }
    }

    fn stage(&self, name: &'static str) -> GotchaResult<()> {
        self.events.lock().unwrap().push(name);
        if self.fail_at == Some(name) {
            return Err(GotchaError::message(name));
        }
        Ok(())
    }
}

async fn read(State(config): State<ConfigWrapper<Settings>>) -> String {
    format!("{}:{}:{}", config.label, config.server.host, config.server.port)
}

fn fallback_settings() -> ConfigWrapper<Settings> {
    ConfigWrapper {
        app: Settings { label: "fallback".into() },
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
    }
}

impl GotchaApp for TraitApp {
    type State = ();
    type Config = Settings;

    fn logger(&self) -> GotchaResult<()> {
        self.stage("logger")
    }

    fn config_error_policy(&self) -> ConfigErrorPolicy<Settings> {
        if self.fallback {
            ConfigErrorPolicy::Fallback(fallback_settings)
        } else {
            ConfigErrorPolicy::Strict
        }
    }

    async fn config(&self) -> GotchaResult<ConfigWrapper<Settings>> {
        self.stage("config")?;
        Ok(self.source.clone().build()?)
    }

    async fn state(&self, config: &ConfigWrapper<Settings>) -> GotchaResult<()> {
        assert_ne!(config.server.port, 0, "state must see the actual port before initialization");
        *self.bound.lock().unwrap() = Some(SocketAddr::new(config.server.host.parse().unwrap(), config.server.port));
        self.stage("state")
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), Settings>>) -> GotchaRouter<GotchaContext<(), Settings>> {
        router.get("/config", read)
    }

    async fn build_router(&self, context: GotchaContext<(), Settings>) -> GotchaResult<Router> {
        self.stage("router")?;
        Ok(self.routes(GotchaRouter::default()).into_axum_router(context))
    }

    #[cfg(feature = "task")]
    async fn tasks(&self, _: &mut crate::TaskScheduler<(), Settings>) -> GotchaResult<()> {
        self.stage("tasks")
    }
}

async fn assert_http(prepared: PreparedServer, label: &str) {
    let address = prepared.listener.local_addr().unwrap();
    let server = tokio::spawn(prepared.serve(std::future::pending()));
    let response = tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.write_all(b"GET /config HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    })
    .await;
    server.abort();
    let _ = server.await;
    let response = response.unwrap();
    assert!(response.starts_with("HTTP/1.0 200"), "{response}");
    assert!(response.ends_with(&format!("{label}:{}:{}", address.ip(), address.port())), "{response}");
}

#[tokio::test]
async fn both_apis_serve_the_loaded_config_and_actual_bound_address() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "label = 'loaded'\n[server]\nhost = '127.0.0.1'\nport = 0").unwrap();
    let app = TraitApp::new(&path);
    let mut builder = Gotcha::with_config::<Settings>().with_file_config(&path).get("/config", read);
    let prepared = prepare(&mut builder, None).await.unwrap();
    assert_http(prepared, "loaded").await;
    let prepared = prepare(&mut &app, None).await.unwrap();
    assert_eq!(*app.bound.lock().unwrap(), Some(prepared.listener.local_addr().unwrap()));
    assert_http(prepared, "loaded").await;
    let mut expected = vec!["logger", "config", "state", "router"];
    if cfg!(feature = "task") {
        expected.push("tasks");
    }
    assert_eq!(*app.events.lock().unwrap(), expected);
}

#[tokio::test]
async fn run_uses_configured_address_and_does_not_initialize_after_bind_failure() {
    let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = occupied.local_addr().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, format!("label = 'occupied'\n[server]\nhost = '127.0.0.1'\nport = {}", address.port())).unwrap();
    let app = TraitApp::new(&path);
    let events = app.events.clone();
    let builder = Gotcha::with_config::<Settings>().with_file_config(&path);
    #[cfg(feature = "task")]
    let builder = builder.tasks(|_| panic!("must bind before task registration"));
    let builder_error = tokio::time::timeout(Duration::from_secs(2), builder.run()).await.unwrap().unwrap_err();
    let trait_error = tokio::time::timeout(Duration::from_secs(2), GotchaApp::run(app)).await.unwrap().unwrap_err();
    for error in [builder_error, trait_error] {
        assert!(matches!(error, GotchaError::Bind { addr, .. } if addr == address.to_string()));
    }
    assert_eq!(*events.lock().unwrap(), ["logger", "config"]);
    // An explicit builder override can use the same configuration on an available port.
    let mut builder = Gotcha::with_config::<Settings>().with_file_config(&path).port(0).get("/config", read);
    assert_http(prepare(&mut builder, None).await.unwrap(), "occupied").await;
}

#[tokio::test]
async fn configuration_fallback_is_explicit_and_shared_by_both_apis() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "label = [").unwrap();
    let mut app = TraitApp::new(&path);
    let builder = || Gotcha::with_config::<Settings>().with_file_config(&path).get("/config", read);
    assert!(matches!(prepare(&mut builder(), None).await, Err(GotchaError::Config(_))));
    assert!(matches!(prepare(&mut &app, None).await, Err(GotchaError::Config(_))));
    assert_eq!(*app.events.lock().unwrap(), ["logger", "config"]);
    app.fallback = true;
    assert_http(
        prepare(&mut builder().config_error_policy(ConfigErrorPolicy::Fallback(fallback_settings)), None)
            .await
            .unwrap(),
        "fallback",
    )
    .await;
    assert_http(prepare(&mut &app, None).await.unwrap(), "fallback").await;
}

#[tokio::test]
async fn initialization_errors_release_the_listener_and_stop_later_stages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "label = 'ready'\n[server]\nhost = '127.0.0.1'\nport = 0").unwrap();
    let mut stages = vec!["state", "router"];
    if cfg!(feature = "task") {
        stages.push("tasks");
    }
    for stage in stages {
        let mut app = TraitApp::new(&path);
        app.fail_at = Some(stage);
        app.fallback = true; // A config fallback must not swallow errors from other stages.
        assert!(matches!(prepare(&mut &app, None).await, Err(GotchaError::Message(message)) if message == stage));
        let bound = app.bound.lock().unwrap().unwrap();
        let _rebound = TcpListener::bind(bound).await.expect("failed startup must release its listener");
        assert_eq!(app.events.lock().unwrap().last(), Some(&stage));
    }
}
