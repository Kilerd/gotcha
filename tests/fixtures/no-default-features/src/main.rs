use gotcha::{ConfigWrapper, EmptyConfig, Gotcha, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Json, Schematic, Valid, Validate};
use serde::Deserialize;

/// `Schematic` and `Valid`'s wire-name mapping are available without the `openapi` feature.
#[derive(Debug, Deserialize, Schematic, Validate)]
#[validate(crate = "gotcha::validator")]
#[serde(rename_all = "camelCase")]
struct Payload {
    #[validate(length(min = 1))]
    first_name: String,
}

async fn create(Valid(Json(payload)): Valid<Json<Payload>>) -> String {
    payload.first_name
}

struct App;

impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        router.post("/", create)
    }
}

fn main() {
    // All startup entry points must compile without relying on workspace feature unification.
    drop(Gotcha::new().run());
    drop(Gotcha::new().listen("127.0.0.1:0"));
    drop(Gotcha::new().listen_on("127.0.0.1:0".parse().unwrap()));
    drop(App.run());
    // Background work and intervals need no feature; only cron is behind `task`.
    drop(Gotcha::new().tasks(|scheduler| {
        scheduler.spawn("consumer", |ctx| async move { ctx.shutdown().cancelled().await });
        scheduler.interval("heartbeat", std::time::Duration::from_secs(60), |_| async {});
    }));
}
