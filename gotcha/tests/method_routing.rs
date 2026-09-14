use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{Method, Request, StatusCode};
use gotcha::{routing, ConfigWrapper, EmptyConfig, Extension, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Json, Path};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower::ServiceExt;

#[cfg(feature = "openapi")]
use gotcha::Schematic;

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(gotcha::Schematic))]
struct Item {
    id: u32,
    label: String,
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn read(Path(id): Path<u32>, Extension(label): Extension<&'static str>) -> Json<Item> {
    Json(Item { id, label: label.into() })
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn write(Path(id): Path<u32>, Json(item): Json<Item>) -> Json<Item> {
    Json(Item { id, ..item })
}

type Context = GotchaContext<(), EmptyConfig>;
struct App;

impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    #[cfg(feature = "openapi")]
    fn openapi_endpoints(&self) -> Option<gotcha::OpenApiEndpoints> {
        Some(gotcha::OpenApiEndpoints::default())
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        let methods = routing::get(read).merge(routing::post(write)).layer(Extension("composed"));
        router
            .get("/shortcut/{id}", read)
            .layer(Extension("shortcut"))
            .nest(
                "/api",
                GotchaRouter::default().route("/items/{id}", methods.clone()).route("/copies/{id}", methods),
            )
            .route_raw("/raw/{id}", gotcha::axum::routing::get(read).layer(Extension("raw")))
    }
}

async fn request(router: &gotcha::axum::Router, method: Method, path: &str, body: &str) -> Value {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn composed_routes_keep_http_handlers_and_documentation_together() {
    let router = App
        .build_router(Context {
            state: (),
            config: ConfigWrapper::default(),
        })
        .await
        .unwrap();
    for (path, label) in [
        ("/api/items/7", "composed"),
        ("/api/copies/7", "composed"),
        ("/shortcut/7", "shortcut"),
        ("/raw/7", "raw"),
    ] {
        assert_eq!(request(&router, Method::GET, path, "").await, json!({"id": 7, "label": label}));
    }
    assert_eq!(
        request(&router, Method::POST, "/api/items/7", r#"{"id":0,"label":"created"}"#).await,
        json!({"id":7,"label":"created"})
    );

    #[cfg(feature = "openapi")]
    {
        let spec = request(&router, Method::GET, "/openapi.json", "").await;
        let paths = spec["paths"].as_object().unwrap();
        assert_eq!(paths.len(), 3, "the annotated raw handler must remain undocumented");
        assert!(!paths.contains_key("/raw/{id}"));
        for path in ["/api/items/{id}", "/api/copies/{id}"] {
            assert_eq!(paths[path]["get"], paths["/shortcut/{id}"]["get"]);
            assert!(paths[path]["head"].is_null(), "implicit HEAD does not create an operation");
            assert_eq!(paths[path]["post"]["parameters"][0]["name"], "id");
            assert_eq!(
                paths[path]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
                "#/components/schemas/Item"
            );
        }
        assert_eq!(spec["components"]["schemas"].as_object().unwrap().len(), 1);
        assert_eq!(spec["components"]["schemas"]["Item"]["properties"]["id"]["type"], "integer");
    }
}
