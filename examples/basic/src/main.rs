use gotcha::{async_trait, ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Responder, State};
use serde::{Deserialize, Serialize};

pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    pub name: String,
}

pub struct App {}

#[async_trait]
impl GotchaApp for App {
    type State = ();

    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/", hello_world)
    }

    #[allow(clippy::manual_async_fn)]
    fn state(&self, _config: &ConfigWrapper<Self::Config>) -> impl std::future::Future<Output = Result<Self::State, gotcha::GotchaError>> + Send {
        async move { Ok(()) }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    (App {}).run().await?;
    Ok(())
}
