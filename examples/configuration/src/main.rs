use gotcha::{get, App, Data, GotchaAppWrapperExt, GotchaCli, HttpServer, Responder};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
struct Config {
    welcome: String,
}

#[get("/")]
pub async fn hello_world(config: Data<Config>) -> impl Responder {
    config.welcome.clone()
}

#[tokio::main]
async fn main() {
    GotchaCli::<_, Config>::new()
        .server(|config| async move {
            HttpServer::new(move || {
                App::new()
                    .into_gotcha()
                    .service(hello_world)
                    .data(config.clone())
                    .done()
            })
            .bind(("127.0.0.1", 8080))
            .unwrap()
            .run()
            .await;
        })
        .run()
        .await
}
