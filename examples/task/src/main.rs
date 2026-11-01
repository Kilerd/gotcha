use gotcha::{ConfigWrapper, EmptyState, Gotcha, GotchaContext, Responder, State};
use serde::{Deserialize, Serialize};

pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {}

async fn interval_task(_: GotchaContext<EmptyState, Config>) {
    println!("interval task");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::with_config::<Config>()
        .with_default_logger()
        .get("/", hello_world)
        .tasks(|scheduler| scheduler.interval("interval task", std::time::Duration::from_secs(1), interval_task))
        .run()
        .await?;
    Ok(())
}
