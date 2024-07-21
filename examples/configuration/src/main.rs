use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;
use gotcha::{get, GotchaApp, GotchaCli, Responder, State};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    welcome: String,
}

pub async fn hello_world(config: State<(Config,)>) -> impl Responder {
    config.0.0.welcome.clone()
}

#[tokio::main]
async fn main() {
    let app = GotchaApp::<_, Config>::new();
    app
        .route("/", get(hello_world))
        .done()
        .serve("127.0.0.1", 8080)
        .await
}
