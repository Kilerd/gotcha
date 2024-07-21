use gotcha::{get, GotchaApp, GotchaCli, Responder, State};
use serde::Deserialize;
use gotcha::axum::extract::FromRef;
use gotcha::axum::handler::Handler;
use gotcha::axum::Router;

pub async fn hello_world(state: State<Config>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Clone)]
struct Config {}

#[derive(Debug, Deserialize, Clone)]
struct AppData {}

impl FromRef<(AppData, i32, Config)> for Config {
    fn from_ref(input: &(AppData, i32, Config)) -> Self {
        input.2.clone()
    }
}


#[tokio::main]
async fn main() {
    let app = GotchaApp::<_, Config>::new()
        .route("/", get(hello_world))
        .data(AppData {})
        .data(1i32)
        .done();
    app
        .serve("127.0.0.1", 8000)
        .await
}
