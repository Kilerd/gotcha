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
use gotcha::{ConfigWrapper, Gotcha, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, ServerConfig, State, WebSocketUpgrade};
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
    // Serving returns at the drain deadline even though the response never ends. The lower bound
    // rejects shutting down immediately instead of waiting for graceful drain.
    within(serving).await.unwrap().unwrap();
    let elapsed = started_at.elapsed();
    assert!(elapsed >= Duration::from_millis(90), "the drain deadline must be honored: {elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "serve must return at the drain deadline: {elapsed:?}");

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

#[derive(Clone)]
struct WsControls {
    started: mpsc::UnboundedSender<()>,
    done: mpsc::UnboundedSender<()>,
    events: Events,
    /// Released by a test after the drain deadline, for handlers that never poll their socket.
    release: Arc<Notify>,
}

/// Completes a WebSocket handshake and leaves the upgraded connection open.
async fn websocket(address: SocketAddr, path: &str) -> TcpStream {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(
            format!(
                "GET {path} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    while !response.ends_with(b"\r\n\r\n") {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut byte)).await.unwrap().unwrap();
        assert_eq!(read, 1, "connection closed during the WebSocket handshake");
        response.push(byte[0]);
        assert!(response.len() < 8192, "unexpectedly large handshake response");
    }
    let response = String::from_utf8_lossy(&response);
    assert!(response.starts_with("HTTP/1.1 101"), "{response}");
    stream
}

/// Closes on the shutdown token after a teardown delay the drain must wait for.
async fn graceful_ws(ws: WebSocketUpgrade, State(context): State<GotchaContext<WsControls, ()>>) -> Response {
    let controls = context.state.clone();
    let shutdown = context.shutdown();
    ws.on_upgrade(move |mut socket| async move {
        controls.events.lock().unwrap().push("ws-start");
        let _ = controls.started.send(());
        tokio::select! {
            _ = shutdown.cancelled() => {}
            _ = socket.recv() => {}
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        controls.events.lock().unwrap().push("ws-done");
        let _ = controls.done.send(());
    })
}

/// Never looks at the shutdown token; only the drain deadline can close it.
async fn ignoring_ws(ws: WebSocketUpgrade, State(context): State<GotchaContext<WsControls, ()>>) -> Response {
    let controls = context.state.clone();
    ws.on_upgrade(move |mut socket| async move {
        controls.events.lock().unwrap().push("ws-start");
        let _ = controls.started.send(());
        while let Some(Ok(_)) = socket.recv().await {}
        controls.events.lock().unwrap().push("ws-done");
        let _ = controls.done.send(());
    })
}

/// Ignores both the token and its socket: only the OS-level shutdown at the deadline can close it.
async fn blocked_ws(ws: WebSocketUpgrade, State(context): State<GotchaContext<WsControls, ()>>) -> Response {
    let controls = context.state.clone();
    ws.on_upgrade(move |mut socket| async move {
        controls.events.lock().unwrap().push("ws-start");
        let _ = controls.started.send(());
        // Waiting on a channel that only the test fires: nothing polls the socket here.
        controls.release.notified().await;
        let closed = !matches!(socket.recv().await, Some(Ok(_)));
        controls.events.lock().unwrap().push(if closed { "ws-closed" } else { "ws-open" });
        let _ = controls.done.send(());
    })
}

#[tokio::test]
async fn cleanup_waits_for_websocket_teardown() {
    let (started, mut started_rx) = mpsc::unbounded_channel();
    let (done, mut done_rx) = mpsc::unbounded_channel();
    let events: Events = Arc::default();
    let cleanup_events = events.clone();
    let controls = WsControls {
        started,
        done,
        events: events.clone(),
        release: Arc::new(Notify::new()),
    };
    let (stop, stopped) = oneshot::channel::<()>();
    let server = Gotcha::from_context(GotchaContext::new(controls, config()))
        .get("/ws", graceful_ws)
        .on_shutdown(move |_| {
            let events = cleanup_events.clone();
            async move {
                events.lock().unwrap().push("cleanup");
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

    let _client = within(websocket(address, "/ws")).await;
    within(started_rx.recv()).await.unwrap();
    stop.send(()).unwrap();
    // The upgrade leaves axum's drain, so cleanup must still wait for the handler's teardown.
    within(done_rx.recv()).await.unwrap();
    within(serving).await.unwrap().unwrap();
    assert_eq!(*events.lock().unwrap(), ["ws-start", "ws-done", "cleanup"]);
}

#[tokio::test]
async fn http_drain_deadline_closes_websocket_that_ignores_the_token() {
    let (started, mut started_rx) = mpsc::unbounded_channel();
    let (done, mut done_rx) = mpsc::unbounded_channel();
    let controls = WsControls {
        started,
        done,
        events: Arc::default(),
        release: Arc::new(Notify::new()),
    };
    let (stop, stopped) = oneshot::channel::<()>();
    let server = Gotcha::from_context(GotchaContext::new(controls, config()))
        .get("/ws", ignoring_ws)
        .http_shutdown_timeout(Duration::from_millis(100))
        .shutdown_signal(async move {
            let _ = stopped.await;
        })
        .bind()
        .await
        .unwrap();
    let address = server.local_addr();
    let serving = tokio::spawn(server.serve());

    let mut client = within(websocket(address, "/ws")).await;
    within(started_rx.recv()).await.unwrap();
    let started_at = std::time::Instant::now();
    stop.send(()).unwrap();
    within(serving).await.unwrap().unwrap();
    let elapsed = started_at.elapsed();
    assert!(elapsed >= Duration::from_millis(90), "the drain deadline must be honored: {elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "serve must return at the drain deadline: {elapsed:?}");

    // Force-closing the tracked stream wakes the handler, which drops the connection.
    within(done_rx.recv()).await.unwrap();
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut rest)).await;
    assert!(closed.is_ok(), "the upgraded connection must be closed at the drain deadline");
}

#[tokio::test]
async fn force_close_closes_socket_that_the_handler_is_not_polling() {
    let (started, mut started_rx) = mpsc::unbounded_channel();
    let (done, mut done_rx) = mpsc::unbounded_channel();
    let events: Events = Arc::default();
    let release = Arc::new(Notify::new());
    let controls = WsControls {
        started,
        done,
        events: events.clone(),
        release: release.clone(),
    };
    let (stop, stopped) = oneshot::channel::<()>();
    let server = Gotcha::from_context(GotchaContext::new(controls, config()))
        .get("/ws", blocked_ws)
        .http_shutdown_timeout(Duration::from_millis(100))
        .shutdown_signal(async move {
            let _ = stopped.await;
        })
        .bind()
        .await
        .unwrap();
    let address = server.local_addr();
    let serving = tokio::spawn(server.serve());

    let mut client = within(websocket(address, "/ws")).await;
    within(started_rx.recv()).await.unwrap();
    let started_at = std::time::Instant::now();
    stop.send(()).unwrap();

    // The handler is blocked off-socket, so only the OS-level shutdown can close the client.
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut rest)).await;
    assert!(closed.is_ok(), "the client must be closed at the deadline while the handler is idle");

    within(serving).await.unwrap().unwrap();
    let elapsed = started_at.elapsed();
    assert!(elapsed >= Duration::from_millis(90), "the drain deadline must be honored: {elapsed:?}");
    assert!(elapsed < Duration::from_secs(3), "shutdown must respect the force-close grace: {elapsed:?}");
    assert_eq!(*events.lock().unwrap(), ["ws-start"]);

    // Waking the handler proves the socket was shut down, not merely that the driver gave up.
    release.notify_one();
    within(done_rx.recv()).await.unwrap();
    assert_eq!(*events.lock().unwrap(), ["ws-start", "ws-closed"]);
}

#[tokio::test]
async fn cancelling_the_context_token_requests_shutdown() {
    async fn cancel(State(context): State<GotchaContext<(), ()>>) -> &'static str {
        context.shutdown().cancel();
        "cancelled"
    }

    let events: Events = Arc::default();
    let cleanup_events = events.clone();
    let server = Gotcha::from_context(GotchaContext::new((), config()))
        .get("/cancel", cancel)
        .on_shutdown(move |_| {
            let events = cleanup_events.clone();
            async move {
                events.lock().unwrap().push("cleanup");
            }
        })
        .bind()
        .await
        .unwrap();
    let address = server.local_addr();
    // The custom signal never fires: only the handler's token cancellation stops the server.
    let serving = tokio::spawn(server.serve_with_shutdown(std::future::pending::<()>()));

    assert!(within(get(address, "/cancel")).await.ends_with("cancelled"));
    within(serving).await.unwrap().unwrap();
    assert_eq!(*events.lock().unwrap(), ["cleanup"]);
}

#[tokio::test]
async fn serve_with_shutdown_replaces_the_application_signal() {
    struct App {
        stop: Arc<Notify>,
        events: Events,
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
            let events = self.events.clone();
            router.get("/", move || {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push("request");
                    "ok"
                }
            })
        }
        async fn shutdown_signal(&self) {
            self.stop.notified().await;
        }
    }

    let stop = Arc::new(Notify::new());
    let events: Events = Arc::default();
    let server = App {
        stop: stop.clone(),
        events: events.clone(),
    }
    .bind()
    .await
    .unwrap();
    let address = server.local_addr();
    let (custom_stop, custom_stopped) = oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve_with_shutdown(async move {
        let _ = custom_stopped.await;
    }));

    // The application signal fires but must not be polled at all.
    stop.notify_one();
    tokio::task::yield_now().await;
    assert!(!serving.is_finished(), "the application signal must not stop serve_with_shutdown");
    assert!(within(get(address, "/")).await.ends_with("ok"));

    custom_stop.send(()).unwrap();
    within(serving).await.unwrap().unwrap();
    assert_eq!(*events.lock().unwrap(), ["request"]);
}

#[tokio::test]
async fn panicking_cleanup_hook_runs_later_hooks_and_surfaces() {
    let events: Events = Arc::default();
    let first_events = events.clone();
    let second_events = events.clone();
    let (stop, stopped) = oneshot::channel::<()>();
    let server = Gotcha::from_context(GotchaContext::new((), config()))
        .on_shutdown(move |_| {
            let events = first_events.clone();
            async move {
                events.lock().unwrap().push("first");
                panic!("cleanup hook panic");
            }
        })
        .on_shutdown(move |_| {
            let events = second_events.clone();
            async move {
                events.lock().unwrap().push("second");
            }
        })
        .shutdown_signal(async move {
            let _ = stopped.await;
        })
        .bind()
        .await
        .unwrap();
    let serving = tokio::spawn(server.serve());
    stop.send(()).unwrap();
    let error = within(serving).await.unwrap_err();
    assert!(error.is_panic(), "a panicking cleanup hook must be re-raised");
    assert_eq!(*events.lock().unwrap(), ["first", "second"]);
}

#[tokio::test]
async fn panicking_shutdown_signal_propagates_out_of_every_entry_point() {
    struct App;
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
            panic!("shutdown signal panic");
        }
    }

    fn assert_signal_panic<T: std::fmt::Debug>(serving: Result<T, tokio::task::JoinError>) {
        let error = serving.expect_err("a panicking shutdown signal must stop serving");
        assert!(error.is_panic(), "the signal panic must reach the caller: {error}");
    }

    // The trait `run()` future polls the signal inline.
    assert_signal_panic(within(tokio::spawn(GotchaApp::run(App))).await);
    // `bind().serve()` used to poll it in a detached driver, which swallowed the panic.
    let server = App.bind().await.unwrap();
    assert_signal_panic(within(tokio::spawn(server.serve())).await);
    // Builder `run()` and `listen_on()` share the bound-server path.
    let builder = || {
        Gotcha::from_context(GotchaContext::new((), config())).shutdown_signal(async {
            panic!("shutdown signal panic");
        })
    };
    assert_signal_panic(within(tokio::spawn(builder().run())).await);
    assert_signal_panic(within(tokio::spawn(builder().listen_on("127.0.0.1:0".parse().unwrap()))).await);
}
