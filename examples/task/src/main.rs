use gotcha::{async_trait, ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Responder, State, TaskScheduler};
use serde::{Deserialize, Serialize};

pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {}

pub struct App {}

#[async_trait]
impl GotchaApp for App {
    type State = ();

    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/", hello_world)
    }

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn tasks(&self, task_scheduler: &mut TaskScheduler<Self::State, Self::Config>) -> Result<(), Box<dyn std::error::Error>> {
        task_scheduler.interval("interval task", std::time::Duration::from_secs(1), interval_task);
        Ok(())
    }
}

async fn interval_task(_: GotchaContext<(), Config>) {
    println!("interval task");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}
