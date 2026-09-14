//! Shared application startup. API adapters provide the application-specific hooks.

#[cfg(feature = "http1")]
use std::future::{Future, IntoFuture};
#[cfg(feature = "http1")]
use std::net::{IpAddr, SocketAddr};

#[cfg(feature = "http1")]
use axum::Router;
#[cfg(feature = "http1")]
use tokio::net::TcpListener;
#[cfg(feature = "http1")]
use tokio_util::sync::CancellationToken;

#[cfg(feature = "http1")]
use crate::{ConfigErrorPolicy, ConfigWrapper, GotchaApp, GotchaContext, GotchaError, GotchaResult, ServerConfig};

#[derive(Default)]
pub(crate) struct ListenOptions {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[cfg(feature = "http1")]
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

#[cfg(feature = "http1")]
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
    #[cfg(feature = "task")]
    fn task_shutdown_timeout(&self) -> std::time::Duration;
    fn config(&mut self) -> impl Future<Output = GotchaResult<ConfigWrapper<Self::Config>>> + Send;
    fn state(&mut self, config: &ConfigWrapper<Self::Config>) -> impl Future<Output = GotchaResult<Self::State>> + Send;
    fn router(&mut self, context: GotchaContext<Self::State, Self::Config>) -> impl Future<Output = GotchaResult<Router>> + Send;
    #[cfg(feature = "task")]
    fn tasks(&mut self, scheduler: &mut crate::TaskScheduler<Self::State, Self::Config>) -> impl Future<Output = GotchaResult<()>> + Send;
}

#[cfg(feature = "http1")]
pub(crate) struct PreparedServer {
    listener: TcpListener,
    pub(crate) router: Router,
    shutdown: CancellationToken,
    #[cfg(feature = "task")]
    tasks: Vec<crate::task::ScheduledTask>,
    #[cfg(feature = "task")]
    task_shutdown_timeout: std::time::Duration,
}

#[cfg(feature = "http1")]
pub(crate) async fn prepare<A: Application>(app: &mut A, explicit: Option<SocketAddr>) -> GotchaResult<PreparedServer> {
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
    let state = app.state(&config).await?;
    let context = GotchaContext { config, state };
    let router = app.router(context.clone()).await?;
    let shutdown = CancellationToken::new();
    #[cfg(feature = "task")]
    let tasks = {
        let mut scheduler = crate::TaskScheduler::with_shutdown(context, shutdown.clone());
        app.tasks(&mut scheduler).await?;
        scheduler.into_tasks()
    };
    Ok(PreparedServer {
        listener,
        router,
        shutdown,
        #[cfg(feature = "task")]
        tasks,
        #[cfg(feature = "task")]
        task_shutdown_timeout: app.task_shutdown_timeout(),
    })
}

#[cfg(feature = "http1")]
pub(crate) async fn run<A: Application>(mut app: A, explicit: Option<SocketAddr>) -> GotchaResult<()> {
    let prepared = prepare(&mut app, explicit).await?;
    prepared.serve(app.shutdown_signal()).await
}

#[cfg(feature = "http1")]
impl PreparedServer {
    async fn serve(self, signal: impl Future<Output = ()> + Send) -> GotchaResult<()> {
        tracing::info!("Server listening on http://{}", self.listener.local_addr().map_err(GotchaError::Io)?);
        // If the serving future is dropped, stop HTTP acceptance and abort owned scheduled tasks.
        let _shutdown_on_drop = self.shutdown.clone().drop_guard();
        #[cfg(feature = "task")]
        let tasks = crate::RunningTasks::start(self.tasks, self.shutdown.clone());
        let server = axum::serve(self.listener, self.router)
            .with_graceful_shutdown(self.shutdown.clone().cancelled_owned())
            .into_future();
        tokio::pin!(server);
        let serving = async {
            let result = tokio::select! {
                biased;
                _ = signal => {
                    self.shutdown.cancel();
                    server.await
                },
                result = &mut server => result,
            };
            self.shutdown.cancel();
            result.map_err(GotchaError::Io)
        };
        #[cfg(feature = "task")]
        {
            let drain_tasks = async {
                self.shutdown.cancelled().await;
                tasks.shutdown(self.task_shutdown_timeout).await;
            };
            // Start the task deadline when shutdown is requested, independently of HTTP draining.
            let (result, ()) = tokio::join!(serving, drain_tasks);
            result
        }
        #[cfg(not(feature = "task"))]
        serving.await
    }
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

#[cfg(all(test, feature = "http1"))]
mod tests;

#[cfg(feature = "http1")]
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

    #[cfg(feature = "task")]
    fn task_shutdown_timeout(&self) -> std::time::Duration {
        A::task_shutdown_timeout(self)
    }

    async fn config(&mut self) -> GotchaResult<ConfigWrapper<Self::Config>> {
        A::config(self).await
    }

    async fn state(&mut self, config: &ConfigWrapper<Self::Config>) -> GotchaResult<Self::State> {
        A::state(self, config).await
    }

    async fn router(&mut self, context: GotchaContext<Self::State, Self::Config>) -> GotchaResult<Router> {
        A::build_router(self, context).await
    }

    #[cfg(feature = "task")]
    async fn tasks(&mut self, scheduler: &mut crate::TaskScheduler<Self::State, Self::Config>) -> GotchaResult<()> {
        A::tasks(self, scheduler).await
    }
}
