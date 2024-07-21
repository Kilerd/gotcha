use serde::Deserialize;

use gotcha::{get, GotchaApp, Responder, State};
use gotcha::axum::extract::FromRef;

pub async fn hello_world(state: State<Config>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Clone)]
struct Config {}

impl FromRef<(Config,)> for Config {
    fn from_ref(input: &(Config, )) -> Self {
        input.0.clone()
    }
}


#[tokio::main]
async fn main() {
    GotchaApp::<_, Config>::new()
        .route("/", get(hello_world))
        .done()
        .serve("127.0.0.1", 8000)
        .await
}
