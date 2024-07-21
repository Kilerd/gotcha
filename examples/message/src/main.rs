use std::sync::Arc;

use gotcha::{async_trait, get, GotchaApp, GotchaCli, Message, Messager, Responder};
use serde::Deserialize;
use gotcha::message::MessagerWrapper;

#[derive(Debug, Deserialize, Clone)]
struct Config {}

struct HelloWorldMessage;

#[async_trait]
impl Message for HelloWorldMessage {
    type Output = String;
    async fn handle(self, messager: Arc<Messager>) -> Self::Output {
        messager.spawn(BackgroudTask).await;
        "hello world".to_string()
    }
}

struct BackgroudTask;

#[async_trait]
impl Message for BackgroudTask {
    type Output = ();
    async fn handle(self, _messager: Arc<Messager>) -> Self::Output {
        // do backend task here
        println!("do backend task here");
    }
}

pub async fn hello_world() -> impl Responder {
    // messager.0.send(HelloWorldMessage).await
}

#[tokio::main]
async fn main() {
    GotchaApp::<_,Config>::new()
        .route("/",get(hello_world))
        .done()
        .serve("127.0.0.1", 8080)
        .await
}
