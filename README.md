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
use serde::Deserialize;

use gotcha::{get, GotchaApp, GotchaConfigLoader, Responder, State};
use gotcha::axum::extract::FromRef;

pub(crate) async fn hello_world(_state: State<Config>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct Config {}

#[tokio::main]
async fn main() {
    let config: Config = GotchaConfigLoader::load(None);
    GotchaApp::new().get("/", hello_world)
        .data(config)
        .done()
        .serve("127.0.0.1", 8000).await
}

```