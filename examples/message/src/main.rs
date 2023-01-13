use std::sync::Arc;

use gotcha::{
    async_trait, get, App, GotchaAppWrapperExt, GotchaCli, HttpServer, Message, Messager,
    MessagerWrapper, Responder,
};

use serde::Deserialize;

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

#[get("/")]
pub async fn hello_world(messager: MessagerWrapper) -> impl Responder {
    messager.into_inner().send(HelloWorldMessage).await
}

#[tokio::main]
async fn main() {
    GotchaCli::<_, Config>::new()
        .server(|_| async move {
            HttpServer::new(|| App::new().into_gotcha().service(hello_world).done())
                .bind(("127.0.0.1", 8080))
                .unwrap()
                .run()
                .await
        })
        .run()
        .await
}
