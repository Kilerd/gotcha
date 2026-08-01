//! Typed header and cookie parameters.
//!
//! axum's [`HeaderMap`](axum::http::HeaderMap) is untyped: a handler can read any header from it,
//! but nothing about it can be documented. [`Header<T>`] and [`Cookie<T>`] name a single parameter
//! through the [`HeaderParam`] / [`CookieParam`] traits, so the same declaration both extracts the
//! value and documents it as an OpenAPI `header` / `cookie` parameter.
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

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// A type that can be parsed from one named HTTP header.
pub trait HeaderParam: Sized {
    /// The header name, matched case-insensitively (HTTP header names are case-insensitive).
    const NAME: &'static str;
    /// Whether the request is rejected when the header is absent. Defaults to `true`.
    const REQUIRED: bool = true;
    /// Description for the generated OpenAPI parameter. Worth setting when the type is a newtype
    /// wrapper, since those are transparent in the schema and their doc comment does not survive.
    const DESCRIPTION: Option<&'static str> = None;
    /// Parse the raw header value, returning a message describing why it was rejected.
    fn parse(raw: &str) -> Result<Self, String>;
    /// The value used when the header is absent and [`REQUIRED`](Self::REQUIRED) is `false`.
    fn missing() -> Option<Self> {
        None
    }
}

/// A type that can be parsed from one named cookie.
pub trait CookieParam: Sized {
    /// The cookie name (case-sensitive, unlike header names).
    const NAME: &'static str;
    /// Whether the request is rejected when the cookie is absent. Defaults to `true`.
    const REQUIRED: bool = true;
    /// Description for the generated OpenAPI parameter. Worth setting when the type is a newtype
    /// wrapper, since those are transparent in the schema and their doc comment does not survive.
    const DESCRIPTION: Option<&'static str> = None;
    /// Parse the raw cookie value, returning a message describing why it was rejected.
    fn parse(raw: &str) -> Result<Self, String>;
    /// The value used when the cookie is absent and [`REQUIRED`](Self::REQUIRED) is `false`.
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
#[derive(Debug)]
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

impl IntoResponse for ParamRejection {
    fn into_response(self) -> Response {
        let body = match self {
            ParamRejection::Missing { kind, name } => format!("missing required {kind} parameter `{name}`"),
            ParamRejection::Invalid { kind, name, message } => format!("invalid {kind} parameter `{name}`: {message}"),
            ParamRejection::NotText { kind, name } => format!("{kind} parameter `{name}` is not valid UTF-8"),
        };
        (StatusCode::BAD_REQUEST, body).into_response()
    }
}

/// Shared by both extractors: turn an optional raw value into `T`, honouring `REQUIRED`.
fn resolve<T>(
    kind: &'static str, name: &'static str, required: bool, raw: Option<&str>, parse: impl FnOnce(&str) -> Result<T, String>,
    missing: impl FnOnce() -> Option<T>,
) -> Result<T, ParamRejection> {
    match raw {
        Some(raw) => parse(raw).map_err(|message| ParamRejection::Invalid { kind, name, message }),
        None if !required => missing().ok_or(ParamRejection::Missing { kind, name }),
        None => Err(ParamRejection::Missing { kind, name }),
    }
}

#[async_trait]
impl<S, T> FromRequestParts<S> for Header<T>
where
    T: HeaderParam,
    S: Send + Sync,
{
    type Rejection = ParamRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let raw = match parts.headers.get(T::NAME) {
            // A header can carry arbitrary bytes; only text can be parsed.
            Some(value) => Some(value.to_str().map_err(|_| ParamRejection::NotText { kind: "header", name: T::NAME })?),
            None => None,
        };
        resolve("header", T::NAME, T::REQUIRED, raw, T::parse, T::missing).map(Header)
    }
}

#[async_trait]
impl<S, T> FromRequestParts<S> for Cookie<T>
where
    T: CookieParam,
    S: Send + Sync,
{
    type Rejection = ParamRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let cookies = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let raw = cookie_value(cookies, T::NAME);
        resolve("cookie", T::NAME, T::REQUIRED, raw, T::parse, T::missing).map(Cookie)
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
    use gotcha_core::oas::{Parameter, ParameterIn, Referenceable, RequestBody};
    use gotcha_core::Schematic;

    use super::{Cookie, CookieParam, Header, HeaderParam};
    use crate::{Either, ParameterProvider};

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
            schema: Some(Referenceable::Data(schema.schema)),
            example: None,
            examples: None,
            content: None,
        }])
    }

    impl<T: HeaderParam + Schematic> ParameterProvider for Header<T> {
        fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
            parameter::<T>(T::NAME, ParameterIn::Header, T::REQUIRED, T::DESCRIPTION)
        }
    }

    impl<T: CookieParam + Schematic> ParameterProvider for Cookie<T> {
        fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
            parameter::<T>(T::NAME, ParameterIn::Cookie, T::REQUIRED, T::DESCRIPTION)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    struct Session(String);

    impl CookieParam for Session {
        const NAME: &'static str = "session";
        fn parse(raw: &str) -> Result<Self, String> {
            Ok(Session(raw.to_string()))
        }
    }

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
    async fn missing_required_header_is_rejected() {
        let mut parts = parts_with(&[]);
        let rejection = Header::<RequestId>::from_request_parts(&mut parts, &()).await.err().expect("must be rejected");
        assert!(matches!(rejection, ParamRejection::Missing { .. }));
        assert_eq!(rejection.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unparseable_header_is_rejected() {
        let mut parts = parts_with(&[("x-request-id", "")]);
        let rejection = Header::<RequestId>::from_request_parts(&mut parts, &()).await.err().expect("must be rejected");
        assert!(matches!(rejection, ParamRejection::Invalid { .. }));
    }

    #[tokio::test]
    async fn optional_header_falls_back() {
        let mut parts = parts_with(&[]);
        let Header(tenant) = Header::<Tenant>::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(tenant.0, "default");
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
