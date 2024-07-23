use gotcha::axum::extract::FromRef;
use gotcha::{GotchaApp, Responder, State};
use serde::Deserialize;

pub(crate) async fn hello_world(_state: State<Config>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct Config {}

impl FromRef<(Config,)> for Config {
    fn from_ref(input: &(Config,)) -> Self {
        input.0.clone()
    }
}

#[tokio::main]
async fn main() {
    GotchaApp::<_, Config>::new().get("/", hello_world).done().serve("127.0.0.1", 8000).await
}
