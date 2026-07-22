//! # Message Module
//!
//! A small message-dispatch system for background/async work with access to the
//! application state.
//!
//! Implement [`Message`] for a unit of work, then dispatch it with a [`Messager`]:
//! `send` awaits the result, `spawn` runs it fire-and-forget. The handler receives
//! the `Messager`, so it can read the application state/config
//! ([`Messager::state`] / [`Messager::context`]) and dispatch further messages.
//!
//! The `Messager` is extractable in handlers as `State<Messager<S, C>>`, because it
//! implements `FromRef<GotchaContext<S, C>>` (the context the framework injects as
//! the axum state).
//!
//! ```ignore
//! use gotcha::prelude::*;
//! use gotcha::{async_trait, Message, Messager};
//!
//! struct Greet { name: String }
//!
//! #[async_trait]
//! impl Message<AppState, AppConfig> for Greet {
//!     type Output = String;
//!     async fn handle(self, messager: Messager<AppState, AppConfig>) -> String {
//!         format!("{}, {}!", messager.state().greeting, self.name)
//!     }
//! }
//!
//! async fn hello(State(messager): State<Messager<AppState, AppConfig>>) -> impl Responder {
//!     messager.send(Greet { name: "world".into() }).await
//! }
//! ```

use async_trait::async_trait;
use axum::extract::FromRef;

use crate::{GotchaConfig, GotchaContext};

/// A unit of asynchronous work, dispatched by a [`Messager`].
///
/// The `handle` method receives the `Messager`, so a message can read the
/// application state and dispatch further messages.
#[async_trait]
pub trait Message<S, C>: Send + 'static
where
    S: Clone + Send + Sync + 'static,
    C: GotchaConfig,
{
    /// The value produced by handling this message.
    type Output: Send + 'static;

    /// Handle the message, producing its output.
    async fn handle(self, messager: Messager<S, C>) -> Self::Output;
}

/// Dispatches [`Message`]s. It carries the application [`GotchaContext`], so
/// messages can access the application state and configuration.
///
/// Extract it in a handler with `State<Messager<S, C>>`.
pub struct Messager<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: GotchaConfig,
{
    context: GotchaContext<S, C>,
}

impl<S, C> Clone for Messager<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: GotchaConfig,
{
    fn clone(&self) -> Self {
        Self { context: self.context.clone() }
    }
}

impl<S, C> Messager<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: GotchaConfig,
{
    /// Create a `Messager` bound to an application context.
    pub fn new(context: GotchaContext<S, C>) -> Self {
        Self { context }
    }

    /// The application context (state + config).
    pub fn context(&self) -> &GotchaContext<S, C> {
        &self.context
    }

    /// The application state.
    pub fn state(&self) -> &S {
        &self.context.state
    }

    /// Dispatch a message and await its output.
    pub async fn send<M: Message<S, C>>(&self, message: M) -> M::Output {
        message.handle(self.clone()).await
    }

    /// Dispatch a message as a detached background task (fire-and-forget).
    pub fn spawn<M: Message<S, C, Output = ()>>(&self, message: M) {
        let messager = self.clone();
        tokio::spawn(async move { message.handle(messager).await });
    }
}

impl<S, C> FromRef<GotchaContext<S, C>> for Messager<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: GotchaConfig,
{
    fn from_ref(context: &GotchaContext<S, C>) -> Self {
        Messager::new(context.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConfigWrapper, EmptyConfig};

    #[derive(Clone, Default)]
    struct AppState {
        greeting: String,
    }

    struct Greet {
        name: String,
    }

    #[async_trait]
    impl Message<AppState, EmptyConfig> for Greet {
        type Output = String;
        async fn handle(self, messager: Messager<AppState, EmptyConfig>) -> String {
            format!("{}, {}!", messager.state().greeting, self.name)
        }
    }

    #[test]
    fn send_dispatches_and_reads_state() {
        let context = GotchaContext {
            config: ConfigWrapper {
                basic: Default::default(),
                application: EmptyConfig::default(),
            },
            state: AppState { greeting: "Hello".to_string() },
        };
        let messager = Messager::new(context);

        let output = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(messager.send(Greet { name: "world".to_string() }));

        assert_eq!(output, "Hello, world!");
    }
}
