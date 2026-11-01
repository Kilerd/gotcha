use gotcha::{ConfigWrapper, Gotcha, Responder, State};
use serde::{Deserialize, Serialize};

pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    pub name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::with_config::<Config>().with_default_logger().get("/", hello_world).run().await?;
    Ok(())
}
