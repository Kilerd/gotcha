# gotcha
provide a featured web framework

## aim to
 - [x] everything of axum
 - [ ] automatically swagger api generation
 - [x] built-in message mechanism
 - [x] environment based configuration system, support environment resolver `${ANY_ENV_VAR}` and path variable `${app.database.name}` powered by [yaac](https://crates.io/crates/yaac)
 - [x] logging system
 - [ ] opt-in prometheus integration
 - [ ] sqlx based magic ORM
 - [ ] cron-based task system

## get started
add dependency into `Cargo.toml`
```toml
gotcha = {version = "0.1"}
tokio = {version = "1", features = ["macros", 'rt-multi-thread']}
serde = {version="1", features=["derive"]}
```
```rust
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

```