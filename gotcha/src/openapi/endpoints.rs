use axum::{body::Bytes, response::Html, routing::get, Router};
use oas::OpenAPIV3;

use crate::endpoint::validate_path;
use crate::{GotchaError, GotchaResult};

/// HTTP paths for explicitly enabled OpenAPI documentation.
///
/// Applications expose no documentation endpoints by default. Pass `Some(config)`
/// to [`Gotcha::openapi_endpoints`](crate::Gotcha::openapi_endpoints), or return it
/// from [`GotchaApp::openapi_endpoints`](crate::GotchaApp::openapi_endpoints).
/// [`Gotcha::with_openapi`](crate::Gotcha::with_openapi) enables the default paths.
///
/// Paths must be distinct, literal, absolute URI paths without a query, fragment,
/// or `.` / `..` segment. Percent-encode characters outside the URI path character set.
/// Invalid configuration returns an error during router assembly. Collisions with
/// existing business routes follow Axum's usual route-conflict behavior (panic).
/// The UI pages load the document from `json_path`, relative to the origin root;
/// configure the externally reachable path when serving behind a reverse proxy.
///
/// ```
/// use gotcha::{Gotcha, OpenApiEndpoints};
/// let app = Gotcha::new().openapi_endpoints(Some(OpenApiEndpoints {
///     json_path: "/docs/schema.json".into(),
///     redoc_path: None,
///     scalar_path: Some("/docs/scalar".into()),
/// }));
/// ```
#[derive(Clone, Debug)]
pub struct OpenApiEndpoints {
    /// Path serving the generated JSON document. Defaults to `/openapi.json`.
    pub json_path: String,
    /// Legacy Redoc UI path. Disabled by default: the bundled Redoc rejects OpenAPI 3.2.
    /// Use Scalar for generated documents.
    pub redoc_path: Option<String>,
    /// Scalar UI path, or `None` to disable it. Defaults to `/scalar`.
    pub scalar_path: Option<String>,
}

impl Default for OpenApiEndpoints {
    fn default() -> Self {
        Self {
            json_path: "/openapi.json".into(),
            redoc_path: None,
            scalar_path: Some("/scalar".into()),
        }
    }
}

impl OpenApiEndpoints {
    pub(crate) fn validate(&self) -> GotchaResult<()> {
        let paths = [Some(self.json_path.as_str()), self.redoc_path.as_deref(), self.scalar_path.as_deref()];
        for (index, path) in paths.iter().enumerate() {
            if let Some(path) = path {
                validate_path("OpenAPI endpoint", path)?;
                if paths[..index].contains(&Some(path)) {
                    return Err(GotchaError::message(format!("duplicate OpenAPI endpoint path: {path}")));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn mount(self, mut router: Router, spec: OpenAPIV3) -> Router {
        // Serialize once at assembly; each request clones the shared `Bytes`. This used to clone
        // the whole document and re-serialize it on every request. The bytes match what
        // `axum::Json` produced: `serde_json::to_vec` and its internal `to_writer` agree, and the
        // content type is the same `application/json`.
        let document = Bytes::from(serde_json::to_vec(&spec).expect("the OpenAPI document is serializable"));
        router = router.route(
            &self.json_path,
            get(move || {
                let document = document.clone();
                async move {
                    (
                        [(axum::http::header::CONTENT_TYPE, axum::http::HeaderValue::from_static("application/json"))],
                        document,
                    )
                }
            }),
        );
        for (path, template) in [
            (self.redoc_path, include_str!("../../statics/redoc.html")),
            (self.scalar_path, include_str!("../../statics/scalar.html")),
        ] {
            if let Some(path) = path {
                // Render once at assembly; Bytes clones share the large embedded UI asset.
                let html = Bytes::from(render_ui(template, &self.json_path));
                router = router.route(&path, get(move || async move { Html(html) }));
            }
        }
        router
    }
}

fn render_ui(template: &str, json_path: &str) -> String {
    let escaped = json_path
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    template.replacen("__GOTCHA_OPENAPI_URL__", &escaped, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_document_is_served_as_its_serialized_bytes() {
        use axum::body::{to_bytes, Body};
        use axum::http::{header, Request, StatusCode};
        use tower::ServiceExt;

        let spec = OpenAPIV3 {
            info: crate::openapi::default_info(),
            paths: Default::default(),
            servers: None,
            components: None,
            security: None,
            tags: None,
            openapi: "3.2.0".to_string(),
            external_docs: None,
            extras: None,
        };
        let expected = serde_json::to_vec(&spec).unwrap();
        let router = OpenApiEndpoints {
            redoc_path: None,
            scalar_path: None,
            ..Default::default()
        }
        .mount(Router::new(), spec);

        let response = router
            .oneshot(Request::builder().uri("/openapi.json").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(body.as_ref(), expected.as_slice(), "the served body is the document's serialization");
    }

    #[test]
    fn endpoint_paths_are_literal_absolute_and_distinct() {
        OpenApiEndpoints::default().validate().unwrap();
        for path in ["/", "/docs/schema.json", "/docs/api's&schema.json"] {
            validate_path("OpenAPI endpoint", path).unwrap();
        }
        for path in [
            "",
            "schema.json",
            "//other-host/schema",
            "/{id}",
            "/{*rest}",
            "/:id",
            "/*rest",
            "/schema?x=1",
            "/schema#part",
            "/has space",
            "/docs/../schema",
            "/docs/%2E./schema",
            "/docs\\schema",
            "/docs/\"schema",
        ] {
            assert!(validate_path("OpenAPI endpoint", path).is_err(), "{path:?}");
        }
        for endpoints in [
            OpenApiEndpoints {
                redoc_path: Some("/openapi.json".into()),
                ..Default::default()
            },
            OpenApiEndpoints {
                redoc_path: Some("/redoc".into()),
                scalar_path: Some("/redoc".into()),
                ..Default::default()
            },
        ] {
            assert!(endpoints.validate().is_err());
        }
        OpenApiEndpoints {
            redoc_path: None,
            scalar_path: None,
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
}
