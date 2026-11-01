//! Typed header and cookie parameters.
//!
//! axum's [`HeaderMap`](axum::http::HeaderMap) is untyped: a handler can read any header from it,
//! but nothing about it can be documented. [`Header<T>`] and [`Cookie<T>`] name a single parameter
//! through the [`HeaderParam`] / [`CookieParam`] traits, so the same declaration both extracts the
//! value and documents it as an OpenAPI `header` / `cookie` parameter.
//!
//! Both wrappers also accept `Option<Header<T>>` / `Option<Cookie<T>>` as handler arguments: an
//! absent parameter then extracts as `None` and is documented as not required, while a present
//! but unparseable value is still rejected.
//!
//! ```ignore
//! use gotcha::{Header, HeaderParam, Schematic};
//!
//! #[derive(Schematic)]
//! struct RequestId(String);
//!
//! impl HeaderParam for RequestId {
//!     const NAME: &'static str = "x-request-id";
//!     const DESCRIPTION: Option<&'static str> = Some("Correlation id propagated across services");
//!     fn parse(raw: &str) -> Result<Self, String> {
//!         Ok(RequestId(raw.to_string()))
//!     }
//! }
//!
//! async fn handler(Header(id): Header<RequestId>) -> String {
//!     id.0
//! }
//! ```

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};

/// A type that can be parsed from one named HTTP header.
pub trait HeaderParam: Sized {
    /// The header name, matched case-insensitively (HTTP header names are case-insensitive).
    const NAME: &'static str;
    /// Whether the request is rejected when the header is absent. Defaults to `true`.
    ///
    /// Setting this to `false` by itself does not make the parameter optional: the extractor
    /// still rejects an absent header unless [`missing`](Self::missing) returns a value, and the
    /// default `missing` returns `None`. The generated document follows the same rule: the
    /// parameter is documented as required unless `REQUIRED` is `false` *and* `missing()`
    /// returns a value. To let the handler see an absent header as `None`, take
    /// `Option<Header<T>>` as the argument instead; that documents the parameter as
    /// `required: false` and does not consult either item.
    const REQUIRED: bool = true;
    /// Description for the generated OpenAPI parameter. Worth setting when the type is a newtype
    /// wrapper, since those are transparent in the schema and their doc comment does not survive.
    const DESCRIPTION: Option<&'static str> = None;
    /// Parse the raw header value, returning a message describing why it was rejected.
    fn parse(raw: &str) -> Result<Self, String>;
    /// The value used when the header is absent and [`REQUIRED`](Self::REQUIRED) is `false`.
    ///
    /// Returning `None` (the default) keeps the rejection, so `REQUIRED = false` alone still
    /// fails with `400` and the parameter is documented as required. Extract
    /// `Option<Header<T>>` to accept an absent header as `None`.
    fn missing() -> Option<Self> {
        None
    }
}

/// A type that can be parsed from one named cookie.
pub trait CookieParam: Sized {
    /// The cookie name (case-sensitive, unlike header names).
    const NAME: &'static str;
    /// Whether the request is rejected when the cookie is absent. Defaults to `true`.
    ///
    /// Setting this to `false` by itself does not make the parameter optional: the extractor
    /// still rejects an absent cookie unless [`missing`](Self::missing) returns a value, and the
    /// default `missing` returns `None`. The generated document follows the same rule: the
    /// parameter is documented as required unless `REQUIRED` is `false` *and* `missing()`
    /// returns a value. To let the handler see an absent cookie as `None`, take
    /// `Option<Cookie<T>>` as the argument instead; that documents the parameter as
    /// `required: false` and does not consult either item.
    const REQUIRED: bool = true;
    /// Description for the generated OpenAPI parameter. Worth setting when the type is a newtype
    /// wrapper, since those are transparent in the schema and their doc comment does not survive.
    const DESCRIPTION: Option<&'static str> = None;
    /// Parse the raw cookie value, returning a message describing why it was rejected.
    fn parse(raw: &str) -> Result<Self, String>;
    /// The value used when the cookie is absent and [`REQUIRED`](Self::REQUIRED) is `false`.
    ///
    /// Returning `None` (the default) keeps the rejection, so `REQUIRED = false` alone still
    /// fails with `400` and the parameter is documented as required. Extract
    /// `Option<Cookie<T>>` to accept an absent cookie as `None`.
    fn missing() -> Option<Self> {
        None
    }
}

/// Extracts the header named by `T`'s [`HeaderParam`] impl, and documents it as an OpenAPI
/// `header` parameter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Header<T>(pub T);

/// Extracts the cookie named by `T`'s [`CookieParam`] impl, and documents it as an OpenAPI
/// `cookie` parameter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cookie<T>(pub T);

impl<T> std::ops::Deref for Header<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::Deref for Cookie<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Rejection produced by [`Header`] and [`Cookie`].
///
/// This enum is `#[non_exhaustive]`: a later release may add a variant, so a `match` outside this
/// crate needs a wildcard arm.
#[derive(Debug)]
#[non_exhaustive]
pub enum ParamRejection {
    /// A required parameter was absent.
    Missing {
        /// `"header"` or `"cookie"`.
        kind: &'static str,
        /// The parameter name.
        name: &'static str,
    },
    /// The parameter was present but could not be parsed.
    Invalid {
        /// `"header"` or `"cookie"`.
        kind: &'static str,
        /// The parameter name.
        name: &'static str,
        /// Why the value was rejected.
        message: String,
    },
    /// The header value was not valid UTF-8, so it cannot be parsed as text.
    NotText {
        /// `"header"` or `"cookie"`.
        kind: &'static str,
        /// The parameter name.
        name: &'static str,
    },
}

impl std::fmt::Display for ParamRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamRejection::Missing { kind, name } => write!(formatter, "missing required {kind} parameter `{name}`"),
            ParamRejection::Invalid { kind, name, message } => write!(formatter, "invalid {kind} parameter `{name}`: {message}"),
            ParamRejection::NotText { kind, name } => write!(formatter, "{kind} parameter `{name}` is not valid UTF-8"),
        }
    }
}

impl IntoResponse for ParamRejection {
    fn into_response(self) -> Response {
        // The default response for this concrete rejection. A configured `HeaderConfig` or
        // `CookieConfig` can replace it when the extractor renders the rejection.
        crate::error::json_response(axum::http::StatusCode::BAD_REQUEST, crate::ErrorBody::new(self.to_string()))
    }
}

/// Turn a parse failure into the rejection both extractors render, shared by the plain and
/// optional forms.
fn parse_value<T>(kind: &'static str, name: &'static str, raw: &str, parse: impl FnOnce(&str) -> Result<T, String>) -> Result<T, ParamRejection> {
    parse(raw).map_err(|message| ParamRejection::Invalid { kind, name, message })
}

/// Shared by both extractors: turn an optional raw value into `T`, honouring `REQUIRED`.
fn resolve<T>(
    kind: &'static str, name: &'static str, required: bool, raw: Option<&str>, parse: impl FnOnce(&str) -> Result<T, String>,
    missing: impl FnOnce() -> Option<T>,
) -> Result<T, ParamRejection> {
    match raw {
        Some(raw) => parse_value(kind, name, raw, parse),
        None if !required => missing().ok_or(ParamRejection::Missing { kind, name }),
        None => Err(ParamRejection::Missing { kind, name }),
    }
}

impl<S, T> FromRequestParts<S> for Header<T>
where
    T: HeaderParam,
    S: Send + Sync,
{
    type Rejection = crate::Rejected<ParamRejection>;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let raw = match parts.headers.get(T::NAME) {
            // A header can carry arbitrary bytes; only text can be parsed.
            Some(value) => match value.to_str() {
                Ok(value) => Some(value),
                Err(_) => {
                    let rejection = ParamRejection::NotText { kind: "header", name: T::NAME };
                    return Err(crate::extract::reject::<crate::extract::HeaderProvider>(&parts.extensions, rejection));
                }
            },
            None => None,
        };
        // The configured handler is only consulted for a rejection: a successful extraction does
        // not touch the shared `Arc<ExtractorConfigs>`.
        resolve("header", T::NAME, T::REQUIRED, raw, T::parse, T::missing)
            .map(Header)
            .map_err(|rejection| crate::extract::reject::<crate::extract::HeaderProvider>(&parts.extensions, rejection))
    }
}

impl<S, T> FromRequestParts<S> for Cookie<T>
where
    T: CookieParam,
    S: Send + Sync,
{
    type Rejection = crate::Rejected<ParamRejection>;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let cookies = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let raw = cookie_value(cookies, T::NAME);
        // The configured handler is only consulted for a rejection: a successful extraction does
        // not touch the shared `Arc<ExtractorConfigs>`.
        resolve("cookie", T::NAME, T::REQUIRED, raw, T::parse, T::missing)
            .map(Cookie)
            .map_err(|rejection| crate::extract::reject::<crate::extract::CookieProvider>(&parts.extensions, rejection))
    }
}

/// `Option<Header<T>>` reports an absent header as `None`. [`HeaderParam::REQUIRED`] and
/// [`HeaderParam::missing`] shape the plain [`Header<T>`] extractor only; a header that is present
/// but cannot be parsed is still rejected through the configured [`HeaderConfig`](crate::HeaderConfig).
impl<S, T> axum::extract::OptionalFromRequestParts<S> for Header<T>
where
    T: HeaderParam,
    S: Send + Sync,
{
    type Rejection = crate::Rejected<ParamRejection>;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Option<Self>, Self::Rejection> {
        let Some(value) = parts.headers.get(T::NAME) else {
            return Ok(None);
        };
        // A header can carry arbitrary bytes; only text can be parsed.
        let raw = match value.to_str() {
            Ok(value) => value,
            Err(_) => {
                let rejection = ParamRejection::NotText { kind: "header", name: T::NAME };
                return Err(crate::extract::reject::<crate::extract::HeaderProvider>(&parts.extensions, rejection));
            }
        };
        parse_value("header", T::NAME, raw, T::parse)
            .map(|value| Some(Header(value)))
            .map_err(|rejection| crate::extract::reject::<crate::extract::HeaderProvider>(&parts.extensions, rejection))
    }
}

/// `Option<Cookie<T>>` reports an absent cookie as `None`. [`CookieParam::REQUIRED`] and
/// [`CookieParam::missing`] shape the plain [`Cookie<T>`] extractor only; a cookie that is present
/// but cannot be parsed is still rejected through the configured [`CookieConfig`](crate::CookieConfig).
impl<S, T> axum::extract::OptionalFromRequestParts<S> for Cookie<T>
where
    T: CookieParam,
    S: Send + Sync,
{
    type Rejection = crate::Rejected<ParamRejection>;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Option<Self>, Self::Rejection> {
        let raw = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(|cookies| cookie_value(cookies, T::NAME));
        let Some(raw) = raw else {
            return Ok(None);
        };
        parse_value("cookie", T::NAME, raw, T::parse)
            .map(|value| Some(Cookie(value)))
            .map_err(|rejection| crate::extract::reject::<crate::extract::CookieProvider>(&parts.extensions, rejection))
    }
}

/// Find `name` in a `Cookie` header value (`a=1; b=2`).
fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key.trim() == name).then(|| value.trim())
    })
}

#[cfg(feature = "openapi")]
mod openapi {
    use gotcha_core::oas::{Parameter, ParameterIn, RequestBody};
    use gotcha_core::Schematic;

    use super::{Cookie, CookieParam, Header, HeaderParam};
    use crate::{Either, ExtractorKind, ParameterProvider};

    /// Whether an absent parameter is a rejection at runtime, which is what `required` means in
    /// the document: `REQUIRED` is set, or there is no `missing()` fallback to use instead.
    fn is_required<T>(required: bool, missing: impl FnOnce() -> Option<T>) -> bool {
        required || missing().is_none()
    }

    fn parameter<T: Schematic>(name: &'static str, _in: ParameterIn, required: bool, description: Option<&'static str>) -> Either<Vec<Parameter>, RequestBody> {
        let schema = T::generate_schema();
        Either::Left(vec![Parameter {
            name: name.to_string(),
            _in,
            // An explicit `DESCRIPTION` wins; otherwise fall back to the type's own doc comment
            // (which a transparent newtype wrapper will not have).
            description: description.map(str::to_string).or_else(T::doc),
            required: Some(required),
            deprecated: None,
            allow_empty_value: None,
            style: None,
            explode: None,
            allow_reserved: None,
            schema: Some(schema.schema.into()),
            example: None,
            examples: None,
            content: None,
        }])
    }

    impl<T: HeaderParam + Schematic> ParameterProvider for Header<T> {
        fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
            parameter::<T>(T::NAME, ParameterIn::Header, is_required(T::REQUIRED, T::missing), T::DESCRIPTION)
        }

        fn extractor_kinds() -> &'static [ExtractorKind] {
            &[ExtractorKind::Header]
        }
    }

    impl<T: CookieParam + Schematic> ParameterProvider for Cookie<T> {
        fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
            parameter::<T>(T::NAME, ParameterIn::Cookie, is_required(T::REQUIRED, T::missing), T::DESCRIPTION)
        }

        fn extractor_kinds() -> &'static [ExtractorKind] {
            &[ExtractorKind::Cookie]
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::StatusCode;

    use super::*;

    #[derive(crate::Schematic)]
    struct RequestId(String);

    impl HeaderParam for RequestId {
        const NAME: &'static str = "x-request-id";
        fn parse(raw: &str) -> Result<Self, String> {
            if raw.is_empty() {
                return Err("must not be empty".to_string());
            }
            Ok(RequestId(raw.to_string()))
        }
    }

    #[derive(crate::Schematic)]
    struct Session(String);

    impl CookieParam for Session {
        const NAME: &'static str = "session";
        fn parse(raw: &str) -> Result<Self, String> {
            Ok(Session(raw.to_string()))
        }
    }

    #[cfg_attr(feature = "openapi", derive(crate::Schematic))]
    struct Tenant(String);

    impl HeaderParam for Tenant {
        const NAME: &'static str = "x-tenant";
        const REQUIRED: bool = false;
        fn parse(raw: &str) -> Result<Self, String> {
            Ok(Tenant(raw.to_string()))
        }
        fn missing() -> Option<Self> {
            Some(Tenant("default".to_string()))
        }
    }

    /// `REQUIRED = false` with the default `missing()`: the runtime still rejects an absent value.
    #[cfg_attr(feature = "openapi", derive(crate::Schematic))]
    struct NoFallbackHeader(String);

    impl HeaderParam for NoFallbackHeader {
        const NAME: &'static str = "x-no-fallback";
        const REQUIRED: bool = false;
        fn parse(raw: &str) -> Result<Self, String> {
            Ok(NoFallbackHeader(raw.to_string()))
        }
    }

    /// `REQUIRED = false` with the default `missing()`: the runtime still rejects an absent value.
    #[cfg_attr(feature = "openapi", derive(crate::Schematic))]
    struct NoFallbackCookie(String);

    impl CookieParam for NoFallbackCookie {
        const NAME: &'static str = "no-fallback";
        const REQUIRED: bool = false;
        fn parse(raw: &str) -> Result<Self, String> {
            Ok(NoFallbackCookie(raw.to_string()))
        }
    }

    /// `REQUIRED = false` with a fallback: an absent value is accepted.
    #[cfg_attr(feature = "openapi", derive(crate::Schematic))]
    struct FallbackCookie(String);

    impl CookieParam for FallbackCookie {
        const NAME: &'static str = "fallback";
        const REQUIRED: bool = false;
        fn parse(raw: &str) -> Result<Self, String> {
            Ok(FallbackCookie(raw.to_string()))
        }
        fn missing() -> Option<Self> {
            Some(FallbackCookie("default".to_string()))
        }
    }

    fn parts_with(headers: &[(&str, &str)]) -> Parts {
        let mut builder = axum::http::Request::builder();
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(axum::body::Body::empty()).unwrap().into_parts().0
    }

    #[tokio::test]
    async fn header_is_extracted() {
        let mut parts = parts_with(&[("x-request-id", "abc123")]);
        let Header(id) = Header::<RequestId>::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(id.0, "abc123");
    }

    #[tokio::test]
    async fn header_name_is_case_insensitive() {
        let mut parts = parts_with(&[("X-Request-Id", "abc123")]);
        assert!(Header::<RequestId>::from_request_parts(&mut parts, &()).await.is_ok());
    }

    #[tokio::test]
    async fn missing_required_header_is_rejected_with_a_json_body() {
        let mut parts = parts_with(&[]);
        let rejected = Header::<RequestId>::from_request_parts(&mut parts, &()).await.err().expect("must be rejected");
        assert!(matches!(rejected.rejection(), ParamRejection::Missing { .. }));
        let response = rejected.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["content-type"], "application/json");
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"], "missing required header parameter `x-request-id`");
    }

    #[tokio::test]
    async fn unparseable_header_is_rejected() {
        let mut parts = parts_with(&[("x-request-id", "")]);
        let rejected = Header::<RequestId>::from_request_parts(&mut parts, &()).await.err().expect("must be rejected");
        assert!(matches!(rejected.rejection(), ParamRejection::Invalid { .. }));
        let response = rejected.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"], "invalid header parameter `x-request-id`: must not be empty");
    }

    #[tokio::test]
    async fn optional_header_falls_back() {
        let mut parts = parts_with(&[]);
        let Header(tenant) = Header::<Tenant>::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(tenant.0, "default");
    }

    #[tokio::test]
    async fn absent_optional_header_is_none_and_ignores_the_missing_fallback() {
        let mut parts = parts_with(&[]);
        let extracted = Option::<Header<Tenant>>::from_request_parts(&mut parts, &())
            .await
            .expect("an absent optional header is not a rejection");
        assert!(extracted.is_none(), "Option<Header<T>> reports absence as None, not missing()'s default");
    }

    #[tokio::test]
    async fn absent_optional_cookie_is_none() {
        let mut parts = parts_with(&[]);
        let extracted = Option::<Cookie<Session>>::from_request_parts(&mut parts, &())
            .await
            .expect("an absent optional cookie is not a rejection");
        assert!(extracted.is_none());
    }

    #[tokio::test]
    async fn present_optional_header_and_cookie_are_some() {
        let mut parts = parts_with(&[("x-request-id", "abc123")]);
        let Some(Header(id)) = Option::<Header<RequestId>>::from_request_parts(&mut parts, &()).await.unwrap() else {
            panic!("a present header must extract");
        };
        assert_eq!(id.0, "abc123");

        let cookie = format!("a=1; {}=xyz789", Session::NAME);
        let mut parts = parts_with(&[("cookie", cookie.as_str())]);
        let Some(Cookie(session)) = Option::<Cookie<Session>>::from_request_parts(&mut parts, &()).await.unwrap() else {
            panic!("a present cookie must extract");
        };
        assert_eq!(session.0, "xyz789");
    }

    #[tokio::test]
    async fn invalid_optional_header_uses_the_configured_header_rejection() {
        let mut parts = parts_with(&[("x-request-id", "")]);
        parts.extensions.insert(std::sync::Arc::new(crate::ExtractorConfigs::default().set(
            crate::HeaderConfig::default().error_handler(|_: &ParamRejection| (StatusCode::IM_A_TEAPOT, crate::ErrorBody::new("configured header rejection"))),
        )));

        let rejected = Option::<Header<RequestId>>::from_request_parts(&mut parts, &())
            .await
            .err()
            .expect("a present but invalid header is still rejected");
        assert!(matches!(rejected.rejection(), ParamRejection::Invalid { .. }));
        let response = rejected.into_response();
        assert_eq!(response.status(), StatusCode::IM_A_TEAPOT);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["error"],
            "configured header rejection"
        );
    }

    #[tokio::test]
    async fn invalid_optional_cookie_uses_the_configured_cookie_rejection() {
        struct FailingCookie;

        impl CookieParam for FailingCookie {
            const NAME: &'static str = "session";
            fn parse(_: &str) -> Result<Self, String> {
                Err("must be a number".to_string())
            }
        }

        let cookie = format!("{}=abc", FailingCookie::NAME);
        let mut parts = parts_with(&[("cookie", cookie.as_str())]);
        parts.extensions.insert(std::sync::Arc::new(crate::ExtractorConfigs::default().set(
            crate::CookieConfig::default().error_handler(|_: &ParamRejection| (StatusCode::IM_A_TEAPOT, crate::ErrorBody::new("configured cookie rejection"))),
        )));

        let rejected = Option::<Cookie<FailingCookie>>::from_request_parts(&mut parts, &())
            .await
            .err()
            .expect("a present but invalid cookie is still rejected");
        assert!(matches!(rejected.rejection(), ParamRejection::Invalid { .. }));
        assert_eq!(rejected.into_response().status(), StatusCode::IM_A_TEAPOT);
    }

    #[tokio::test]
    async fn required_false_without_a_missing_fallback_still_rejects_an_absent_value() {
        let mut parts = parts_with(&[]);
        let rejected = Header::<NoFallbackHeader>::from_request_parts(&mut parts, &())
            .await
            .err()
            .expect("REQUIRED = false alone does not make the parameter optional");
        assert!(matches!(rejected.rejection(), ParamRejection::Missing { .. }));

        let mut parts = parts_with(&[]);
        let rejected = Cookie::<NoFallbackCookie>::from_request_parts(&mut parts, &())
            .await
            .err()
            .expect("REQUIRED = false alone does not make the parameter optional");
        assert!(matches!(rejected.rejection(), ParamRejection::Missing { .. }));
    }

    #[cfg(feature = "openapi")]
    #[test]
    fn required_parameter_documentation_matches_the_runtime_rule() {
        use crate::{Either, ParameterProvider};

        fn header_required<T: HeaderParam + crate::Schematic>() -> bool {
            let Either::Left(params) = <Header<T> as ParameterProvider>::generate("/documented".to_owned()) else {
                panic!("a header parameter is not a request body");
            };
            assert_eq!(params.len(), 1);
            params[0].required.expect("the document always spells out required")
        }

        fn cookie_required<T: CookieParam + crate::Schematic>() -> bool {
            let Either::Left(params) = <Cookie<T> as ParameterProvider>::generate("/documented".to_owned()) else {
                panic!("a cookie parameter is not a request body");
            };
            assert_eq!(params.len(), 1);
            params[0].required.expect("the document always spells out required")
        }

        assert!(header_required::<RequestId>(), "the default REQUIRED = true is documented");
        assert!(!header_required::<Tenant>(), "REQUIRED = false with a missing() fallback is optional");
        assert!(
            header_required::<NoFallbackHeader>(),
            "REQUIRED = false with the default missing() is rejected at runtime, so it is documented as required"
        );
        assert!(cookie_required::<Session>(), "the default REQUIRED = true is documented");
        assert!(!cookie_required::<FallbackCookie>(), "REQUIRED = false with a missing() fallback is optional");
        assert!(
            cookie_required::<NoFallbackCookie>(),
            "REQUIRED = false with the default missing() is rejected at runtime, so it is documented as required"
        );
    }

    #[cfg(feature = "openapi")]
    #[test]
    fn optional_header_and_cookie_document_as_not_required() {
        use crate::{Either, ParameterProvider};

        let Either::Left(header) = <Option<Header<RequestId>> as ParameterProvider>::generate("/optional".to_owned()) else {
            panic!("a header parameter is not a request body");
        };
        assert_eq!(header.len(), 1);
        assert_eq!(header[0].name, "x-request-id");
        assert_eq!(header[0].required, Some(false), "Option<Header<T>> is documented as not required");

        let Either::Left(cookie) = <Option<Cookie<Session>> as ParameterProvider>::generate("/optional".to_owned()) else {
            panic!("a cookie parameter is not a request body");
        };
        assert_eq!(cookie.len(), 1);
        assert_eq!(cookie[0].name, "session");
        assert_eq!(cookie[0].required, Some(false), "Option<Cookie<T>> is documented as not required");
    }

    #[tokio::test]
    async fn cookie_is_extracted_from_the_cookie_header() {
        let mut parts = parts_with(&[("cookie", "theme=dark; session=xyz789; lang=en")]);
        let Cookie(session) = Cookie::<Session>::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(session.0, "xyz789");
    }

    #[tokio::test]
    async fn missing_cookie_is_rejected() {
        let mut parts = parts_with(&[("cookie", "theme=dark")]);
        assert!(Cookie::<Session>::from_request_parts(&mut parts, &()).await.is_err());
    }

    #[test]
    fn cookie_values_are_split_on_pairs() {
        assert_eq!(cookie_value("a=1; b=2", "b"), Some("2"));
        assert_eq!(cookie_value("a=1", "missing"), None);
        // A cookie value may itself contain `=`.
        assert_eq!(cookie_value("token=abc=def", "token"), Some("abc=def"));
    }
}
