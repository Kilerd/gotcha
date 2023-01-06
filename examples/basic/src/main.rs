use gotcha::{get, App, GotchaAppWrapperExt, GotchaCli, HttpServer, Responder};

#[get("/")]
pub async fn hello_world() -> impl Responder {
    "hello world"
}

#[tokio::main]
async fn main() {
    GotchaCli::new()
        .server(|| async move {
            HttpServer::new(|| App::new().into_gotcha().service(hello_world).done())
                .bind(("127.0.0.1", 8080))
                .unwrap()
                .run()
                .await;
        })
        .run()
        .await
}
