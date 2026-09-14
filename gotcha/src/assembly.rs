//! Assemble application endpoints before applying application-wide middleware.

use axum::Router;

use crate::{GotchaResult, GotchaRouter};

pub(crate) fn assemble<State: Clone + Send + Sync + 'static>(
    router: GotchaRouter<State>, state: State, #[cfg(feature = "openapi")] endpoints: Option<crate::OpenApiEndpoints>, finish: impl FnOnce(Router) -> Router,
) -> GotchaResult<Router> {
    #[cfg(feature = "openapi")]
    let router = if let Some(endpoints) = endpoints {
        endpoints.validate()?;
        let (router, spec) = router.into_openapi_parts();
        endpoints.mount(router.with_state(state), spec)
    } else {
        router.into_axum_router(state)
    };
    #[cfg(not(feature = "openapi"))]
    let router = router.into_axum_router(state);

    Ok(finish(router))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        extract::{Request, State},
        http::{HeaderValue, StatusCode},
        middleware::{from_fn_with_state, Next},
        response::Response,
        Router,
    };
    use tower::ServiceExt;

    use crate::{startup::Application, ConfigWrapper, EmptyConfig, Gotcha, GotchaApp, GotchaContext, GotchaResult, GotchaRouter};

    fn context() -> GotchaContext<(), EmptyConfig> {
        GotchaContext {
            state: (),
            config: ConfigWrapper::default(),
        }
    }

    async fn request(router: &Router, path: &str) -> (StatusCode, Vec<u8>) {
        let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        (response.status(), to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap().to_vec())
    }

    async fn mark(State(name): State<&'static str>, mut request: Request, next: Next) -> Response {
        request.headers_mut().append("x-order", HeaderValue::from_static(name));
        next.run(request).await
    }

    #[tokio::test]
    async fn application_layers_cover_later_routes_and_fallback_in_axum_order() {
        async fn order(request: Request) -> String {
            request
                .headers()
                .get_all("x-order")
                .iter()
                .map(|value| value.to_str().unwrap())
                .collect::<Vec<_>>()
                .join(",")
        }
        struct App;
        impl GotchaApp for App {
            type State = ();
            type Config = EmptyConfig;
            async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
                Ok(())
            }
            fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
                router.get("/late", order).fallback(order)
            }
            fn finish_router(&self, router: Router) -> Router {
                router.layer(from_fn_with_state("inner", mark)).layer(from_fn_with_state("outer", mark))
            }
        }
        let mut builder = Gotcha::from_state(())
            .app_layer(from_fn_with_state("inner", mark))
            .get("/late", order)
            .fallback(order)
            .app_layer(from_fn_with_state("outer", mark));
        for router in [builder.router(context()).await.unwrap(), App.build_router(context()).await.unwrap()] {
            for path in ["/late", "/missing"] {
                assert_eq!(request(&router, path).await, (StatusCode::OK, b"outer,inner".to_vec()));
            }
        }
    }

    #[cfg(feature = "openapi")]
    mod docs {
        use super::*;
        use crate::{api, OpenApiEndpoints};
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        #[api]
        async fn hello() -> String {
            "hello".into()
        }

        type Context = GotchaContext<(), EmptyConfig>;

        fn routes(router: GotchaRouter<Context>, calls: Arc<AtomicUsize>) -> GotchaRouter<Context> {
            // A Send-only, consumed capture keeps the FnOnce export contract exercised.
            let title = std::cell::Cell::new(String::from("Exported API"));
            router.nest("/api", GotchaRouter::default().get("/hello", hello)).openapi(move |mut spec| {
                spec.info.title = title.into_inner();
                calls.fetch_add(1, Ordering::SeqCst);
                spec
            })
        }

        #[derive(Clone, Copy, Default)]
        enum Protection {
            #[default]
            None,
            Business,
            Application,
        }

        async fn deny(_: Request, _: Next) -> StatusCode {
            StatusCode::UNAUTHORIZED
        }

        #[derive(Default)]
        struct App {
            endpoints: Option<OpenApiEndpoints>,
            calls: Arc<AtomicUsize>,
            protection: Protection,
        }

        impl GotchaApp for App {
            type State = ();
            type Config = EmptyConfig;
            async fn config(&self) -> GotchaResult<ConfigWrapper<EmptyConfig>> {
                panic!("export must not load configuration")
            }
            async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
                panic!("export must not initialize state")
            }
            #[cfg(feature = "task")]
            async fn tasks(&self, _: &mut crate::TaskScheduler<(), EmptyConfig>) -> GotchaResult<()> {
                panic!("export must not register tasks")
            }
            fn openapi_endpoints(&self) -> Option<OpenApiEndpoints> {
                self.endpoints.clone()
            }
            fn routes(&self, router: GotchaRouter<Context>) -> GotchaRouter<Context> {
                let router = routes(router, self.calls.clone());
                if matches!(self.protection, Protection::Business) {
                    router.layer(axum::middleware::from_fn(deny))
                } else {
                    router
                }
            }
            fn finish_router(&self, router: Router) -> Router {
                if matches!(self.protection, Protection::Application) {
                    router.layer(axum::middleware::from_fn(deny))
                } else {
                    router
                }
            }
        }

        fn application_builder(app: &App) -> Gotcha<(), EmptyConfig> {
            let mut builder = Gotcha::from_state(());
            if matches!(app.protection, Protection::Application) {
                // Registration happens before business routes and documentation options.
                builder = builder.app_layer(axum::middleware::from_fn(deny));
            }
            builder.routes(|router| app.routes(router))
        }

        #[tokio::test]
        async fn default_and_explicitly_disabled_docs_do_not_generate_or_reserve_routes() {
            let app = App::default();
            let mut default = application_builder(&app);
            let mut disabled = application_builder(&app).with_openapi().openapi_endpoints(None);
            for router in [
                default.router(context()).await.unwrap(),
                disabled.router(context()).await.unwrap(),
                app.build_router(context()).await.unwrap(),
            ] {
                assert_eq!(request(&router, "/api/hello").await.0, StatusCode::OK);
                for path in ["/openapi.json", "/redoc", "/scalar"] {
                    assert_eq!(request(&router, path).await.0, StatusCode::NOT_FOUND);
                }
            }
            assert_eq!(app.calls.load(Ordering::SeqCst), 0);
        }

        #[tokio::test]
        async fn invalid_endpoint_configuration_fails_before_document_generation() {
            let endpoints = OpenApiEndpoints {
                json_path: "relative.json".into(),
                ..Default::default()
            };
            let app = App {
                endpoints: Some(endpoints.clone()),
                ..Default::default()
            };
            let mut builder = application_builder(&app).openapi_endpoints(Some(endpoints));
            for result in [builder.router(context()).await, app.build_router(context()).await] {
                assert!(result.unwrap_err().to_string().contains("OpenAPI endpoint"));
            }
            assert_eq!(app.calls.load(Ordering::SeqCst), 0);
        }

        #[tokio::test]
        async fn explicit_docs_and_exports_share_the_complete_document_and_run_transforms_once() {
            let app = App {
                endpoints: Some(OpenApiEndpoints::default()),
                ..Default::default()
            };
            let expected = serde_json::to_vec(&app.openapi_document()).unwrap();
            assert_eq!(serde_json::to_vec(&application_builder(&app).into_openapi()).unwrap(), expected);
            assert_eq!(serde_json::to_vec(&app.routes(GotchaRouter::default()).into_openapi()).unwrap(), expected);
            let mut builder = application_builder(&app).with_openapi();
            let routers = [builder.router(context()).await.unwrap(), app.build_router(context()).await.unwrap()];
            assert_eq!(app.calls.load(Ordering::SeqCst), 5);
            for router in routers {
                for _ in 0..2 {
                    assert_eq!(request(&router, "/openapi.json").await, (StatusCode::OK, expected.clone()));
                }
                for path in ["/redoc", "/scalar"] {
                    assert_eq!(request(&router, path).await.0, StatusCode::OK);
                }
            }
            assert_eq!(app.calls.load(Ordering::SeqCst), 5);
            let spec: serde_json::Value = serde_json::from_slice(&expected).unwrap();
            assert_eq!(spec["info"]["title"], "Exported API");
            assert_eq!(spec["paths"].as_object().unwrap().keys().collect::<Vec<_>>(), ["/api/hello"]);
        }

        #[tokio::test]
        async fn application_and_business_auth_have_explicit_documentation_scopes() {
            for protection in [Protection::Business, Protection::Application] {
                let app = App {
                    endpoints: Some(OpenApiEndpoints::default()),
                    protection,
                    ..Default::default()
                };
                let mut builder = application_builder(&app).with_openapi();
                for router in [builder.router(context()).await.unwrap(), app.build_router(context()).await.unwrap()] {
                    assert_eq!(request(&router, "/api/hello").await.0, StatusCode::UNAUTHORIZED);
                    for path in ["/openapi.json", "/redoc", "/scalar"] {
                        let expected = if matches!(protection, Protection::Application) {
                            StatusCode::UNAUTHORIZED
                        } else {
                            StatusCode::OK
                        };
                        assert_eq!(request(&router, path).await.0, expected);
                    }
                }
            }
        }

        #[tokio::test]
        async fn custom_paths_update_both_uis_and_individual_uis_can_be_disabled() {
            let endpoints = OpenApiEndpoints {
                json_path: "/docs/api's&schema.json".into(),
                redoc_path: Some("/docs/redoc".into()),
                scalar_path: Some("/docs/scalar".into()),
            };
            let app = App {
                endpoints: Some(endpoints.clone()),
                ..Default::default()
            };
            let mut builder = application_builder(&app).with_openapi().openapi_endpoints(Some(endpoints));
            for router in [builder.router(context()).await.unwrap(), app.build_router(context()).await.unwrap()] {
                assert_eq!(request(&router, "/docs/api's&schema.json").await.0, StatusCode::OK);
                for (path, attribute) in [
                    ("/docs/redoc", "spec-url='/docs/api&#39;s&amp;schema.json'"),
                    ("/docs/scalar", "data-url=\"/docs/api&#39;s&amp;schema.json\""),
                ] {
                    let (status, html) = request(&router, path).await;
                    assert_eq!(status, StatusCode::OK);
                    assert!(String::from_utf8(html).unwrap().contains(attribute));
                }
                for path in ["/openapi.json", "/redoc", "/scalar"] {
                    assert_eq!(request(&router, path).await.0, StatusCode::NOT_FOUND);
                }
            }
            let endpoints = OpenApiEndpoints {
                redoc_path: None,
                scalar_path: None,
                ..Default::default()
            };
            let app = App {
                endpoints: Some(endpoints.clone()),
                ..Default::default()
            };
            let mut builder = application_builder(&app).openapi_endpoints(Some(endpoints));
            for router in [builder.router(context()).await.unwrap(), app.build_router(context()).await.unwrap()] {
                assert_eq!(request(&router, "/openapi.json").await.0, StatusCode::OK);
                assert_eq!(request(&router, "/redoc").await.0, StatusCode::NOT_FOUND);
                assert_eq!(request(&router, "/scalar").await.0, StatusCode::NOT_FOUND);
            }
        }

        #[test]
        fn export_needs_no_runtime_and_does_not_initialize_the_application() {
            #[derive(Clone)]
            struct NeverDefault;
            impl Default for NeverDefault {
                fn default() -> Self {
                    panic!("export must not construct default state")
                }
            }
            #[derive(Clone)]
            struct NeverLayer;
            impl tower_layer::Layer<axum::routing::Route> for NeverLayer {
                type Service = axum::routing::Route;
                fn layer(&self, _: Self::Service) -> Self::Service {
                    panic!("export must not install application middleware")
                }
            }
            let builder = Gotcha::with_state::<NeverDefault>()
                .with_file_config("this-file-must-not-be-opened-during-export.toml")
                .app_layer(NeverLayer)
                .get("/hello", hello);
            #[cfg(feature = "task")]
            let builder = builder.tasks(|_| panic!("export must not register tasks"));
            assert!(builder.into_openapi().paths.contains_key("/hello"));
            assert!(App::default().openapi_document().paths.contains_key("/api/hello"));
        }
    }
}
