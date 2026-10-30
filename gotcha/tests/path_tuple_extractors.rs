#![cfg(feature = "openapi")]
#![allow(dead_code)]

use either::Either;
use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{Request, StatusCode};
use gotcha::{api, ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, ParameterProvider, Path};
use serde_json::Value;
use tower::ServiceExt;

#[api]
async fn issue(Path((org, repo, number)): Path<(String, String, u32)>) -> String {
    format!("{org}/{repo}#{number}")
}

type Context = GotchaContext<(), EmptyConfig>;
struct TupleApp;
impl GotchaApp for TupleApp {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    fn openapi_endpoints(&self) -> Option<gotcha::OpenApiEndpoints> {
        Some(gotcha::OpenApiEndpoints::default())
    }

    fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
        router.get("/orgs/{org}/repos/{repo}/issues/{number}", issue)
    }
}

#[tokio::test]
async fn three_segment_tuple_is_documented_in_route_order() {
    let router = TupleApp
        .build_router(Context {
            state: (),
            config: ConfigWrapper::default(),
        })
        .await
        .unwrap();

    let response = router
        .clone()
        .oneshot(Request::builder().uri("/orgs/acme/repos/gotcha/issues/3").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap().as_ref(), b"acme/gotcha#3");

    let response = router
        .oneshot(Request::builder().uri("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let spec: Value = serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    let parameters = spec["paths"]["/orgs/{org}/repos/{repo}/issues/{number}"]["get"]["parameters"]
        .as_array()
        .expect("path parameters are documented");
    assert_eq!(
        parameters.iter().map(|param| param["name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["org", "repo", "number"]
    );
    for parameter in parameters {
        assert_eq!(parameter["in"], "path");
        assert_eq!(parameter["required"], true);
    }
    assert_eq!(parameters[0]["schema"]["type"], "string");
    assert_eq!(parameters[1]["schema"]["type"], "string");
    assert_eq!(parameters[2]["schema"]["type"], "integer");
}

#[test]
fn sixteen_segment_tuple_is_the_largest_documentable_arity() {
    // serde implements `Deserialize` for tuples up to 16 elements, and axum's `Path` relies on
    // it; a 16-segment route is the deepest tuple a handler can extract.
    type Sixteen = (u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8);
    let url = "/{p1}/{p2}/{p3}/{p4}/{p5}/{p6}/{p7}/{p8}/{p9}/{p10}/{p11}/{p12}/{p13}/{p14}/{p15}/{p16}";
    match <Path<Sixteen> as ParameterProvider>::generate(url.to_string()) {
        Either::Left(params) => {
            assert_eq!(params.len(), 16);
            assert_eq!(
                params.iter().map(|param| param.name.as_str()).collect::<Vec<_>>(),
                ["p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9", "p10", "p11", "p12", "p13", "p14", "p15", "p16"]
            );
        }
        Either::Right(_) => panic!("Path<(..16 types..)> should generate parameters, not a request body"),
    }
}
