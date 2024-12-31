# gotcha
provide a featured web framework

## aim to
 - [x] everything of axum
 - [ ] automatically swagger api generation
 - [x] built-in message mechanism
 - [x] environment based configuration system, support environment resolver `${ANY_ENV_VAR}` and path variable `${app.database.name}` powered by [yaac](https://crates.io/crates/yaac)
 - [x] logging system
 - [x] opt-in prometheus integration
 - [x] task system with interval and cron

## get started
add dependency into `Cargo.toml`
```toml
gotcha = {version = "0.1"}
tokio = {version = "1", features = ["macros", 'rt-multi-thread']}
serde = {version="1", features=["derive"]}
```
```rust
use gotcha::{async_trait, ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Responder, State};
use serde::{Deserialize, Serialize};

pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Serialize, Clone)]
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

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}

```