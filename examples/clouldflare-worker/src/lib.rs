use gotcha::router::GotchaRouter;
use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, State};
use serde::{Deserialize, Serialize};


#[derive(Debug, Deserialize, Clone, Serialize, Default)]
pub struct Config {
    // welcome: String,
}

pub async fn hello_world(config: State<ConfigWrapper<Config>>) -> impl Responder {
    "hello world"
}

pub struct App;

impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/", hello_world)
    }

    async fn state<'a, 'b>(&'a self, _config: &'b ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }
}



#[worker::event(fetch)]
async fn handle_fetch(request: worker::Request, env: worker::Env, ctx: worker::Context) -> worker::Result<worker::Response> {
    
    let app = App{};
    let router = match app.worker_router(env).await {
        Ok(router) => router,
        Err(e) => return worker::Response::error(e.to_string(), 500)
    };
    let res = match router.call(request).await {
        Ok(res) => res,
        Err(e) => return worker::Response::error(e.to_string(), 500)
    };
    Ok(res)
    // worker::Response::ok("Hello, World!")
}
