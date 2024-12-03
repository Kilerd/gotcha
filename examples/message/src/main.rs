// #![allow(dead_code)]
// use std::sync::Arc;

// use gotcha::{async_trait, Message, Messager,, Responder};
// use serde::Deserialize;

// #[derive(Debug, Deserialize, Clone)]
// struct Config {}

// pub(crate) struct HelloWorldMessage;

// #[async_trait]
// impl Message for HelloWorldMessage {
//     type Output = String;
//     async fn handle(self, messager: Arc<Messager>) -> Self::Output {
//         messager.spawn(BackgroundTask).await;
//         "hello world".to_string()
//     }
// }

// pub struct BackgroundTask;

// #[async_trait]
// impl Message for BackgroundTask {
//     type Output = ();
//     async fn handle(self, _messager: Arc<Messager>) -> Self::Output {
//         // do backend task here
//         println!("do backend task here");
//     }
// }

// pub async fn hello_world() -> impl Responder {
//     // messager.0.send(HelloWorldMessage).await
// }

#[tokio::main]
async fn main() {
    // OldGotchaApp::new().get("/", hello_world).done().serve("127.0.0.1", 8080).await
}
