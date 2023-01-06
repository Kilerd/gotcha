# gotcha
provide a featured web framework

## aim to
 - everything of actix-web
 - automatically swagger api generation
 - built-in message machenism
 - environment based configuration system
 - logging system
 - opt-in prometheus integration
 - sqlx based magic ORM


## get started
add dependency into `Cargo.toml`
```toml
actix-web = "4"
gotcha = {version = "0.1"}
tokio = {version = "1", features = ["macros", 'rt-multi-thread']}
serde = {version="1", features=["derive"]}
```
```rust
use gotcha::{get, App, GotchaAppWrapperExt, GotchaCli, HttpServer, Responder};
use serde::Deserialize;

#[get("/")]
pub async fn hello_world() -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Clone)]
struct Config {}

#[tokio::main]
async fn main() {
    GotchaCli::<_, Config>::new()
        .server(|config| async move {
            HttpServer::new(|| {
                App::new()
                    .into_gotcha()
                    .service(hello_world)
                    .data(config)
                    .done()
            })
            .bind(("127.0.0.1", 8080))
            .unwrap()
            .run()
            .await;
        })
        .run()
        .await
}
```