use gotcha::{get, App, Responder, GotchaAppWrapperExt, HttpServer, Message, async_trait, MessagerWrapper};

struct HelloWorldMessage;


#[async_trait]
impl Message for HelloWorldMessage {
    type Output = String;
    async fn handle(self) -> Self::Output {
        "hello world".to_string()
    }
}



#[get("/")]
pub async fn hello_world(messager:MessagerWrapper) -> impl Responder {
    messager.send(HelloWorldMessage).await
}

#[tokio::main]
async fn main() {
    HttpServer::new(|| {
        App::new()
    .into_gotcha()
    .service(hello_world)
    .done()
    })
    .bind(("127.0.0.1", 8080)).unwrap()
    .run()
    .await;
}
