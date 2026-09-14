//! Typed commands with access to the application context.
//!
//! A [`Message`] groups a command's input, output, and handler. [`Messager`] passes
//! the application state/config to that handler and to any further messages it
//! sends. This convention lets HTTP handlers and scheduled tasks reuse commands;
//! ordinary async service methods remain an equally valid way to organize work.
//! Messages are always available, without a feature flag.
//!
//! There is no queue, worker pool, retry, or transport between services:
//! [`send`](Messager::send) directly awaits the handler in the calling future;
//! [`spawn`](Messager::spawn) starts a Tokio task and returns its [`JoinHandle`].
//! Both clone the context, using the state/config types' own `Clone` behavior.
//!
//! The `Messager` is extractable in handlers as `State<Messager<S, C>>`, because it
//! implements `FromRef<GotchaContext<S, C>>` (the context the framework injects as
//! the axum state).
//!
//! ```rust,no_run
//! use gotcha::{async_trait, ConfigWrapper, GotchaContext, Message, Messager, State};
//!
//! #[derive(Clone)]
//! struct AppState { greeting: String }
//!
//! struct Greet { name: String }
//!
//! #[async_trait]
//! impl Message<AppState, ()> for Greet {
//!     type Output = String;
//!     async fn handle(self, messager: Messager<AppState, ()>) -> String {
//!         format!("{}, {}!", messager.state().greeting, self.name)
//!     }
//! }
//!
//! async fn hello(State(messager): State<Messager<AppState, ()>>) -> String {
//!     messager.send(Greet { name: "world".into() }).await
//! }
//!
//! # async fn example() -> Result<(), tokio::task::JoinError> {
//! let messager = Messager::new(GotchaContext {
//!     state: AppState { greeting: "Hello".into() },
//!     config: ConfigWrapper::<()>::default(),
//! });
//! let task = messager.spawn(Greet { name: "world".into() });
//! assert_eq!(task.await?, "Hello, world!");
//! # Ok(())
//! # }
//! ```
//!
//! # Task lifetime
//!
//! Managing spawned messages is the caller's responsibility; they are not
//! registered with application shutdown or the task scheduler. Dropping their
//! handle (including by cancelling a future that owns it) detaches the task; it
//! does not cancel it. Retain the handle to join or abort the task. The runtime
//! must remain alive for the task to finish.
//!
//! To keep a message inside a scheduled execution's existing cancellation and
//! shutdown boundary, await `messager.send(message)` in that execution. A message
//! that spawns further tasks must manage those tasks separately. Cancellation
//! drops futures; it does not roll back side effects already performed.

use async_trait::async_trait;
use axum::extract::FromRef;
use tokio::task::JoinHandle;

use crate::GotchaContext;

/// A unit of asynchronous work, dispatched by a [`Messager`].
///
/// The `handle` method receives the `Messager`, so a message can read the
/// application state and dispatch further messages.
#[async_trait]
pub trait Message<S, C>: Send + 'static
where
    S: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    /// The value produced by handling this message, including any application
    /// error when this is a `Result<T, E>`.
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
    C: Clone + Send + Sync + 'static,
{
    context: GotchaContext<S, C>,
}

impl<S, C> Clone for Messager<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self { context: self.context.clone() }
    }
}

impl<S, C> Messager<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
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

    /// Execute a message directly in the calling future and return its output.
    ///
    /// Nothing executes until this future is polled. There is no queue or new
    /// task: application errors are returned unchanged, panics unwind through
    /// the caller, and dropping this future drops the in-flight handler future.
    pub async fn send<M: Message<S, C>>(&self, message: M) -> M::Output {
        message.handle(self.clone()).await
    }

    /// Spawn a Tokio task and return its handle, without waiting for completion.
    ///
    /// Awaiting the handle returns `Result<M::Output, tokio::task::JoinError>`.
    /// If the output itself is a `Result`, application errors remain in that
    /// inner result; a panic or task cancellation produces the outer `JoinError`.
    ///
    /// Dropping the handle detaches the task. To request cancellation, call
    /// [`JoinHandle::abort`], then await the handle to observe completion. Abort
    /// takes effect when the task yields and may race with normal completion.
    /// Application shutdown does not automatically cancel or join this task.
    ///
    /// # Panics
    ///
    /// Panics if called outside a Tokio runtime.
    pub fn spawn<M: Message<S, C>>(&self, message: M) -> JoinHandle<M::Output> {
        let messager = self.clone();
        tokio::spawn(async move { message.handle(messager).await })
    }
}

impl<S, C> FromRef<GotchaContext<S, C>> for Messager<S, C>
where
    S: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn from_ref(context: &GotchaContext<S, C>) -> Self {
        Messager::new(context.clone())
    }
}

#[cfg(test)]
mod tests;
