//! HTTP startup methods are absent when gotcha's `http1` feature is disabled.
//!
//! ```compile_fail
//! let _ = gotcha::Gotcha::new().run();
//! ```
//!
//! ```compile_fail
//! let _ = gotcha::Gotcha::new().listen("127.0.0.1:3000");
//! ```
//!
//! ```compile_fail
//! let _ = gotcha::Gotcha::new().listen_on("127.0.0.1:3000".parse().unwrap());
//! ```
//!
//! ```compile_fail
//! let _ = gotcha::GotchaApp::run(no_http1::App);
//! ```

use gotcha::{ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter};

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn health() -> &'static str {
    "ok"
}

fn routes(router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
    router.get("/health", health)
}

pub struct App;

impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        panic!("route assembly and document export must not initialize state")
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        routes(router)
    }
}

#[cfg(test)]
#[tokio::test]
async fn routes_can_be_built_without_serving() {
    let _builder = gotcha::Gotcha::from_state(()).merge(routes(GotchaRouter::default()));
    let context = GotchaContext {
        state: (),
        config: ConfigWrapper::default(),
    };
    let _: gotcha::axum::Router = App.build_router(context).await.unwrap();
}

#[cfg(feature = "openapi")]
#[test]
fn documents_can_be_exported_without_http_or_a_runtime() {
    let builder = gotcha::Gotcha::from_state(()).merge(routes(GotchaRouter::default())).into_openapi();
    let document = gotcha::serde_json::to_value(builder).unwrap();
    assert!(document["paths"]["/health"]["get"].is_object());
    assert_eq!(document, gotcha::serde_json::to_value(App.openapi_document()).unwrap());
}
