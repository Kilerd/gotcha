//! Shared application startup. API adapters provide the application-specific hooks.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};

use axum::Router;
use tokio::net::TcpListener;

use crate::{ConfigErrorPolicy, ConfigWrapper, GotchaApp, GotchaContext, GotchaError, GotchaResult, ServerConfig};

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
    fn config(&mut self) -> impl Future<Output = GotchaResult<ConfigWrapper<Self::Config>>> + Send;
    fn state(&mut self, config: &ConfigWrapper<Self::Config>) -> impl Future<Output = GotchaResult<Self::State>> + Send;
    fn router(&mut self, context: GotchaContext<Self::State, Self::Config>) -> impl Future<Output = GotchaResult<Router>> + Send;
    #[cfg(feature = "task")]
    fn tasks(&mut self, scheduler: &mut crate::TaskScheduler<Self::State, Self::Config>) -> impl Future<Output = GotchaResult<()>> + Send;
}

pub(crate) struct PreparedServer {
    listener: TcpListener,
    pub(crate) router: Router,
}

pub(crate) async fn prepare<A: Application>(mut app: A, explicit: Option<SocketAddr>) -> GotchaResult<PreparedServer> {
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
    #[cfg(feature = "task")]
    app.tasks(&mut crate::TaskScheduler::new(context)).await?;
    Ok(PreparedServer { listener, router })
}

pub(crate) async fn run<A: Application>(app: A, explicit: Option<SocketAddr>) -> GotchaResult<()> {
    prepare(app, explicit).await?.serve().await
}

impl PreparedServer {
    async fn serve(self) -> GotchaResult<()> {
        tracing::info!("Server listening on http://{}", self.listener.local_addr().map_err(GotchaError::Io)?);
        axum::serve(self.listener, self.router).await.map_err(GotchaError::Io)
    }
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
