//! Demonstrates typed commands with shared application context. The HTTP handler
//! spawns a greeting command and joins its result; that command directly awaits
//! another message. Plain async service methods can also organize this work.

use gotcha::axum::http::StatusCode;
use gotcha::prelude::*;
use gotcha::{async_trait, Message, Messager};

#[derive(Clone, Default)]
struct AppState {
    greeting: String,
}

/// Produces a greeting from the app state and awaits a nested message.
struct Greet {
    name: String,
}

#[async_trait]
impl Message<AppState, EmptyConfig> for Greet {
    type Output = String;
    async fn handle(self, messager: Messager<AppState, EmptyConfig>) -> String {
        messager.send(LogGreeting(self.name.clone())).await;
        format!("{}, {}!", messager.state().greeting, self.name)
    }
}

/// A message executed within the greeting command's future.
struct LogGreeting(String);

#[async_trait]
impl Message<AppState, EmptyConfig> for LogGreeting {
    type Output = ();
    async fn handle(self, _messager: Messager<AppState, EmptyConfig>) {
        println!("greeted {}", self.0);
    }
}

async fn hello(State(messager): State<Messager<AppState, EmptyConfig>>) -> Result<String, StatusCode> {
    let task = messager.spawn(Greet { name: "world".to_string() });
    // Awaiting observes completion and panics. Dropping this handle would detach
    // the task, including if the HTTP handler's future were cancelled.
    task.await.map_err(|error| {
        gotcha::tracing::error!(%error, "Greeting task failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::with_state::<AppState>()
        .state(AppState { greeting: "Hello".to_string() })
        .get("/", hello)
        .run()
        .await?;
    Ok(())
}
