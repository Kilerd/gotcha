//! Binding before serving, shutdown tokens in the context, bounded HTTP draining, and cleanup hooks.

use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

use futures_util::{stream, StreamExt};
use gotcha::axum::body::{Body, Bytes};
use gotcha::axum::response::Response;
use gotcha::{ConfigWrapper, Gotcha, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, ServerConfig, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Notify};

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

async fn get(address: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

async fn open(address: SocketAddr, path: &str) -> TcpStream {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n").as_bytes())
        .await
        .unwrap();
    stream
}

#[derive(Clone, Default)]
struct Controls {
    started: Option<mpsc::UnboundedSender<()>>,
    observed: Option<mpsc::UnboundedSender<()>>,
}

fn controls() -> (Controls, mpsc::UnboundedReceiver<()>, mpsc::UnboundedReceiver<()>) {
    let (started, started_rx) = mpsc::unbounded_channel();
    let (observed, observed_rx) = mpsc::unbounded_channel();
    (
        Controls {
            started: Some(started),
            observed: Some(observed),
        },
        started_rx,
        observed_rx,
    )
}

/// Streams one event, then ends when the context shutdown token is cancelled.
async fn token_stream(State(context): State<GotchaContext<Controls, ()>>) -> Response {
    let token = context.shutdown();
    let observed = context.state.observed.clone().unwrap();
    let _ = context.state.started.as_ref().unwrap().send(());
    let mut cancelled = Box::pin(token.cancelled_owned());
    let mut first = true;
    let body = stream::poll_fn(move |cx| {
        if first {
            first = false;
            return Poll::Ready(Some(Ok::<_, Infallible>(Bytes::from_static(b"data: start\n\n"))));
        }
        if cancelled.as_mut().poll(cx).is_ready() {
            let _ = observed.send(());
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    });
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(body))
        .unwrap()
}

/// Starts an event stream that never ends, regardless of shutdown.
async fn endless_stream(State(context): State<GotchaContext<Controls, ()>>) -> Response {
    let _ = context.state.started.as_ref().unwrap().send(());
    let body = stream::once(async { Ok::<_, Infallible>(Bytes::from_static(b"data: start\n\n")) }).chain(stream::pending());
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(body))
        .unwrap()
}

#[tokio::test]
async fn builder_bind_reports_the_assigned_port_and_serves_requests() {
    let cleaned = Arc::new(AtomicBool::new(false));
    let hook_cleaned = cleaned.clone();
    let server = Gotcha::from_context(GotchaContext::new((), ConfigWrapper::<()>::default()))
        .port(0)
        .get("/", || async { "builder-ok" })
        .on_shutdown(move |context| async move {
            assert!(context.shutdown().is_cancelled());
            hook_cleaned.store(true, Ordering::SeqCst);
        })
        .bind()
        .await
        .unwrap();
    let address = server.local_addr();
    assert_ne!(address.port(), 0, "bind must report the port chosen for port 0");

    let (stop, stopped) = oneshot::channel::<()>();
    // `BoundServer` is `Send`, so serving can move to another task.
    let serving = tokio::spawn(server.serve_with_shutdown(async move {
        let _ = stopped.await;
    }));
    let response = within(get(address, "/")).await;
    assert!(response.starts_with("HTTP/1.0 200"), "{response}");
    assert!(response.ends_with("builder-ok"), "{response}");
    stop.send(()).unwrap();
    within(serving).await.unwrap().unwrap();
    assert!(cleaned.load(Ordering::SeqCst), "cleanup hooks run for serve_with_shutdown too");
}

#[tokio::test]
async fn trait_bind_reports_the_assigned_port_and_serves_requests() {
    struct App {
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
        fn routes(&self, router: GotchaRouter<GotchaContext<(), ()>>) -> GotchaRouter<GotchaContext<(), ()>> {
            router.get("/", || async { "trait-ok" })
        }
        async fn state(&self, _: &ConfigWrapper<()>) -> GotchaResult<()> {
            Ok(())
        }
        async fn shutdown_signal(&self) {
            self.stop.notified().await;
        }
    }

    let stop = Arc::new(Notify::new());
    let server = App { stop: stop.clone() }.bind().await.unwrap();
    let address = server.local_addr();
    assert_ne!(address.port(), 0, "bind must report the port chosen for port 0");

    let serving = tokio::spawn(server.serve());
    let response = within(get(address, "/")).await;
    assert!(response.starts_with("HTTP/1.0 200"), "{response}");
    assert!(response.ends_with("trait-ok"), "{response}");
    stop.notify_one();
    within(serving).await.unwrap().unwrap();
}

#[tokio::test]
async fn handler_stream_observes_context_shutdown_token() {
    let (state, mut started, mut observed) = controls();
    let (stop, stopped) = oneshot::channel::<()>();
    let server = Gotcha::from_context(GotchaContext::new(state, config()))
        .get("/stream", token_stream)
        .shutdown_signal(async move {
            let _ = stopped.await;
        })
        .bind()
        .await
        .unwrap();
    let address = server.local_addr();
    let serving = tokio::spawn(server.serve());

    let mut client = within(open(address, "/stream")).await;
    within(started.recv()).await.unwrap();
    let mut chunk = [0u8; 128];
    let read = within(client.read(&mut chunk)).await.unwrap();
    assert!(read > 0, "the streaming response must start");

    stop.send(()).unwrap();
    // The handler-held stream sees the same cancellation as the listener.
    within(observed.recv()).await.unwrap();
    within(serving).await.unwrap().unwrap();
}

#[tokio::test]
async fn http_drain_deadline_closes_streaming_connections() {
    let (state, mut started, _observed) = controls();
    let (stop, stopped) = oneshot::channel::<()>();
    let server = Gotcha::from_context(GotchaContext::new(state, config()))
        .get("/stream", endless_stream)
        .shutdown_signal(async move {
            let _ = stopped.await;
        })
        .http_shutdown_timeout(Duration::from_millis(100))
        .bind()
        .await
        .unwrap();
    let address = server.local_addr();
    let serving = tokio::spawn(server.serve());

    let mut client = within(open(address, "/stream")).await;
    within(started.recv()).await.unwrap();
    let mut chunk = [0u8; 128];
    assert!(within(client.read(&mut chunk)).await.unwrap() > 0);

    let started_at = std::time::Instant::now();
    stop.send(()).unwrap();
    // Serving returns at the drain deadline even though the response never ends.
    within(serving).await.unwrap().unwrap();
    assert!(started_at.elapsed() < Duration::from_secs(2));

    // The remaining connection was closed rather than left running.
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut rest)).await;
    assert!(closed.is_ok(), "the streaming connection must be closed at the drain deadline");
}

#[tokio::test]
async fn builder_cleanup_hooks_run_in_order_after_serving_stops() {
    let events: Events = Arc::default();
    let request_events = events.clone();
    let first_events = events.clone();
    let second_events = events.clone();
    let (stop, stopped) = oneshot::channel::<()>();
    let server = Gotcha::from_context(GotchaContext::new((), config()))
        .get("/mark", move || {
            let events = request_events.clone();
            async move {
                events.lock().unwrap().push("request");
                "marked"
            }
        })
        .on_shutdown(move |context| {
            let events = first_events.clone();
            async move {
                assert!(context.shutdown().is_cancelled());
                assert_ne!(context.config.server.port, 0);
                events.lock().unwrap().push("cleanup-1");
            }
        })
        .on_shutdown(move |_| {
            let events = second_events.clone();
            async move {
                events.lock().unwrap().push("cleanup-2");
            }
        })
        .shutdown_signal(async move {
            let _ = stopped.await;
        })
        .bind()
        .await
        .unwrap();
    let address = server.local_addr();
    let serving = tokio::spawn(server.serve());

    assert!(within(get(address, "/mark")).await.ends_with("marked"));
    stop.send(()).unwrap();
    within(serving).await.unwrap().unwrap();
    assert_eq!(*events.lock().unwrap(), ["request", "cleanup-1", "cleanup-2"]);
}

#[tokio::test]
async fn trait_run_runs_cleanup_after_serving_stops() {
    struct App {
        stop: Arc<Notify>,
        events: Events,
        address: mpsc::UnboundedSender<SocketAddr>,
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
        async fn state(&self, config: &ConfigWrapper<()>) -> GotchaResult<()> {
            let address = SocketAddr::new(config.server.host.parse().unwrap(), config.server.port);
            self.address.send(address).unwrap();
            Ok(())
        }
        fn routes(&self, router: GotchaRouter<GotchaContext<(), ()>>) -> GotchaRouter<GotchaContext<(), ()>> {
            let events = self.events.clone();
            router.get("/mark", move || {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push("request");
                    "marked"
                }
            })
        }
        async fn shutdown_signal(&self) {
            self.stop.notified().await;
        }
        async fn on_shutdown(&self, context: &GotchaContext<(), ()>) {
            assert!(context.shutdown().is_cancelled());
            self.events.lock().unwrap().push("trait-cleanup");
        }
    }

    let (addresses, mut address_rx) = mpsc::unbounded_channel();
    let stop = Arc::new(Notify::new());
    let events: Events = Arc::default();
    let serving = tokio::spawn(
        App {
            stop: stop.clone(),
            events: events.clone(),
            address: addresses,
        }
        .run(),
    );
    let address = within(address_rx.recv()).await.unwrap();
    assert!(within(get(address, "/mark")).await.ends_with("marked"));
    stop.notify_one();
    within(serving).await.unwrap().unwrap();
    assert_eq!(*events.lock().unwrap(), ["request", "trait-cleanup"]);
}
