use axum::{body::Bytes, response::Html, routing::get, Router};
use oas::OpenAPIV3;

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
///     redoc_path: Some("/docs/redoc".into()),
///     scalar_path: None,
/// }));
/// ```
#[derive(Clone, Debug)]
pub struct OpenApiEndpoints {
    /// Path serving the generated JSON document. Defaults to `/openapi.json`.
    pub json_path: String,
    /// Redoc UI path, or `None` to disable it. Defaults to `/redoc`.
    pub redoc_path: Option<String>,
    /// Scalar UI path, or `None` to disable it. Defaults to `/scalar`.
    pub scalar_path: Option<String>,
}

impl Default for OpenApiEndpoints {
    fn default() -> Self {
        Self {
            json_path: "/openapi.json".into(),
            redoc_path: Some("/redoc".into()),
            scalar_path: Some("/scalar".into()),
        }
    }
}

impl OpenApiEndpoints {
    pub(crate) fn validate(&self) -> GotchaResult<()> {
        let paths = [Some(self.json_path.as_str()), self.redoc_path.as_deref(), self.scalar_path.as_deref()];
        for (index, path) in paths.iter().enumerate() {
            if let Some(path) = path {
                validate_path(path)?;
                if paths[..index].contains(&Some(path)) {
                    return Err(GotchaError::message(format!("duplicate OpenAPI endpoint path: {path}")));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn mount(self, mut router: Router, spec: OpenAPIV3) -> Router {
        router = router.route(&self.json_path, get(move || async move { axum::Json(spec.clone()) }));
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

fn validate_path(path: &str) -> GotchaResult<()> {
    let parsed = path.parse::<axum::http::uri::PathAndQuery>();
    if !path.starts_with('/')
        || path.starts_with("//")
        || !path.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@%".contains(&byte))
        || path.split('/').any(|segment| {
            // Browsers normalize literal and percent-encoded dot segments before requesting a URL.
            let decoded = segment.replace("%2e", ".").replace("%2E", ".");
            segment.starts_with([':', '*']) || matches!(decoded.as_str(), "." | "..")
        })
        || !matches!(parsed, Ok(parsed) if parsed.path() == path && parsed.query().is_none())
    {
        return Err(GotchaError::message(format!("OpenAPI endpoint must be a literal absolute URI path: {path:?}")));
    }
    Ok(())
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

    #[test]
    fn endpoint_paths_are_literal_absolute_and_distinct() {
        OpenApiEndpoints::default().validate().unwrap();
        for path in ["/", "/docs/schema.json", "/docs/api's&schema.json"] {
            validate_path(path).unwrap();
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
            assert!(validate_path(path).is_err(), "{path:?}");
        }
        for endpoints in [
            OpenApiEndpoints {
                redoc_path: Some("/openapi.json".into()),
                ..Default::default()
            },
            OpenApiEndpoints {
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
