//! Typed commands in application code. `Message` groups a command's input, output, and handler;
//! `Messager` is a small struct over `GotchaContext` that hands the application state to those
//! handlers and is extractable as `State<Messager>`. Plain async service methods can organize this
//! work just as well.
//!
//! The HTTP handler awaits a command in its own future. Work that outlives the request goes through
//! a queue consumed by `scheduler.spawn`, which the application owns: shutdown stops the consumer,
//! lets it finish what is queued, and only then runs cleanup hooks. A bare `tokio::spawn` here would
//! outlive that drain and could race the hooks.
//!
//! HTTP connections and tasks drain at the same time, so a request still in flight can find the
//! consumer gone. The consumer closes the queue before its last drain, which makes that `send` fail,
//! and the request answers 503 instead of losing the name.

use std::future::Future;

use gotcha::axum::extract::FromRef;
use gotcha::axum::http::StatusCode;
use gotcha::prelude::*;
use tokio::sync::mpsc;

#[derive(Clone)]
struct AppState {
    greeting: String,
    greeted: mpsc::UnboundedSender<String>,
}

type Context = GotchaContext<AppState, EmptyConfig>;

/// A command and its handler. An implementation can write `async fn handle`.
trait Message: Send + 'static {
    type Output: Send + 'static;
    fn handle(self, messager: Messager) -> impl Future<Output = Self::Output> + Send;
}

/// Carries the application context to commands.
#[derive(Clone)]
struct Messager {
    context: Context,
}

impl Messager {
    fn state(&self) -> &AppState {
        &self.context.state
    }

    /// Run a command in the calling future and return its output.
    async fn send<M: Message>(&self, message: M) -> M::Output {
        message.handle(self.clone()).await
    }
}

impl FromRef<Context> for Messager {
    fn from_ref(context: &Context) -> Self {
        Self { context: context.clone() }
    }
}

/// Produces a greeting from the app state and awaits a nested command.
struct Greet {
    name: String,
}

impl Message for Greet {
    type Output = Result<String, StatusCode>;
    async fn handle(self, messager: Messager) -> Self::Output {
        messager.send(RecordGreeting(self.name.clone())).await?;
        Ok(format!("{}, {}!", messager.state().greeting, self.name))
    }
}

/// Hands the name to the background consumer without making the request wait for it.
struct RecordGreeting(String);

impl Message for RecordGreeting {
    type Output = Result<(), StatusCode>;
    async fn handle(self, messager: Messager) -> Self::Output {
        // Sending fails only once the consumer has stopped, during shutdown.
        messager.state().greeted.send(self.0).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn hello(State(messager): State<Messager>) -> Result<String, StatusCode> {
    messager.send(Greet { name: "world".to_string() }).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (greeted, mut queue) = mpsc::unbounded_channel();
    Gotcha::from_state(AppState {
        greeting: "Hello".to_string(),
        greeted,
    })
    .get("/", hello)
    .tasks(move |scheduler| {
        scheduler.spawn("greeting-log", |ctx| async move {
            let shutdown = ctx.shutdown();
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    Some(name) = queue.recv() => println!("greeted {name}"),
                }
            }
            // Refuse new names and finish the queued ones; cleanup hooks run after this returns.
            queue.close();
            while let Some(name) = queue.recv().await {
                println!("greeted {name}");
            }
            println!("greeting log drained");
        });
    })
    .on_shutdown(|_| async { println!("cleanup hooks run after the drain") })
    .run()
    .await?;
    Ok(())
}
