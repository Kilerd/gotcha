#![cfg(feature = "openapi")]

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{Request, StatusCode};
use gotcha::axum::Router;
use gotcha::oas::Info;
use gotcha::serde_json::{json, Value};
use gotcha::{api, ConfigWrapper, EmptyConfig, Gotcha, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, OpenApiEndpoints};
use tower::ServiceExt;

#[api]
async fn hello() -> String {
    "hello".into()
}

fn custom_info() -> Info {
    Info {
        title: "Inventory API".into(),
        description: Some("Stock levels".into()),
        terms_of_service: Some("https://example.com/terms".into()),
        contact: None,
        license: None,
        version: "2.3.4".into(),
    }
}

fn assert_custom(info: &Info) {
    assert_eq!(info.title, "Inventory API");
    assert_eq!(info.description.as_deref(), Some("Stock levels"));
    assert_eq!(info.terms_of_service.as_deref(), Some("https://example.com/terms"));
    assert!(info.contact.is_none());
    assert!(info.license.is_none());
    assert_eq!(info.version, "2.3.4");
}

fn assert_custom_json(info: &Value) {
    assert_eq!(info["title"], "Inventory API");
    assert_eq!(info["description"], "Stock levels");
    assert_eq!(info["termsOfService"], "https://example.com/terms");
    assert_eq!(info["version"], "2.3.4");
    assert!(info.get("contact").is_none());
    assert!(info.get("license").is_none());
}

#[test]
fn default_document_has_neutral_info() {
    let spec = GotchaRouter::<()>::default().into_openapi();
    assert_eq!(spec.info.title, "API");
    assert_eq!(spec.info.version, "0.1.0");
    assert_eq!(spec.info.description, None);
    assert_eq!(spec.info.terms_of_service, None);
    assert!(spec.info.contact.is_none());
    assert!(spec.info.license.is_none());
    // Optional fields are omitted rather than serialized as `null`.
    assert_eq!(
        gotcha::serde_json::to_value(&spec).unwrap()["info"],
        json!({"title": "API", "version": "0.1.0"})
    );
}

#[test]
fn builder_info_is_exported() {
    let spec = Gotcha::new().openapi_info(custom_info()).get("/hello", hello).into_openapi();
    assert_custom(&spec.info);
}

struct InfoApp {
    transform: bool,
}

impl GotchaApp for InfoApp {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn openapi_endpoints(&self) -> Option<OpenApiEndpoints> {
        Some(OpenApiEndpoints::default())
    }

    fn openapi_info(&self) -> Info {
        custom_info()
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        let router = router.get("/hello", hello);
        if self.transform {
            router.openapi(|mut spec| {
                spec.info.title = "Transform wins".into();
                spec
            })
        } else {
            router
        }
    }
}

async fn served_json(router: Router) -> Value {
    let response = router
        .oneshot(Request::builder().uri("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    gotcha::serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn trait_info_is_used_by_export_and_the_served_document() {
    let app = InfoApp { transform: false };
    assert_custom(&app.openapi_document().info);

    let context = GotchaContext::new((), ConfigWrapper::<EmptyConfig>::default());
    let router = app.build_router(context).await.unwrap();
    let document = served_json(router).await;
    assert_custom_json(&document["info"]);
}

#[test]
fn transforms_run_after_the_info_block_and_can_override_it() {
    let spec = Gotcha::new()
        .openapi_info(custom_info())
        .openapi(|mut spec| {
            spec.info.title = "Transform wins".into();
            spec
        })
        .into_openapi();
    assert_eq!(spec.info.title, "Transform wins");
    assert_eq!(spec.info.version, "2.3.4", "fields the transform leaves alone keep the configured info");

    let app = InfoApp { transform: true };
    assert_eq!(app.openapi_document().info.title, "Transform wins");
}

#[test]
fn cargo_info_uses_the_calling_crate() {
    let info = gotcha::cargo_info!();
    // This test crate is built as part of the `gotcha` package, so these are its values.
    assert_eq!(info.title, env!("CARGO_PKG_NAME"));
    assert_eq!(info.title, "gotcha");
    assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(info.description.as_deref(), Some(env!("CARGO_PKG_DESCRIPTION")));
    assert!(info.description.is_some(), "gotcha declares a package description");
    assert_eq!(info.terms_of_service, None);
    assert!(info.contact.is_none());
    assert!(info.license.is_none());
}
