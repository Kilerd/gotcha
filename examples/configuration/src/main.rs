use gotcha::{get, App, Responder, GotchaAppWrapperExt, HttpServer};
use serde::{Deserialize}

#[derive(Debug, Deserialize)]
struct Config {
    welcome: String
}


#[get("/")]
pub async fn hello_world() -> impl Responder {
    "hello world"
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
