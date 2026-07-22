//! Demonstrates the message system: a handler extracts `State<Messager<..>>`,
//! sends a message that reads the application state, and that message in turn
//! spawns a fire-and-forget background message.

use gotcha::prelude::*;
use gotcha::{async_trait, Message, Messager};

#[derive(Clone, Default)]
struct AppState {
    greeting: String,
}

/// Produces a greeting from the app state and kicks off a background message.
struct Greet {
    name: String,
}

#[async_trait]
impl Message<AppState, EmptyConfig> for Greet {
    type Output = String;
    async fn handle(self, messager: Messager<AppState, EmptyConfig>) -> String {
        messager.spawn(LogGreeting(self.name.clone()));
        format!("{}, {}!", messager.state().greeting, self.name)
    }
}

/// A fire-and-forget background message.
struct LogGreeting(String);

#[async_trait]
impl Message<AppState, EmptyConfig> for LogGreeting {
    type Output = ();
    async fn handle(self, _messager: Messager<AppState, EmptyConfig>) {
        println!("[background] greeted {}", self.0);
    }
}

async fn hello(State(messager): State<Messager<AppState, EmptyConfig>>) -> impl Responder {
    messager.send(Greet { name: "world".to_string() }).await
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
