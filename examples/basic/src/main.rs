use gotcha::axum::extract::FromRef;
use gotcha::{GotchaApp, GotchaConfigLoader, Responder, State};
use serde::Deserialize;

pub(crate) async fn hello_world(_state: State<Config>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct Config {}

#[tokio::main]
async fn main() {
    let config: Config = GotchaConfigLoader::load(None);
    GotchaApp::new().get("/", hello_world).data(config).done().serve("127.0.0.1", 8000).await
}
