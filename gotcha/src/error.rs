//! Error types and the JSON error body used by every framework-generated error response.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

use crate::config::ConfigError;

/// The JSON body of every error response Gotcha generates.
///
/// It serializes as `{"error": "..."}`, plus a `fields` map for failures that report more than
/// one problem: `Valid` sends `{"error": "validation failed", "fields": {"name": ["is
/// required"]}}`. `fields` is omitted when there is nothing to report, so every other provider's
/// body is unchanged. The type is public so it can be declared as the body of an OpenAPI response
/// (`body = "ErrorBody"` in `#[api(errors(...))]` / `#[api(responses(...))]`), and so applications
/// can reuse the same wire shape for their own errors.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(gotcha_macro::Schematic))]
pub struct ErrorBody {
    /// Human-readable description of the failure.
    pub error: String,
    /// Per-field failure messages keyed by the field's flattened path (for example `address.city`
    /// or `items[0].name`). Omitted when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<String, Vec<String>>>,
}

/// An HTTP error rendered as `{"error": "<message>"}` with `Content-Type: application/json`.
///
/// Use it as a handler return type or inside `Result<T, HttpError>`:
///
/// ```
/// use gotcha::HttpError;
///
/// let error = HttpError::not_found("user not found");
/// assert_eq!(error.status, gotcha::axum::http::StatusCode::NOT_FOUND);
/// ```
///
/// This is a convenience type, not the framework's rejection mechanism: each extractor's
/// rejection is rendered by its own config (see [`JsonConfig`](crate::JsonConfig) and friends).
/// `HttpError` is useful for application errors and in `#[api(errors(...))]`. Axum's rejected
/// extractor types convert into it:
///
/// ```
/// use gotcha::HttpError;
/// use gotcha::axum::extract::{rejection::PathRejection, Path};
///
/// async fn handler(id: Result<Path<u32>, PathRejection>) -> Result<String, HttpError> {
///     let Path(id) = id?;
///     Ok(id.to_string())
/// }
/// ```
///
/// Statuses that cannot carry a body (`1xx`, `204`, `205`, `304`) are sent without one.
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct HttpError {
    /// The HTTP status to send.
    pub status: StatusCode,
    /// The message sent in the `error` field.
    pub message: String,
}

impl HttpError {
    /// Build an error with an explicit status and message.
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// A `400 Bad Request` error.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// A `404 Not Found` error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    /// A `409 Conflict` error.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    /// A `500 Internal Server Error` error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        json_response(
            self.status,
            ErrorBody {
                error: self.message,
                fields: None,
            },
        )
    }
}

/// Whether `status` can carry a response body, matching [`WithStatus`](crate::WithStatus).
pub(crate) fn status_has_body(status: StatusCode) -> bool {
    status.as_u16() >= 200 && !matches!(status, StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT | StatusCode::NOT_MODIFIED)
}

/// Render `body` as JSON with `status`, removing the body and its headers for statuses that
/// cannot carry one.
pub(crate) fn json_response<E: Serialize>(status: StatusCode, body: E) -> Response {
    let mut response = (status, axum::Json(body)).into_response();
    if !status_has_body(status) {
        *response.body_mut() = axum::body::Body::empty();
        let headers = response.headers_mut();
        headers.remove(axum::http::header::CONTENT_TYPE);
        headers.remove(axum::http::header::CONTENT_LENGTH);
        headers.remove(axum::http::header::TRANSFER_ENCODING);
    }
    response
}

/// Convert Axum's extractor rejections into the framework's JSON error body.
macro_rules! from_axum_rejection {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for HttpError {
                fn from(rejection: $ty) -> Self {
                    Self::new(rejection.status(), rejection.body_text())
                }
            }
        )*
    };
}

from_axum_rejection!(
    axum::extract::rejection::JsonRejection,
    axum::extract::rejection::PathRejection,
    axum::extract::rejection::QueryRejection,
    axum::extract::rejection::FormRejection,
    axum::extract::rejection::ExtensionRejection,
    axum::extract::multipart::MultipartRejection,
);

impl From<axum_extra::typed_header::TypedHeaderRejection> for HttpError {
    fn from(rejection: axum_extra::typed_header::TypedHeaderRejection) -> Self {
        // `TypedHeaderRejection` always responds with `400`; its `Display` is the body text.
        Self::bad_request(rejection.to_string())
    }
}

#[cfg(feature = "openapi")]
impl crate::Schematic for HttpError {
    fn name() -> &'static str {
        <ErrorBody as crate::Schematic>::name()
    }

    fn schema_name() -> Option<&'static str> {
        <ErrorBody as crate::Schematic>::schema_name()
    }

    fn required() -> bool {
        <ErrorBody as crate::Schematic>::required()
    }

    fn nullable() -> Option<bool> {
        <ErrorBody as crate::Schematic>::nullable()
    }

    fn type_() -> &'static str {
        <ErrorBody as crate::Schematic>::type_()
    }

    fn doc() -> Option<String> {
        <ErrorBody as crate::Schematic>::doc()
    }

    fn format() -> Option<String> {
        <ErrorBody as crate::Schematic>::format()
    }

    fn fields() -> Vec<(&'static str, crate::EnhancedSchema)> {
        <ErrorBody as crate::Schematic>::fields()
    }

    fn generate_schema() -> crate::EnhancedSchema {
        <ErrorBody as crate::Schematic>::generate_schema()
    }

    fn flatten_schema() -> Option<serde_json::Value> {
        <ErrorBody as crate::Schematic>::flatten_schema()
    }
}

/// `HttpError` carries a runtime status, so it documents as an OpenAPI `default` response.
#[cfg(feature = "openapi")]
impl crate::Responsible for HttpError {
    fn response() -> crate::oas::Responses {
        crate::response::default_response::<ErrorBody>("application/json", "error response")
    }
}

/// The error type returned by framework operations (`run`, `listen`, config
/// loading, …). It implements [`IntoResponse`], so it can also be returned
/// directly from handlers, where it renders as a `500 Internal Server Error` with the body
/// `{"error": "internal server error"}`. The full error is logged with `tracing::error!`; its
/// details are not sent to the client.
#[derive(Debug, Error)]
pub enum GotchaError {
    /// Configuration could not be loaded or parsed.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// The configured listen address was not a valid socket address.
    #[error("invalid listen address: {0}")]
    InvalidAddress(String),

    /// The server failed to bind to the given address.
    #[error("failed to bind server to {addr}: {source}")]
    Bind {
        /// The address the server tried to bind to.
        addr: String,
        /// Why the bind failed.
        source: std::io::Error,
    },

    /// An I/O error from the runtime (e.g. `axum::serve`).
    #[error(transparent)]
    Io(std::io::Error),

    /// A generic, framework-level error message.
    #[error("{0}")]
    Message(String),

    /// Any other error — typically produced by a user-provided hook.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl GotchaError {
    /// Build a [`GotchaError::Message`] from anything `Display`.
    pub fn message(msg: impl std::fmt::Display) -> Self {
        Self::Message(msg.to_string())
    }
}

/// Convenient result alias for framework operations.
pub type GotchaResult<T> = Result<T, GotchaError>;

impl IntoResponse for GotchaError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self, "request failed with a gotcha error");
        // Internal details (config paths, bind addresses, I/O errors) stay in the log. The client
        // only learns that the request failed; leaking the message could disclose deployment data.
        HttpError::internal("internal server error").into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn renders_as_internal_server_error_without_leaking_details() {
        let response = GotchaError::message("secret config at /etc/gotcha.toml").into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response.headers()["content-type"], "application/json");
        assert_eq!(body_json(response).await, serde_json::json!({"error": "internal server error"}));
    }

    #[tokio::test]
    async fn http_error_renders_status_and_json_body() {
        for (error, status) in [
            (HttpError::bad_request("bad"), StatusCode::BAD_REQUEST),
            (HttpError::not_found("missing"), StatusCode::NOT_FOUND),
            (HttpError::conflict("duplicate"), StatusCode::CONFLICT),
            (HttpError::internal("oops"), StatusCode::INTERNAL_SERVER_ERROR),
            (HttpError::new(StatusCode::IM_A_TEAPOT, "teapot"), StatusCode::IM_A_TEAPOT),
        ] {
            let response = error.clone().into_response();
            assert_eq!(response.status(), status);
            assert_eq!(response.headers()["content-type"], "application/json");
            assert_eq!(body_json(response).await, serde_json::json!({"error": error.message}));
        }
    }

    #[tokio::test]
    async fn bodyless_statuses_drop_the_json_body() {
        for status in [StatusCode::NO_CONTENT, StatusCode::RESET_CONTENT, StatusCode::NOT_MODIFIED] {
            let response = HttpError::new(status, "boom").into_response();
            assert_eq!(response.status(), status);
            assert!(response.headers().get("content-type").is_none(), "{status} must not advertise JSON");
            assert!(response.headers().get("content-length").is_none());
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            assert!(bytes.is_empty(), "{status} must not carry the error body");
        }
    }

    #[test]
    fn axum_rejection_types_convert_into_http_error() {
        fn assert_into<T: Into<HttpError>>() {}
        assert_into::<axum::extract::rejection::JsonRejection>();
        assert_into::<axum::extract::rejection::PathRejection>();
        assert_into::<axum::extract::rejection::QueryRejection>();
        assert_into::<axum::extract::rejection::FormRejection>();
        assert_into::<axum::extract::rejection::ExtensionRejection>();
        assert_into::<axum::extract::multipart::MultipartRejection>();
        assert_into::<axum_extra::typed_header::TypedHeaderRejection>();
    }

    #[tokio::test]
    async fn typed_header_rejection_converts_into_http_error() {
        use axum::extract::FromRequestParts;

        let mut parts = axum::http::Request::builder().body(axum::body::Body::empty()).unwrap().into_parts().0;
        let rejection = axum_extra::TypedHeader::<axum_extra::headers::Host>::from_request_parts(&mut parts, &())
            .await
            .err()
            .expect("the Host header is absent");
        let error = HttpError::from(rejection);
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(!error.message.is_empty());
    }

    #[test]
    fn config_error_converts() {
        let err: GotchaError = ConfigError::Error("bad".into()).into();
        assert!(matches!(err, GotchaError::Config(_)));
    }

    #[test]
    fn error_body_omits_fields_when_none() {
        let body = ErrorBody {
            error: "bad".into(),
            fields: None,
        };
        assert_eq!(serde_json::to_value(body).unwrap(), serde_json::json!({"error": "bad"}));
    }

    #[test]
    fn error_body_serializes_fields_when_present() {
        let body = ErrorBody {
            error: "validation failed".into(),
            fields: Some(BTreeMap::from([
                ("address.city".into(), vec!["is required".into()]),
                ("items[0].name".into(), vec!["too short".into(), "must not be empty".into()]),
            ])),
        };
        assert_eq!(
            serde_json::to_value(body).unwrap(),
            serde_json::json!({
                "error": "validation failed",
                "fields": {
                    "address.city": ["is required"],
                    "items[0].name": ["too short", "must not be empty"]
                }
            })
        );
    }

    #[cfg(feature = "openapi")]
    #[test]
    fn error_body_documents_fields_as_an_optional_object_of_string_arrays() {
        let schema = serde_json::to_value(<ErrorBody as crate::Schematic>::generate_schema().schema).unwrap();
        assert_eq!(schema["required"], serde_json::json!(["error"]));
        assert_eq!(schema["properties"]["error"]["type"], "string");
        // `skip_serializing_if = "Option::is_none"` means optional with no null branch.
        assert!(schema["properties"]["fields"].get("anyOf").is_none());
        assert_eq!(schema["properties"]["fields"]["type"], "object");
        assert_eq!(schema["properties"]["fields"]["additionalProperties"]["type"], "array");
        assert_eq!(schema["properties"]["fields"]["additionalProperties"]["items"]["type"], "string");
    }
}
