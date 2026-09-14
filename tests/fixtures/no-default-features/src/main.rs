use gotcha::{ConfigWrapper, EmptyConfig, Gotcha, GotchaApp, GotchaContext, GotchaResult, GotchaRouter};

struct App;

impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        router
    }
}

fn main() {
    // All startup entry points must compile without relying on workspace feature unification.
    drop(Gotcha::new().run());
    drop(Gotcha::new().listen("127.0.0.1:0"));
    drop(Gotcha::new().listen_on("127.0.0.1:0".parse().unwrap()));
    drop(App.run());
}
