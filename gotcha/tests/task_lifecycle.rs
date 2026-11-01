// These tests also cover `GotchaApp`, which stays supported until it is removed.
#![allow(deprecated)]

use gotcha::{ConfigWrapper, Gotcha, GotchaApp, GotchaContext, GotchaError, GotchaResult, GotchaRouter, ServerConfig, State, TaskScheduler};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Notify};

#[derive(Clone)]
struct Controls {
    addresses: mpsc::UnboundedSender<SocketAddr>,
    started: mpsc::UnboundedSender<()>,
    dropped: mpsc::UnboundedSender<()>,
    stop: Arc<Notify>,
    stopping: Arc<Notify>,
    registered: Arc<Notify>,
    finish_task: Arc<Notify>,
    http_started: Arc<Notify>,
    finish_http: Arc<Notify>,
    executions: Arc<AtomicUsize>,
    completions: Arc<AtomicUsize>,
}

fn controls() -> (
    Controls,
    mpsc::UnboundedReceiver<SocketAddr>,
    mpsc::UnboundedReceiver<()>,
    mpsc::UnboundedReceiver<()>,
) {
    let (addresses, address_rx) = mpsc::unbounded_channel();
    let (started, start_rx) = mpsc::unbounded_channel();
    let (dropped, drop_rx) = mpsc::unbounded_channel();
    (
        Controls {
            addresses,
            started,
            dropped,
            stop: Arc::default(),
            stopping: Arc::default(),
            registered: Arc::default(),
            finish_task: Arc::default(),
            http_started: Arc::default(),
            finish_http: Arc::default(),
            executions: Arc::default(),
            completions: Arc::default(),
        },
        address_rx,
        start_rx,
        drop_rx,
    )
}

struct Dropped(mpsc::UnboundedSender<()>);
impl Drop for Dropped {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

fn config() -> ConfigWrapper<()> {
    ConfigWrapper {
        app: (),
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
    }
}

fn address(config: &ConfigWrapper<()>) -> SocketAddr {
    SocketAddr::new(config.server.host.parse().unwrap(), config.server.port)
}

fn register(scheduler: &mut TaskScheduler<Controls, ()>) {
    scheduler.interval("work", Duration::from_millis(1), |ctx| async move {
        let _resource = Dropped(ctx.state.dropped.clone());
        ctx.state.executions.fetch_add(1, Ordering::SeqCst);
        ctx.state.addresses.send(address(&ctx.config)).unwrap();
        ctx.state.started.send(()).unwrap();
        ctx.state.finish_task.notified().await;
        ctx.state.completions.fetch_add(1, Ordering::SeqCst);
    });
}

async fn hold(State(ctx): State<GotchaContext<Controls, ()>>) -> &'static str {
    ctx.state.http_started.notify_one();
    ctx.state.finish_http.notified().await;
    "drained"
}

#[derive(Clone, Copy)]
enum Registration {
    Normal,
    Fail,
    Wait,
    #[cfg(feature = "task")]
    InvalidCron,
}

struct App {
    controls: Controls,
    timeout: Duration,
    registration: Registration,
}
impl GotchaApp for App {
    type State = Controls;
    type Config = ();
    fn logger(&self) -> GotchaResult<()> {
        Ok(())
    }
    async fn config(&self) -> GotchaResult<ConfigWrapper<()>> {
        Ok(config())
    }
    async fn state(&self, config: &ConfigWrapper<()>) -> GotchaResult<Controls> {
        self.controls.addresses.send(address(config)).unwrap();
        Ok(self.controls.clone())
    }
    fn routes(&self, router: GotchaRouter<GotchaContext<Controls, ()>>) -> GotchaRouter<GotchaContext<Controls, ()>> {
        router.get("/hold", hold)
    }
    async fn shutdown_signal(&self) {
        // This future borrows the application; no added 'static or Clone bound is needed.
        self.controls.stop.notified().await;
        self.controls.stopping.notify_one();
    }
    fn task_shutdown_timeout(&self) -> Duration {
        self.timeout
    }
    async fn tasks(&self, scheduler: &mut TaskScheduler<Controls, ()>) -> GotchaResult<()> {
        register_as(self.controls.clone(), self.registration, scheduler).await
    }
}

/// The registration both APIs run: the trait's `tasks` and the builder's `try_tasks`.
async fn register_as(controls: Controls, registration: Registration, scheduler: &mut TaskScheduler<Controls, ()>) -> GotchaResult<()> {
    if matches!(registration, Registration::Normal) {
        register(scheduler);
        return Ok(());
    }
    let resource = Dropped(controls.dropped.clone());
    scheduler.interval("partial-registration", Duration::from_millis(1), move |ctx| {
        let _ = &resource;
        ctx.state.executions.fetch_add(1, Ordering::SeqCst);
        async {}
    });
    controls.registered.notify_one();
    tokio::task::yield_now().await; // An eagerly spawned registration would run here.
    match registration {
        Registration::Fail => Err(GotchaError::message("registration failed")),
        Registration::Wait => std::future::pending().await,
        #[cfg(feature = "task")]
        Registration::InvalidCron => {
            scheduler.cron("bad-cron", "not a schedule".into(), |_| async {});
            Ok(())
        }
        Registration::Normal => unreachable!(),
    }
}

/// The builder equivalent of [`App`]: state from the loaded configuration, fallible async tasks.
fn builder(controls: Controls, registration: Registration) -> Gotcha<Controls, ()> {
    let state = controls.clone();
    Gotcha::from_state_fn(move |config: Arc<ConfigWrapper<()>>| async move {
        state.addresses.send(address(&config)).unwrap();
        Ok(state)
    })
    .load_config(|| async { Ok(config()) })
    .get("/hold", hold)
    .try_tasks(move |scheduler| Box::pin(register_as(controls, registration, scheduler)))
}

async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("lifecycle operation timed out")
}

async fn request(address: SocketAddr) -> String {
    request_path(address, "/hold").await
}

async fn request_path(address: SocketAddr, path: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

async fn stop_scheduler(State(context): State<GotchaContext<Controls, ()>>) -> &'static str {
    let mut scheduler = TaskScheduler::new(context);
    scheduler.interval("detached", Duration::from_millis(1), |_| async {});
    scheduler.start().shutdown(Duration::from_secs(1)).await;
    "stopped"
}

#[tokio::test]
async fn standalone_scheduler_shutdown_in_a_handler_does_not_stop_the_server() {
    let (controls, _addresses, _starts, _drops) = controls();
    let stop = controls.stop.clone();
    let server = Gotcha::from_context(GotchaContext::new(controls.clone(), config()))
        .get("/stop-scheduler", stop_scheduler)
        .get("/ping", || async { "pong" })
        .shutdown_signal(async move {
            stop.notified().await;
        })
        .bind()
        .await
        .unwrap();
    let address = server.local_addr();
    let serving = tokio::spawn(server.serve());

    // The handler builds a standalone scheduler from the request's context and shuts it down.
    assert!(within(request_path(address, "/stop-scheduler")).await.ends_with("stopped"));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!serving.is_finished(), "a standalone scheduler shutdown must not stop the server");
    assert!(within(request_path(address, "/ping")).await.ends_with("pong"));

    controls.stop.notify_one();
    within(serving).await.unwrap().unwrap();
}

#[tokio::test]
async fn both_public_apis_share_http_and_task_shutdown_and_own_current_executions() {
    for trait_api in [false, true] {
        for mode in ["finish", "timeout", "abort"] {
            let (controls, mut addresses, mut starts, mut drops) = controls();
            let timeout = if mode == "timeout" {
                Duration::from_millis(10)
            } else {
                Duration::from_secs(30)
            };
            let server = if trait_api {
                tokio::spawn(
                    App {
                        controls: controls.clone(),
                        timeout,
                        registration: Registration::Normal,
                    }
                    .run(),
                )
            } else {
                let stop = controls.stop.clone();
                let stopping = controls.stopping.clone();
                tokio::spawn(
                    Gotcha::from_context(GotchaContext::new(controls.clone(), config()))
                        .get("/hold", hold)
                        .tasks(register)
                        .shutdown_signal(async move {
                            stop.notified().await;
                            stopping.notify_one();
                        })
                        .task_shutdown_timeout(timeout)
                        .run(),
                )
            };
            let address = within(addresses.recv()).await.unwrap();
            within(starts.recv()).await.unwrap();
            let client = tokio::spawn(request(address));
            within(controls.http_started.notified()).await;
            if mode == "abort" {
                server.abort();
                assert!(within(server).await.unwrap_err().is_cancelled());
                within(drops.recv()).await.unwrap();
                controls.finish_http.notify_one();
                assert!(within(client).await.unwrap().ends_with("drained"));
            } else {
                controls.stop.notify_one();
                within(controls.stopping.notified()).await;
                if mode == "timeout" {
                    // The task deadline must run while the HTTP request is still draining.
                    within(drops.recv()).await.unwrap();
                    assert_eq!(controls.completions.load(Ordering::SeqCst), 0);
                    assert!(!server.is_finished());
                    controls.finish_http.notify_one();
                    assert!(within(client).await.unwrap().ends_with("drained"));
                } else {
                    controls.finish_http.notify_one();
                    assert!(within(client).await.unwrap().ends_with("drained"));
                    tokio::task::yield_now().await;
                    assert!(!server.is_finished(), "server must wait for the current scheduled execution");
                    controls.finish_task.notify_one();
                }
                within(server).await.unwrap().unwrap();
                if mode == "finish" {
                    assert_eq!(drops.try_recv(), Ok(()), "shutdown must join before returning");
                    assert_eq!(controls.completions.load(Ordering::SeqCst), 1);
                }
            }
            assert_eq!(controls.executions.load(Ordering::SeqCst), 1, "shutdown must stop subsequent executions");
        }
    }
}

#[tokio::test]
async fn failed_or_cancelled_registration_discards_tasks_and_releases_the_listener() {
    for (trait_api, registration) in [
        (true, Registration::Fail),
        (true, Registration::Wait),
        (false, Registration::Fail),
        (false, Registration::Wait),
    ] {
        let (controls, mut addresses, _starts, mut drops) = controls();
        let server = if trait_api {
            let app = App {
                controls: controls.clone(),
                timeout: Duration::from_secs(30),
                registration,
            };
            tokio::spawn(app.run())
        } else {
            tokio::spawn(builder(controls.clone(), registration).run())
        };
        let address = within(addresses.recv()).await.unwrap();
        within(controls.registered.notified()).await;
        if matches!(registration, Registration::Wait) {
            server.abort();
            assert!(within(server).await.unwrap_err().is_cancelled());
        } else {
            assert!(matches!(within(server).await.unwrap(), Err(GotchaError::Message(message)) if message == "registration failed"));
        }
        assert_eq!(drops.try_recv(), Ok(()));
        assert_eq!(controls.executions.load(Ordering::SeqCst), 0);
        let _rebound = tokio::net::TcpListener::bind(address).await.unwrap();
    }
}

#[cfg(feature = "task")]
#[tokio::test]
async fn invalid_registrations_fail_startup_and_release_the_listener() {
    let (app_controls, mut addresses, _starts, mut drops) = controls();
    let app = App {
        controls: app_controls.clone(),
        timeout: Duration::from_secs(30),
        registration: Registration::InvalidCron,
    };
    let server = tokio::spawn(app.run());
    let address = within(addresses.recv()).await.unwrap();
    let error = within(server).await.unwrap().unwrap_err();
    let message = error.to_string();
    assert!(message.contains("bad-cron") && message.contains("not a schedule"), "{message}");
    assert_eq!(drops.try_recv(), Ok(()));
    assert_eq!(app_controls.executions.load(Ordering::SeqCst), 0);
    let _rebound = tokio::net::TcpListener::bind(address).await.unwrap();

    // The builder path reports the same registration error from `bind()`.
    let (builder_controls, _addresses, _starts, _drops) = controls();
    let error = within(
        Gotcha::from_context(GotchaContext::new(builder_controls, config()))
            .tasks(|scheduler| {
                scheduler.cron("bad-cron", "not a schedule".into(), |_| async {});
            })
            .bind(),
    )
    .await
    .err()
    .expect("an invalid cron expression must fail bind");
    let message = error.to_string();
    assert!(message.contains("bad-cron") && message.contains("not a schedule"), "{message}");
}

/// A zero interval is the same class of silently dropped registration as an invalid cron.
#[tokio::test]
async fn zero_interval_fails_startup() {
    let (zero_controls, _addresses, _starts, _drops) = controls();
    let error = within(
        Gotcha::from_context(GotchaContext::new(zero_controls, config()))
            .tasks(|scheduler| {
                scheduler.interval("bad-interval", Duration::ZERO, |_| async {});
            })
            .bind(),
    )
    .await
    .err()
    .expect("a zero interval must fail bind");
    let message = error.to_string();
    assert!(message.contains("bad-interval") && message.contains("zero interval"), "{message}");
}

#[tokio::test]
async fn builder_async_registration_starts_its_tasks_with_serving() {
    let (controls, mut addresses, mut starts, _drops) = controls();
    let (stop, stopping) = (controls.stop.clone(), controls.stopping.clone());
    let server = tokio::spawn(
        builder(controls.clone(), Registration::Normal)
            .shutdown_signal(async move {
                stop.notified().await;
                stopping.notify_one();
            })
            .run(),
    );
    let bound = within(addresses.recv()).await.unwrap();
    within(starts.recv()).await.unwrap();
    assert_eq!(
        within(addresses.recv()).await.unwrap(),
        bound,
        "the task sees the address the state was built with"
    );
    // Release the execution only after shutdown is requested, so no further tick starts one.
    controls.stop.notify_one();
    within(controls.stopping.notified()).await;
    controls.finish_task.notify_one();
    within(server).await.unwrap().unwrap();
}

#[tokio::test]
async fn builder_registration_panic_discards_previous_registrations() {
    let (controls, _addresses, _starts, mut drops) = controls();
    let dropped = Dropped(controls.dropped.clone());
    let app = Gotcha::from_context(GotchaContext::new(controls.clone(), config()))
        .tasks(move |scheduler| {
            scheduler.interval("before-panic", Duration::from_millis(1), move |ctx| {
                let _ = &dropped;
                ctx.state.executions.fetch_add(1, Ordering::SeqCst);
                async {}
            });
        })
        .tasks(|_| panic!("registration panic"));
    assert!(within(tokio::spawn(app.run())).await.unwrap_err().is_panic());
    assert_eq!(drops.try_recv(), Ok(()));
    assert_eq!(controls.executions.load(Ordering::SeqCst), 0);
}
