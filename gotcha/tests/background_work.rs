//! Work registered with `TaskScheduler::spawn` joins the application's lifecycle. Nothing here
//! needs the `task` feature, so these also run in the builds without it.

use gotcha::{ConfigWrapper, Gotcha, GotchaApp, GotchaContext, GotchaError, GotchaResult, GotchaRouter, ServerConfig, TaskScheduler};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Notify};

type Events = Arc<Mutex<Vec<&'static str>>>;

fn config() -> ConfigWrapper<()> {
    ConfigWrapper {
        app: (),
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
    }
}

async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("lifecycle operation timed out")
}

async fn ping(address: SocketAddr) -> String {
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream.write_all(b"GET /ping HTTP/1.0\r\nHost: localhost\r\n\r\n").await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

/// Records its event when the future holding it is dropped.
struct OnDrop(&'static str, Events);
impl Drop for OnDrop {
    fn drop(&mut self) {
        self.1.lock().unwrap().push(self.0);
    }
}

/// Consumes until shutdown, then does more awaited work. `on_shutdown` must see all of it.
fn consumer(scheduler: &mut TaskScheduler<(), ()>, events: Events) {
    scheduler.spawn("consumer", |ctx| async move {
        let shutdown = ctx.shutdown();
        let mut poll = tokio::time::interval(Duration::from_millis(1));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = poll.tick() => {}
            }
        }
        events.lock().unwrap().push("shutdown observed");
        tokio::time::sleep(Duration::from_millis(50)).await;
        events.lock().unwrap().push("consumer drained");
    });
}

struct App {
    events: Events,
    stop: Arc<Notify>,
}
impl GotchaApp for App {
    type State = ();
    type Config = ();
    fn logger(&self) -> GotchaResult<()> {
        Ok(())
    }
    async fn config(&self) -> GotchaResult<ConfigWrapper<()>> {
        Ok(config())
    }
    async fn state(&self, _: &ConfigWrapper<()>) -> GotchaResult<()> {
        Ok(())
    }
    fn routes(&self, router: GotchaRouter<GotchaContext<(), ()>>) -> GotchaRouter<GotchaContext<(), ()>> {
        router
    }
    async fn shutdown_signal(&self) {
        self.stop.notified().await;
    }
    async fn on_shutdown(&self, _: &GotchaContext<(), ()>) {
        self.events.lock().unwrap().push("on_shutdown");
    }
    async fn tasks(&self, scheduler: &mut TaskScheduler<(), ()>) -> GotchaResult<()> {
        consumer(scheduler, self.events.clone());
        Ok(())
    }
}

fn builder(events: &Events) -> Gotcha<(), ()> {
    let hook = events.clone();
    Gotcha::from_context(GotchaContext::new((), config())).on_shutdown(move |_| async move {
        hook.lock().unwrap().push("on_shutdown");
    })
}

#[tokio::test]
async fn consumer_drains_before_on_shutdown_hooks_in_every_serving_path() {
    for path in ["builder run", "builder bind", "trait run", "trait bind"] {
        let events = Events::default();
        let stop = Arc::new(Notify::new());
        let signal = stop.clone();
        let registered = events.clone();
        let app = builder(&events)
            .tasks(move |scheduler| consumer(scheduler, registered))
            .shutdown_signal(async move { signal.notified().await });
        let trait_app = App {
            events: events.clone(),
            stop: stop.clone(),
        };
        let server = match path {
            "builder run" => tokio::spawn(app.run()),
            "builder bind" => tokio::spawn(app.bind().await.unwrap().serve()),
            "trait run" => tokio::spawn(trait_app.run()),
            _ => tokio::spawn(trait_app.bind().await.unwrap().serve()),
        };
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(events.lock().unwrap().is_empty(), "{path}: the consumer runs until shutdown");
        stop.notify_one();
        within(server).await.unwrap().unwrap();
        assert_eq!(*events.lock().unwrap(), ["shutdown observed", "consumer drained", "on_shutdown"], "{path}");
    }
}

#[tokio::test]
async fn future_ignoring_the_token_is_aborted_at_the_deadline() {
    let events = Events::default();
    let stop = Arc::new(Notify::new());
    let (registered, signal) = (events.clone(), stop.clone());
    let (started, mut starts) = mpsc::unbounded_channel();
    let server = tokio::spawn(
        builder(&events)
            .tasks(move |scheduler| {
                scheduler.spawn("stubborn", move |_| async move {
                    let _resource = OnDrop("aborted", registered.clone());
                    started.send(()).unwrap();
                    std::future::pending::<()>().await;
                    registered.lock().unwrap().push("returned");
                });
            })
            .task_shutdown_timeout(Duration::from_millis(20))
            .shutdown_signal(async move { signal.notified().await })
            .run(),
    );
    within(starts.recv()).await.unwrap();
    stop.notify_one();
    within(server).await.unwrap().unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        ["aborted", "on_shutdown"],
        "the future is dropped at the deadline, before cleanup"
    );
}

#[tokio::test]
async fn panicking_spawned_future_leaves_other_tasks_and_the_server_running() {
    let events = Events::default();
    let stop = Arc::new(Notify::new());
    let signal = stop.clone();
    let check = Arc::new(Notify::new());
    let requested = check.clone();
    let (alive, mut alive_rx) = mpsc::unbounded_channel();
    let (panicked, mut panics) = mpsc::unbounded_channel();
    let (polling_panicked, construction_panicked) = (panicked.clone(), panicked);
    let app = builder(&events)
        .get("/ping", || async { "pong" })
        .tasks(move |scheduler| {
            scheduler.spawn("panics-polled", move |_| async move {
                polling_panicked.send(()).unwrap();
                panic!("polling panic");
            });
            scheduler.spawn("panics-constructed", move |_| -> std::future::Ready<()> {
                construction_panicked.send(()).unwrap();
                panic!("construction panic");
            });
            scheduler.spawn("survivor", move |_| async move {
                requested.notified().await;
                alive.send(()).unwrap();
            });
        })
        .shutdown_signal(async move { signal.notified().await });
    let server = app.bind().await.unwrap();
    let address = server.local_addr();
    let serving = tokio::spawn(server.serve());
    within(panics.recv()).await.unwrap();
    within(panics.recv()).await.unwrap();
    // Both panics have started unwinding; let them finish before checking what survived.
    tokio::task::yield_now().await;
    check.notify_one();
    within(alive_rx.recv()).await.unwrap();
    assert!(within(ping(address)).await.ends_with("pong"));
    assert!(!serving.is_finished(), "a panicking task must not stop the server");
    stop.notify_one();
    within(serving).await.unwrap().expect("a task panic is not a server error");
    assert_eq!(*events.lock().unwrap(), ["on_shutdown"], "cleanup still runs");
}

#[tokio::test]
async fn spawned_future_can_run_to_completion_without_stopping_the_server() {
    let events = Events::default();
    let stop = Arc::new(Notify::new());
    let signal = stop.clone();
    let (finished, mut finishes) = mpsc::unbounded_channel();
    let app = builder(&events)
        .get("/ping", || async { "pong" })
        .tasks(move |scheduler| {
            scheduler.spawn("migration", move |_| async move {
                finished.send(()).unwrap();
            });
        })
        // A finished future must not hold the drain: the test's deadline is much shorter.
        .task_shutdown_timeout(Duration::from_secs(60))
        .shutdown_signal(async move { signal.notified().await });
    let server = app.bind().await.unwrap();
    let address = server.local_addr();
    let serving = tokio::spawn(server.serve());
    within(finishes.recv()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(within(ping(address)).await.ends_with("pong"));
    assert!(!serving.is_finished(), "completing is not a shutdown");
    assert!(finishes.try_recv().is_err(), "a finished future must not restart");
    stop.notify_one();
    within(serving).await.unwrap().unwrap();
    assert_eq!(*events.lock().unwrap(), ["on_shutdown"]);
}

#[tokio::test]
async fn spawned_future_starts_with_serving_and_not_after_a_failed_registration() {
    let events = Events::default();
    let stop = Arc::new(Notify::new());
    let signal = stop.clone();
    let (started, mut starts) = mpsc::unbounded_channel();
    let bound = builder(&events)
        .tasks(move |scheduler| {
            scheduler.spawn("consumer", move |_| async move {
                started.send(()).unwrap();
            });
        })
        .shutdown_signal(async move { signal.notified().await })
        .bind()
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(starts.try_recv().is_err(), "binding must not start background work");
    let serving = tokio::spawn(bound.serve());
    within(starts.recv()).await.unwrap();
    stop.notify_one();
    within(serving).await.unwrap().unwrap();

    struct Failing(Events);
    impl GotchaApp for Failing {
        type State = ();
        type Config = ();
        fn logger(&self) -> GotchaResult<()> {
            Ok(())
        }
        async fn config(&self) -> GotchaResult<ConfigWrapper<()>> {
            Ok(config())
        }
        async fn state(&self, _: &ConfigWrapper<()>) -> GotchaResult<()> {
            Ok(())
        }
        fn routes(&self, router: GotchaRouter<GotchaContext<(), ()>>) -> GotchaRouter<GotchaContext<(), ()>> {
            router
        }
        async fn tasks(&self, scheduler: &mut TaskScheduler<(), ()>) -> GotchaResult<()> {
            let events = self.0.clone();
            scheduler.spawn("discarded", move |_| async move {
                events.lock().unwrap().push("started");
            });
            Err(GotchaError::message("registration failed"))
        }
    }
    let events = Events::default();
    let error = within(Failing(events.clone()).run()).await.unwrap_err();
    assert!(matches!(error, GotchaError::Message(message) if message == "registration failed"));
    tokio::task::yield_now().await;
    assert!(events.lock().unwrap().is_empty(), "a failed registration discards spawned work");
}
