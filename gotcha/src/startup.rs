//! Shared application startup, binding, and shutdown. API adapters provide the application-specific hooks.

use std::future::{Future, IntoFuture};
use std::io;
use std::net::{IpAddr, Shutdown, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::serve::Listener;
use axum::Router;
use slab::Slab;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

use crate::{ConfigErrorPolicy, ConfigWrapper, GotchaApp, GotchaContext, GotchaError, GotchaResult, ServerConfig};

/// How long HTTP connections may drain before their sockets are closed.
pub(crate) const DEFAULT_HTTP_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for handlers to exit after their sockets have been force-closed.
const FORCE_CLOSE_GRACE: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct ListenOptions {
    pub host: Option<String>,
    pub port: Option<u16>,
}

impl ListenOptions {
    fn resolve(&self, config: &ServerConfig, explicit: Option<SocketAddr>) -> GotchaResult<SocketAddr> {
        if let Some(address) = explicit {
            return Ok(address);
        }
        let host = self.host.as_deref().unwrap_or(&config.host);
        let ip: IpAddr = host.parse().map_err(|_| GotchaError::InvalidAddress(host.into()))?;
        Ok(SocketAddr::new(ip, self.port.unwrap_or(config.port)))
    }
}

pub(crate) trait Application: Send {
    type State: Clone + Send + Sync + 'static;
    type Config: Clone + Send + Sync + 'static;

    fn logger(&mut self) -> GotchaResult<()> {
        Ok(())
    }
    fn listen_options(&mut self) -> ListenOptions {
        ListenOptions::default()
    }
    fn config_error_policy(&mut self) -> ConfigErrorPolicy<Self::Config>;
    fn shutdown_signal(&mut self) -> impl Future<Output = ()> + Send;
    /// How long in-flight HTTP connections may drain after shutdown is requested.
    fn http_shutdown_timeout(&self) -> Duration {
        DEFAULT_HTTP_SHUTDOWN_TIMEOUT
    }
    /// Run application cleanup after HTTP and task draining. The default does nothing.
    fn on_shutdown(&mut self, _context: &GotchaContext<Self::State, Self::Config>) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn task_shutdown_timeout(&self) -> std::time::Duration;
    fn config(&mut self) -> impl Future<Output = GotchaResult<ConfigWrapper<Self::Config>>> + Send;
    /// Build the state from the loaded configuration, which the context then shares.
    fn state(&mut self, config: &Arc<ConfigWrapper<Self::Config>>) -> impl Future<Output = GotchaResult<Self::State>> + Send;
    fn router(&mut self, context: GotchaContext<Self::State, Self::Config>) -> impl Future<Output = GotchaResult<Router>> + Send;
    fn tasks(&mut self, scheduler: &mut crate::TaskScheduler<Self::State, Self::Config>) -> impl Future<Output = GotchaResult<()>> + Send;
}

/// A bound application listener with its state, routes, and tasks initialized.
pub(crate) struct PreparedServer {
    listener: TcpListener,
    local_addr: SocketAddr,
    pub(crate) router: Router,
    shutdown: CancellationToken,
    tasks: Vec<crate::task::ScheduledTask>,
    task_shutdown_timeout: std::time::Duration,
    http_shutdown_timeout: Duration,
}

impl PreparedServer {
    pub(crate) fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Serve until `signal`, `extra_shutdown`, or the shared shutdown token resolves, then drain
    /// HTTP up to `http_shutdown_timeout` and scheduled tasks up to their own timeout. Both
    /// deadlines run concurrently. HTTP draining also waits for upgraded connections, which axum
    /// no longer tracks after a WebSocket upgrade. At the deadline all remaining sockets are shut
    /// down, and their handlers get a short grace period to exit before shutdown continues.
    pub(crate) async fn serve(self, signal: impl Future<Output = ()> + Send, extra_shutdown: impl Future<Output = ()> + Send) -> GotchaResult<()> {
        let shutdown = self.shutdown.clone();
        let local_addr = self.local_addr;
        let http_shutdown_timeout = self.http_shutdown_timeout;
        tracing::info!("Server listening on http://{local_addr}");
        // If the serving future is dropped, stop HTTP acceptance and abort owned scheduled tasks.
        // Connections keep the existing cancellation semantics: graceful shutdown is signalled,
        // but force-closing happens only at the HTTP drain deadline.
        let _shutdown_on_drop = shutdown.clone().drop_guard();
        let tasks = crate::RunningTasks::start(self.tasks, shutdown.clone());
        let task_shutdown_timeout = self.task_shutdown_timeout;
        let tracker = ConnectionTracker::default();
        let listener = TrackingListener::new(self.listener, tracker.clone());
        let server = axum::serve(listener, self.router)
            .with_graceful_shutdown(shutdown.clone().cancelled_owned())
            .into_future();
        tokio::pin!(server);
        let serving = async {
            let result = tokio::select! {
                biased;
                // A handler or task that holds the context token can also request shutdown.
                _ = shutdown.cancelled() => {
                    drain_http(&mut server, http_shutdown_timeout, &tracker).await
                }
                _ = signal => {
                    // Cancel before draining so handlers observe shutdown at the same moment the
                    // listener stops accepting, and before any request has to finish.
                    shutdown.cancel();
                    drain_http(&mut server, http_shutdown_timeout, &tracker).await
                }
                _ = extra_shutdown => {
                    shutdown.cancel();
                    drain_http(&mut server, http_shutdown_timeout, &tracker).await
                }
                result = &mut server => result,
            };
            shutdown.cancel();
            result.map_err(GotchaError::Io)
        };
        let drain_tasks = async {
            shutdown.cancelled().await;
            tasks.shutdown(task_shutdown_timeout).await;
        };
        // Start the task deadline when shutdown is requested, independently of HTTP draining.
        let (result, ()) = tokio::join!(serving, drain_tasks);
        result
    }
}

async fn drain_http<F>(server: &mut Pin<&mut F>, timeout: Duration, tracker: &ConnectionTracker) -> io::Result<()>
where
    F: Future<Output = io::Result<()>>,
{
    let drained = async {
        let result = server.await;
        // Axum stops tracking a connection once it upgrades, so a WebSocket handler can still be
        // running after the server future resolves. Wait for the tracked streams themselves.
        tracker.wait_empty().await;
        result
    };
    match tokio::time::timeout(timeout, drained).await {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!("HTTP connections did not drain within {timeout:?}; closing remaining connections");
            tracker.force_close();
            // Force-close wakes tasks polling their socket. Give handlers that were blocked
            // elsewhere (for example on an application channel) a brief chance to observe it and
            // drop the connection before cleanup runs; a handler that never polls leaks its task
            // instead of holding shutdown forever.
            if tokio::time::timeout(FORCE_CLOSE_GRACE, tracker.wait_empty()).await.is_err() {
                tracing::warn!("connections still live {FORCE_CLOSE_GRACE:?} after force-close; continuing shutdown");
            }
            Ok(())
        }
    }
}

/// Run the startup pipeline and discard the context. Used by crate tests that only need the bound
/// server; serving paths use [`prepare_with_context`] because cleanup hooks need the context.
#[cfg(test)]
pub(crate) async fn prepare<A: Application>(app: &mut A, explicit: Option<SocketAddr>) -> GotchaResult<PreparedServer> {
    Ok(prepare_with_context(app, explicit).await?.0)
}

pub(crate) async fn prepare_with_context<A: Application>(
    app: &mut A, explicit: Option<SocketAddr>,
) -> GotchaResult<(PreparedServer, GotchaContext<A::State, A::Config>)> {
    app.logger()?;
    let policy = app.config_error_policy();
    let mut config = policy.apply(app.config().await)?;
    let address = app.listen_options().resolve(&config.server, explicit)?;
    let listener = TcpListener::bind(address).await.map_err(|source| GotchaError::Bind {
        addr: address.to_string(),
        source,
    })?;
    // Resolve port 0 before state initialization and make all consumers see the effective address.
    let bound = listener.local_addr().map_err(GotchaError::Io)?;
    config.server = ServerConfig {
        host: bound.ip().to_string(),
        port: bound.port(),
    };
    let shutdown = CancellationToken::new();
    // Anything initialized below can capture the token through the context. If a later step
    // fails, cancel it so those waiters do not stay pending forever.
    let cancel_on_failure = shutdown.clone().drop_guard();
    let config = Arc::new(config);
    let state = app.state(&config).await?;
    let context = GotchaContext {
        config,
        state,
        shutdown: shutdown.clone(),
    };
    let router = app.router(context.clone()).await?;
    let tasks = {
        let mut scheduler = crate::TaskScheduler::with_shutdown(context.clone(), shutdown.clone());
        app.tasks(&mut scheduler).await?;
        scheduler.into_tasks()?
    };
    let server = PreparedServer {
        listener,
        local_addr: bound,
        router,
        shutdown,
        tasks,
        task_shutdown_timeout: app.task_shutdown_timeout(),
        http_shutdown_timeout: app.http_shutdown_timeout(),
    };
    // Preparation succeeded; the server owns the token and cancels it when shutdown starts.
    cancel_on_failure.disarm();
    Ok((server, context))
}

pub(crate) async fn run<A: Application>(mut app: A, explicit: Option<SocketAddr>) -> GotchaResult<()> {
    let (server, context) = prepare_with_context(&mut app, explicit).await?;
    let signal = app.shutdown_signal();
    let result = server.serve(signal, std::future::pending()).await;
    // Cleanup runs after HTTP draining and task shutdown, whether serving succeeded or not.
    app.on_shutdown(&context).await;
    result
}

type LifecycleStarter = Box<dyn FnOnce(bool) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// Bind, then prepare the signal and cleanup driver. The driver is not started (and no signal
/// handler is installed) until [`BoundServer::serve`] or [`BoundServer::serve_with_shutdown`].
pub(crate) async fn bind<A>(mut app: A, explicit: Option<SocketAddr>) -> GotchaResult<BoundServer>
where
    A: Application + 'static,
{
    let (server, context) = prepare_with_context(&mut app, explicit).await?;
    let shutdown = server.shutdown.clone();
    let drop_shutdown = shutdown.clone();
    let (finish, finish_rx) = oneshot::channel();
    // The driver owns the application. It resolves the shutdown signal, cancels the shared token,
    // waits until serving has drained, and only then runs application cleanup. The argument
    // selects whether the application's own signal is polled at all. It runs inline in the
    // serving future, so a panicking signal propagates to the caller instead of ending a
    // detached task. Serving always cancels the token before it resolves, so a resolved
    // `finish_rx` also covers shutdowns requested by a handler or task.
    let start: LifecycleStarter = Box::new(move |use_app_signal| {
        Box::pin(async move {
            let mut finish_rx = finish_rx;
            if use_app_signal {
                tokio::select! {
                    biased;
                    _ = &mut finish_rx => {}
                    _ = app.shutdown_signal() => {
                        shutdown.cancel();
                        let _ = finish_rx.await;
                    }
                }
            } else {
                // The replacement signal is polled by the serving future itself.
                let _ = finish_rx.await;
            }
            app.on_shutdown(&context).await;
        })
    });
    Ok(BoundServer {
        server,
        lifecycle: BoundLifecycle {
            starter: Some(start),
            shutdown: drop_shutdown,
        },
        finish: Some(finish),
    })
}

/// An application whose listener is bound, with the effective local address available before
/// serving starts. Binding can use port 0; [`Self::local_addr`] then reports the assigned port.
///
/// Shutdown order: the signal resolves, and the context shutdown token is cancelled and the
/// listener stops accepting. Then two drains run at the same time: in-flight HTTP connections
/// (including WebSocket upgrades, bounded by [`GotchaApp::http_shutdown_timeout`] or the builder's
/// `.http_shutdown_timeout()`) and background tasks (bounded by the task shutdown timeout). Neither
/// waits for the other, so a task can finish while a handler is still running. Cleanup hooks run
/// once both have finished. At the HTTP deadline, remaining sockets are shut down at the OS level,
/// their handlers get up to one second to observe the closure, and shutdown then continues. At the
/// task deadline, remaining tasks are aborted and get up to one second to stop, with the same
/// result; see [`RunningTasks::shutdown`](crate::RunningTasks::shutdown).
///
/// Signal handling starts only when serving starts: holding an unserved `BoundServer` does not
/// install a signal handler, and dropping it cancels the context shutdown token (releasing tasks
/// or handlers that captured it) without running cleanup hooks.
#[must_use = "a bound server holds a listener; call `serve` to run it"]
pub struct BoundServer {
    server: PreparedServer,
    lifecycle: BoundLifecycle,
    finish: Option<oneshot::Sender<()>>,
}

impl BoundServer {
    /// The address the listener is actually bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.server.local_addr()
    }

    /// Serve until the application's shutdown signal resolves, then drain and clean up.
    pub async fn serve(self) -> GotchaResult<()> {
        self.serve_inner(std::future::pending(), true).await
    }

    /// Serve until `signal` resolves instead of the application's configured shutdown signal.
    ///
    /// The application signal is not polled at all, so Ctrl-C and SIGTERM do not stop this server;
    /// `signal` replaces them. Application cleanup hooks still run after draining.
    pub async fn serve_with_shutdown<S>(self, signal: S) -> GotchaResult<()>
    where
        S: Future<Output = ()> + Send,
    {
        self.serve_inner(signal, false).await
    }

    async fn serve_inner<S>(mut self, signal: S, use_app_signal: bool) -> GotchaResult<()>
    where
        S: Future<Output = ()> + Send,
    {
        // Drive the lifecycle inline instead of spawning it: a panicking signal or cleanup hook
        // then surfaces to the caller, and dropping this future drops the driver with the server.
        let lifecycle = self.lifecycle.start(use_app_signal);
        let finish = self.finish.take();
        let serving = async move {
            let result = self.server.serve(std::future::pending(), signal).await;
            if let Some(finish) = finish {
                let _ = finish.send(());
            }
            result
        };
        let ((), result) = tokio::join!(lifecycle, serving);
        result
    }
}

/// Owns the signal/cleanup driver. Created at bind and consumed when serving begins.
struct BoundLifecycle {
    starter: Option<LifecycleStarter>,
    shutdown: CancellationToken,
}

impl BoundLifecycle {
    fn start(&mut self, use_app_signal: bool) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let starter = self.starter.take().expect("bound server was already served");
        starter(use_app_signal)
    }
}

impl Drop for BoundLifecycle {
    fn drop(&mut self) {
        if self.starter.is_some() {
            // Bound but never served: release anything that captured the context token.
            self.shutdown.cancel();
        }
    }
}

/// Install gotcha's default `tracing` subscriber: formatted output filtered by `RUST_LOG`, at
/// `INFO` when it is unset.
///
/// [`GotchaApp::logger`](crate::GotchaApp::logger) installs it by default, and
/// [`Gotcha::with_default_logger`](crate::Gotcha::with_default_logger) installs it at startup.
/// Call it yourself when you serve an assembled router, such as one from
/// [`Gotcha::into_router`](crate::Gotcha::into_router), on your own listener, since nothing else
/// installs a subscriber there. A global subscriber that is already installed is kept, so this
/// never fails and calling it more than once is harmless.
pub fn default_logger() -> GotchaResult<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .with_env_var("RUST_LOG")
                .from_env_lossy(),
        )
        .try_init()
        .ok();
    Ok(())
}

pub(crate) async fn shutdown_signal() {
    #[cfg(unix)]
    let result = {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => tokio::select! {
                result = tokio::signal::ctrl_c() => result,
                _ = terminate.recv() => Ok(()),
            },
            Err(error) => Err(error),
        }
    };
    #[cfg(not(unix))]
    let result = tokio::signal::ctrl_c().await;
    if let Err(error) = result {
        tracing::error!("failed to install shutdown signal handler: {error}; shutting down");
    }
}

/// Tracks live accepted connections: their OS sockets for deadline force-close, and their count
/// for drain completion. It also covers upgraded connections (WebSockets), which axum stops
/// tracking after the upgrade.
#[derive(Clone, Default)]
struct ConnectionTracker {
    state: Arc<TrackerState>,
}

#[derive(Default)]
struct TrackerState {
    sockets: Mutex<Slab<RawSocket>>,
    empty: tokio::sync::Notify,
    close: CancellationToken,
}

impl ConnectionTracker {
    fn track(&self, stream: TcpStream) -> TrackedStream {
        // A child token isolates this connection's waiter from every other connection's.
        let cancelled = Box::pin(self.state.close.child_token().cancelled_owned());
        let handle = raw_socket(&stream);
        let mut sockets = self.state.sockets.lock().unwrap();
        let slot = sockets.insert(handle);
        // A connection accepted while the deadline is firing must not escape the close.
        if self.state.close.is_cancelled() {
            shutdown_socket(handle);
        }
        TrackedStream {
            stream,
            cancelled,
            closed: false,
            slot,
            state: self.state.clone(),
        }
    }

    /// Resolve once every tracked connection has been dropped.
    async fn wait_empty(&self) {
        loop {
            let notified = self.state.empty.notified();
            tokio::pin!(notified);
            // Register before checking, so a drop between the check and the await is not missed.
            notified.as_mut().enable();
            if self.state.sockets.lock().unwrap().is_empty() {
                return;
            }
            notified.await;
        }
    }

    /// Wake cooperative streams and shut down every tracked OS socket. The socket shutdown is
    /// what closes a connection whose handler is not polling the socket when the deadline fires.
    fn force_close(&self) {
        self.state.close.cancel();
        let sockets = self.state.sockets.lock().unwrap();
        for (_, handle) in sockets.iter() {
            shutdown_socket(*handle);
        }
    }
}

#[cfg(unix)]
type RawSocket = std::os::fd::RawFd;
#[cfg(windows)]
type RawSocket = std::os::windows::io::RawSocket;

#[cfg(unix)]
fn raw_socket(stream: &TcpStream) -> RawSocket {
    use std::os::fd::AsRawFd;
    stream.as_raw_fd()
}

#[cfg(windows)]
fn raw_socket(stream: &TcpStream) -> RawSocket {
    use std::os::windows::io::AsRawSocket;
    stream.as_raw_socket()
}

#[cfg(unix)]
fn shutdown_socket(handle: RawSocket) {
    use std::os::fd::BorrowedFd;
    // SAFETY: a registered handle stays open until `TrackedStream::drop` removes it under this
    // same lock, and the owning stream is dropped only after removal.
    let fd = unsafe { BorrowedFd::borrow_raw(handle) };
    let _ = socket2::SockRef::from(&fd).shutdown(Shutdown::Both);
}

#[cfg(windows)]
fn shutdown_socket(handle: RawSocket) {
    use std::os::windows::io::BorrowedSocket;
    // SAFETY: see the Unix implementation.
    let socket = unsafe { BorrowedSocket::borrow_raw(handle) };
    let _ = socket2::SockRef::from(&socket).shutdown(Shutdown::Both);
}

/// Accepts TCP connections and counts them until the connection future is dropped.
struct TrackingListener {
    listener: TcpListener,
    tracker: ConnectionTracker,
}

impl TrackingListener {
    fn new(listener: TcpListener, tracker: ConnectionTracker) -> Self {
        Self { listener, tracker }
    }
}

impl Listener for TrackingListener {
    type Io = TrackedStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => return (self.tracker.track(stream), addr),
                Err(error) if is_connection_error(&error) => continue,
                Err(error) => {
                    tracing::error!("accept error: {error}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

/// A connection that fails its I/O as soon as the server force-closes it. The cancellation future
/// is polled alongside the socket, and the OS socket is shut down under the tracker lock, so a
/// task blocked on this stream wakes even when it is not being polled at the deadline.
struct TrackedStream {
    stream: TcpStream,
    cancelled: Pin<Box<dyn Future<Output = ()> + Send>>,
    closed: bool,
    slot: usize,
    state: Arc<TrackerState>,
}

impl TrackedStream {
    fn ensure_open(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        if self.closed {
            return Err(connection_aborted());
        }
        if self.cancelled.as_mut().poll(cx).is_ready() {
            self.closed = true;
            return Err(connection_aborted());
        }
        Ok(())
    }
}

fn connection_aborted() -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionAborted, "server is shutting down")
}

impl Drop for TrackedStream {
    fn drop(&mut self) {
        // Remove under the lock before this stream (and its socket) is closed: fields drop after
        // `drop` returns, so a force-close holding the lock never touches a reused descriptor.
        let mut sockets = self.state.sockets.lock().unwrap();
        sockets.remove(self.slot);
        if sockets.is_empty() {
            self.state.empty.notify_waiters();
        }
    }
}

impl AsyncRead for TrackedStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if let Err(error) = self.ensure_open(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for TrackedStream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        if let Err(error) = self.ensure_open(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Err(error) = self.ensure_open(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Err(error) = self.ensure_open(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_write_vectored(mut self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[io::IoSlice<'_>]) -> Poll<io::Result<usize>> {
        if let Err(error) = self.ensure_open(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
    }
}

fn is_connection_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused | io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset
    )
}

#[cfg(test)]
mod tests;

impl<A> Application for &A
where
    A: GotchaApp,
    A::Config: serde::de::DeserializeOwned,
{
    type State = A::State;
    type Config = A::Config;

    fn logger(&mut self) -> GotchaResult<()> {
        A::logger(self)
    }

    fn config_error_policy(&mut self) -> ConfigErrorPolicy<Self::Config> {
        A::config_error_policy(self)
    }

    async fn shutdown_signal(&mut self) {
        A::shutdown_signal(self).await;
    }

    fn http_shutdown_timeout(&self) -> Duration {
        A::http_shutdown_timeout(self)
    }

    async fn on_shutdown(&mut self, context: &GotchaContext<A::State, A::Config>) {
        A::on_shutdown(self, context).await;
    }

    fn task_shutdown_timeout(&self) -> std::time::Duration {
        A::task_shutdown_timeout(self)
    }

    async fn config(&mut self) -> GotchaResult<ConfigWrapper<Self::Config>> {
        A::config(self).await
    }

    async fn state(&mut self, config: &Arc<ConfigWrapper<Self::Config>>) -> GotchaResult<Self::State> {
        A::state(self, config).await
    }

    async fn router(&mut self, context: GotchaContext<Self::State, Self::Config>) -> GotchaResult<Router> {
        A::build_router(self, context).await
    }

    async fn tasks(&mut self, scheduler: &mut crate::TaskScheduler<Self::State, Self::Config>) -> GotchaResult<()> {
        A::tasks(self, scheduler).await
    }
}

/// Owned adapter used by [`GotchaApp::bind`], which erases the application into the bound
/// server's signal and cleanup driver.
pub(crate) struct OwnedApp<A>(pub(crate) A);

impl<A> Application for OwnedApp<A>
where
    A: GotchaApp + 'static,
    A::Config: serde::de::DeserializeOwned,
{
    type State = A::State;
    type Config = A::Config;

    fn logger(&mut self) -> GotchaResult<()> {
        A::logger(&self.0)
    }

    fn config_error_policy(&mut self) -> ConfigErrorPolicy<Self::Config> {
        A::config_error_policy(&self.0)
    }

    async fn shutdown_signal(&mut self) {
        A::shutdown_signal(&self.0).await;
    }

    fn http_shutdown_timeout(&self) -> Duration {
        A::http_shutdown_timeout(&self.0)
    }

    async fn on_shutdown(&mut self, context: &GotchaContext<A::State, A::Config>) {
        A::on_shutdown(&self.0, context).await;
    }

    fn task_shutdown_timeout(&self) -> std::time::Duration {
        A::task_shutdown_timeout(&self.0)
    }

    async fn config(&mut self) -> GotchaResult<ConfigWrapper<Self::Config>> {
        A::config(&self.0).await
    }

    async fn state(&mut self, config: &Arc<ConfigWrapper<Self::Config>>) -> GotchaResult<Self::State> {
        A::state(&self.0, config).await
    }

    async fn router(&mut self, context: GotchaContext<Self::State, Self::Config>) -> GotchaResult<Router> {
        A::build_router(&self.0, context).await
    }

    async fn tasks(&mut self, scheduler: &mut crate::TaskScheduler<Self::State, Self::Config>) -> GotchaResult<()> {
        A::tasks(&self.0, scheduler).await
    }
}
