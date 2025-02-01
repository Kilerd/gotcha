use gotcha::router::GotchaRouter;
use gotcha::{async_trait, ConfigWrapper, GotchaApp, GotchaContext, Responder, State};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone, Serialize, Default)]
pub struct Config {
    welcome: String,
}

pub async fn hello_world(config: State<ConfigWrapper<Config>>) -> impl Responder {
    config.0.application.welcome.clone()
}

pub struct App;

impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/", hello_world)
    }

    async fn state<'a, 'b>(&'a self, _config: &'b ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App {};
    app.run().await?;
    Ok(())
}
