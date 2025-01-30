//! # Gotcha
//! 
//! Gotcha is an enhanced web framework based on Axum, providing additional features and conveniences
//! for building web applications in Rust.
//! 
//! ## Features
//! 
//! - Built on top of Axum for high performance and reliability
//! - OpenAPI documentation generation (optional)
//! - Prometheus metrics integration (optional) 
//! - CORS support (optional)
//! - Static file serving (optional)
//! - Task scheduling
//! - Configuration management
//! - Message system
//! - State management
//! 
//! ## Example
//! 
//! ```no_run
//! use gotcha::{async_trait, ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Responder, State};
//! use serde::{Deserialize, Serialize};
//! 
//! pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
//!     "hello world"
//! }
//! 
//! #[derive(Debug, Deserialize, Serialize, Clone, Default)]
//! pub struct Config {
//!     pub name: String,
//! }
//! 
//! pub struct App {}
//! 
//! #[async_trait]
//! impl GotchaApp for App {
//!     type State = ();
//!     type Config = Config;
//! 
//!     fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
//!         router.get("/", hello_world)
//!     }
//! 
//!     async fn state(&self, _config: &ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
//!         Ok(())
//!     }
//! }
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     App {}.run().await?;
//!     Ok(())
//! }
//! ```
//! 
//! ## Optional Features
//! 
//! - `openapi` - Enables OpenAPI documentation generation
//! - `prometheus` - Enables Prometheus metrics
//! - `cors` - Enables CORS support
//! - `static_files` - Enables static file serving capabilities
//!

use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;

pub use async_trait::async_trait;
use axum::extract::FromRef;
pub use axum::extract::{Json, Path, Query, State};
pub use axum::response::IntoResponse as Responder;
pub use axum::routing::{delete, get, patch, post, put};
pub use axum_macros::debug_handler;
pub use config::ConfigWrapper;
pub use either::Either;
pub use gotcha_core::{ParameterProvider, Schematic};
pub use gotcha_macro::*;
pub use once_cell::sync::Lazy;
pub use router::GotchaRouter;
use serde::{Deserialize, Serialize};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};
pub use {axum, inventory, tracing};

pub use crate::config::GotchaConfigLoader;

#[cfg(feature = "message")]
pub mod message;
#[cfg(feature = "message")]
pub use crate::message::{Message, Messager};

#[cfg(feature = "openapi")]
pub use crate::openapi::Operable;
#[cfg(feature = "openapi")]
pub use oas;

#[cfg(feature = "openapi")]
pub use gotcha_macro::api;

pub mod config;

#[cfg(feature = "openapi")]
pub mod openapi;
pub mod router;
pub mod state;
pub mod error;

#[cfg(feature = "task")]
pub mod task;

#[cfg(feature = "cloudflare_worker")]
pub use worker;

#[cfg(feature = "prometheus")]
pub mod prometheus {
    pub use axum_prometheus::metrics::*;
}

pub mod layers {
    #[cfg(feature = "cors")]
    pub use tower_http::cors::{self, CorsLayer};
}

#[cfg(feature = "static_files")]
pub use tower_http::services::{ServeDir, ServeFile};

#[cfg(feature = "task")]
pub use task::TaskScheduler;

#[derive(Clone)]
pub struct GotchaContext<State: Clone + Send + Sync + 'static, Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default> {
    pub config: ConfigWrapper<Config>,
    pub state: State,
}

impl<State, Config> FromRef<GotchaContext<State, Config>> for ConfigWrapper<Config>
where
    State: Clone + Send + Sync + 'static,
    Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
{
    fn from_ref(context: &GotchaContext<State, Config>) -> Self {
        context.config.clone()
    }
}

pub trait GotchaApp: Sized + Send + Sync {
    type State: Clone + Send + Sync + 'static;
    type Config: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default;

    fn config(&self) -> impl std::future::Future<Output = Result<ConfigWrapper<Self::Config>, Box<dyn std::error::Error>>> + Send {async move  {
        let config = GotchaConfigLoader::load::<ConfigWrapper<Self::Config>>(std::env::var("GOTCHA_ACTIVE_PROFILE").ok());
        Ok(config)
    } }

    fn logger(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .with_env_var("RUST_LOG")
                    .from_env_lossy(),
            )
            .try_init()?;
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>>;

    fn state(&self, config: &ConfigWrapper<Self::Config>) -> impl std::future::Future<Output = Result<Self::State, Box<dyn std::error::Error>>> + Send;

    #[cfg(feature = "task")]
    fn tasks(&self, _task_scheduler: &mut TaskScheduler<Self::State, Self::Config>) -> impl std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send {async {
        Ok(())
    } }

    fn build_router(&self, context: GotchaContext<Self::State, Self::Config>) -> impl std::future::Future<Output = Result<axum::Router, Box<dyn std::error::Error>>> + Send {async move {
        let router = GotchaRouter::<GotchaContext<Self::State, Self::Config>>::default();
        let router = self.routes(router);

        let GotchaRouter {
            #[cfg(feature = "openapi")]
            operations,
            router: raw_router,
        } = router;

        #[cfg(feature = "openapi")]
        let openapi_spec = crate::openapi::generate_openapi(operations);

        cfg_if::cfg_if! {
            if #[cfg(feature = "openapi")] {
                let router = raw_router
                .with_state(context.clone())
                .route("/openapi.json", axum::routing::get(|| async move { Json(openapi_spec.clone()) }))
                .route("/redoc", axum::routing::get(openapi::openapi_html))
                .route("/scalar", axum::routing::get(openapi::scalar_html));
            }else {
                let router = raw_router
                .with_state(context.clone());
            }
        }
        Ok(router)
    } }

    #[cfg(not(feature = "cloudflare_worker"))]
    fn run(self) -> impl std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send {async move {
        self.logger()?;
        info!("logger has been initialized");
        let config: ConfigWrapper<Self::Config> = self.config().await?;
        let state = self.state(&config).await?;

        let context = GotchaContext { config: config.clone(), state };

        let router = self.build_router(context.clone()).await?;
        
        cfg_if::cfg_if! {
            if #[cfg(feature = "task")] {
                let mut task_scheduler = TaskScheduler::new(context.clone());
                self.tasks(&mut task_scheduler).await?;
            }
        }

        let addr = SocketAddrV4::new(Ipv4Addr::from_str(&config.basic.host)?, config.basic.port);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, router).await?;
        Ok(())
    } }

    #[cfg(feature = "cloudflare_worker")]
    fn worker_router(self, worker_env: worker::Env) -> impl std::future::Future<Output = Result<GotchaRouter<()>, Box<dyn std::error::Error>>> + Send {async move {
        let config: ConfigWrapper<Self::Config> = self.config().await?;
        let state = self.state(&config).await?;
        let context = GotchaContext { config: config.clone(), state };

        let router = self.build_router(context.clone()).await?;
 
        Ok(GotchaRouter { 
            #[cfg(feature = "openapi")]
            operations: Default::default(), 
            router 
        })
    } }
}

#[cfg(test)]
mod test {
    #[test]
    #[cfg(feature = "openapi")]
    fn pass() {
        let t = trybuild::TestCases::new();
        t.pass("tests/pass/openapi/*.rs");
    }
}
