use gotcha::{GotchaApp, GotchaConfigLoader, Responder, State};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    welcome: String,
}

pub async fn hello_world(config: State<(Config,)>) -> impl Responder {
    config.0 .0.welcome.clone()
}

#[tokio::main]
async fn main() {
    let config:Config = GotchaConfigLoader::load(None);
    let app = GotchaApp::new();
    app
        .get("/", hello_world)
        .data((config,))
        .done()
        .serve("127.0.0.1", 8080)
        .await
}
