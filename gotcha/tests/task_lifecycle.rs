#![cfg(feature = "task")]

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
        if matches!(self.registration, Registration::Normal) {
            register(scheduler);
            return Ok(());
        }
        let resource = Dropped(self.controls.dropped.clone());
        scheduler.interval("partial-registration", Duration::from_millis(1), move |ctx| {
            let _ = &resource;
            ctx.state.executions.fetch_add(1, Ordering::SeqCst);
            async {}
        });
        self.controls.registered.notify_one();
        tokio::task::yield_now().await; // An eagerly spawned registration would run here.
        match self.registration {
            Registration::Fail => Err(GotchaError::message("registration failed")),
            Registration::Wait => std::future::pending().await,
            Registration::Normal => unreachable!(),
        }
    }
}

async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("lifecycle operation timed out")
}

async fn request(address: SocketAddr) -> String {
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream.write_all(b"GET /hold HTTP/1.0\r\nHost: localhost\r\n\r\n").await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
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
                    Gotcha::from_context(GotchaContext {
                        state: controls.clone(),
                        config: config(),
                    })
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
    for registration in [Registration::Fail, Registration::Wait] {
        let (controls, mut addresses, _starts, mut drops) = controls();
        let app = App {
            controls: controls.clone(),
            timeout: Duration::from_secs(30),
            registration,
        };
        let server = tokio::spawn(app.run());
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

#[tokio::test]
async fn builder_registration_panic_discards_previous_registrations() {
    let (controls, _addresses, _starts, mut drops) = controls();
    let dropped = Dropped(controls.dropped.clone());
    let app = Gotcha::from_context(GotchaContext {
        state: controls.clone(),
        config: config(),
    })
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
