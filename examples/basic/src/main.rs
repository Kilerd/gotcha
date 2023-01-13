use gotcha::{get, App, GotchaAppWrapperExt, GotchaCli, HttpServer, Responder, tracing::{info}, task::{cron_proc_macro_wrapper, interval_proc_macro_wrapper}};
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
            info!("starting application");
            HttpServer::new(move || {
                App::new()
                    .into_gotcha()
                    .service(hello_world)
                    .data(config.clone())
                    .task(interval_proc_macro_wrapper)
                    .done()
            })
            .workers(6)
            .bind(("127.0.0.1", 8080))
            .unwrap()
            .run()
            .await
        })
        .run()
        .await
}
