//! Extractors with configurable rejection responses.
//!
//! `Json`, `Path`, `Query`, `Form`, `Multipart`, and `TypedHeader` replace Axum's (and
//! `axum-extra`'s) extractors at the crate root and in the prelude. They extract exactly like the
//! originals, and each provider renders its rejections through its own config — see
//! [`JsonConfig`] and friends. The defaults keep the original status (`400`, `415`, `422`, ...)
//! and send `{"error": "<message>"}` as JSON.
//!
//! Axum's originals remain available as `gotcha::axum::extract::{Json, Path, Query, Form,
//! Multipart}` and `gotcha::axum_extra::TypedHeader`.
//!
//! ```no_run
//! use gotcha::{Json, Path, Query};
//!
//! async fn handler(Path(id): Path<u32>, Query(filter): Query<Filter>, Json(body): Json<Body>) -> Json<Body> {
//!     # let _ = (id, filter);
//!     Json(body)
//! }
//! # #[derive(serde::Deserialize)] struct Filter { page: u32 }
//! # #[derive(serde::Deserialize, serde::Serialize)] struct Body { name: String }
//! ```

pub(crate) mod config;

use axum::extract::{FromRequest, FromRequestParts, OptionalFromRequest, OptionalFromRequestParts, Request};
use axum::http::request::Parts;
use axum::response::{IntoResponse, IntoResponseParts, Response, ResponseParts};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::HttpError;

/// A rejection that keeps the original Axum rejection next to its rendered response.
///
/// `IntoResponse` returns the rendered response (the provider's configured handler), while
/// [`rejection`](Self::rejection) and [`into_inner`](Self::into_inner) expose the original
/// rejection. This is what makes Axum's per-handler pattern work with Gotcha's wrappers:
///
/// ```
/// use gotcha::{Json, Rejected};
/// use gotcha::axum::extract::rejection::JsonRejection;
///
/// async fn handler(body: Result<Json<u32>, Rejected<JsonRejection>>) -> String {
///     match body {
///         Ok(Json(value)) => value.to_string(),
///         Err(rejected) => rejected.into_inner().body_text(),
///     }
/// }
/// ```
pub struct Rejected<R> {
    rejection: R,
    response: Response,
}

impl<R> Rejected<R> {
    /// Build a rejection from the original rejection and its rendered response.
    pub fn new(rejection: R, response: Response) -> Self {
        Self { rejection, response }
    }

    /// The original rejection.
    pub fn rejection(&self) -> &R {
        &self.rejection
    }

    /// The response that will be sent.
    pub fn response(&self) -> &Response {
        &self.response
    }

    /// Recover the original rejection, discarding the rendered response.
    pub fn into_inner(self) -> R {
        self.rejection
    }

    /// Split into the original rejection and the rendered response.
    pub fn into_parts(self) -> (R, Response) {
        (self.rejection, self.response)
    }
}

impl<R: std::fmt::Debug> std::fmt::Debug for Rejected<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Rejected").field("rejection", &self.rejection).finish_non_exhaustive()
    }
}

impl<R: std::fmt::Display> std::fmt::Display for Rejected<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.rejection.fmt(formatter)
    }
}

impl<R> IntoResponse for Rejected<R> {
    fn into_response(self) -> Response {
        self.response
    }
}

pub use config::{
    reject, CookieConfig, CookieProvider, ErrorBodyResponse, ExtractorConfig, ExtractorConfigs, FormConfig, FormProvider, HeaderConfig, HeaderProvider,
    JsonConfig, JsonProvider, MultipartConfig, MultipartProvider, PathConfig, PathProvider, QueryConfig, QueryProvider, RejectionConfig, RejectionProvider,
    TakenConfigs, TypedHeaderConfig, TypedHeaderProvider, ValidConfig, ValidProvider,
};

/// JSON extractor and response.
///
/// Extracts a JSON body like [`axum::Json`](crate::axum::extract::Json) and deserializes it into
/// `T`. Rejections use [`JsonConfig`]; by default they keep Axum's status and send
/// `{"error": "<message>"}` as JSON.
///
/// As a response it serializes `T` with `Content-Type: application/json`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

/// Path extractor.
///
/// Captures path parameters like [`axum::Path`](crate::axum::extract::Path). Rejections use
/// [`PathConfig`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Path<T>(pub T);

/// Query extractor.
///
/// Deserializes the query string like [`axum::Query`](crate::axum::extract::Query). Rejections use
/// [`QueryConfig`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Query<T>(pub T);

/// URL-encoded form extractor.
///
/// Extracts an `application/x-www-form-urlencoded` body like
/// [`axum::Form`](crate::axum::extract::Form). Rejections use [`FormConfig`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Form<T>(pub T);

/// Multipart form extractor.
///
/// Extracts a `multipart/form-data` body like [`axum::Multipart`](crate::axum::extract::Multipart).
/// Rejections use [`MultipartConfig`].
#[derive(Debug)]
pub struct Multipart(pub axum::extract::Multipart);

/// Typed header extractor and response.
///
/// Extracts a typed header like [`axum_extra::TypedHeader`].
/// Rejections use [`TypedHeaderConfig`].
#[derive(Debug, Clone, Copy)]
pub struct TypedHeader<T>(pub T);

impl<T> std::ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> std::ops::Deref for Path<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Path<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> std::ops::Deref for Query<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Query<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> std::ops::Deref for Form<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Form<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl std::ops::Deref for Multipart {
    type Target = axum::extract::Multipart;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Multipart {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> std::ops::Deref for TypedHeader<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for TypedHeader<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> From<T> for Json<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> Json<T>
where
    T: DeserializeOwned,
{
    /// Deserialize a JSON value from a byte slice.
    ///
    /// This mirrors [`axum::Json::from_bytes`], but rejections are the framework's [`HttpError`]
    /// with Axum's status and body text: `from_bytes` has no request context, so it cannot apply
    /// a configured [`JsonConfig`] handler.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, HttpError> {
        axum::Json::<T>::from_bytes(bytes)
            .map(|axum::Json(value)| Json(value))
            .map_err(|rejection| HttpError::new(rejection.status(), rejection.body_text()))
    }
}

impl<T> Query<T>
where
    T: DeserializeOwned,
{
    /// Deserialize a query string from a URI.
    ///
    /// This mirrors [`axum::extract::Query::try_from_uri`]. Rejections are the framework's
    /// [`HttpError`] with Axum's status and body text: there is no request context, so a
    /// configured [`QueryConfig`] handler cannot be applied.
    pub fn try_from_uri(uri: &axum::http::Uri) -> Result<Self, HttpError> {
        axum::extract::Query::<T>::try_from_uri(uri)
            .map(|axum::extract::Query(value)| Query(value))
            .map_err(|rejection| HttpError::new(rejection.status(), rejection.body_text()))
    }
}

impl<T> IntoResponse for Json<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

impl<T> IntoResponse for Form<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        axum::extract::Form(self.0).into_response()
    }
}

impl<T> IntoResponse for TypedHeader<T>
where
    T: axum_extra::headers::Header,
{
    fn into_response(self) -> Response {
        axum_extra::TypedHeader(self.0).into_response()
    }
}

impl<T> IntoResponseParts for TypedHeader<T>
where
    T: axum_extra::headers::Header,
{
    type Error = std::convert::Infallible;

    fn into_response_parts(self, parts: ResponseParts) -> Result<ResponseParts, Self::Error> {
        axum_extra::TypedHeader(self.0).into_response_parts(parts)
    }
}

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Rejected<axum::extract::rejection::JsonRejection>;

    async fn from_request(mut req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let configs = TakenConfigs::take(req.extensions_mut());
        <axum::Json<T> as FromRequest<S>>::from_request(req, state)
            .await
            .map(|axum::Json(value)| Json(value))
            .map_err(|rejection| configs.reject::<JsonProvider>(rejection))
    }
}

impl<T, S> OptionalFromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Rejected<axum::extract::rejection::JsonRejection>;

    async fn from_request(mut req: Request, state: &S) -> Result<Option<Self>, Self::Rejection> {
        let configs = TakenConfigs::take(req.extensions_mut());
        <axum::Json<T> as OptionalFromRequest<S>>::from_request(req, state)
            .await
            .map(|value| value.map(|axum::Json(value)| Json(value)))
            .map_err(|rejection| configs.reject::<JsonProvider>(rejection))
    }
}

impl<T, S> FromRequest<S> for Form<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Rejected<axum::extract::rejection::FormRejection>;

    async fn from_request(mut req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let configs = TakenConfigs::take(req.extensions_mut());
        <axum::extract::Form<T> as FromRequest<S>>::from_request(req, state)
            .await
            .map(|axum::extract::Form(value)| Form(value))
            .map_err(|rejection| configs.reject::<FormProvider>(rejection))
    }
}

impl<S> FromRequest<S> for Multipart
where
    S: Send + Sync,
{
    type Rejection = Rejected<axum::extract::multipart::MultipartRejection>;

    async fn from_request(mut req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let configs = TakenConfigs::take(req.extensions_mut());
        <axum::extract::Multipart as FromRequest<S>>::from_request(req, state)
            .await
            .map(Multipart)
            .map_err(|rejection| configs.reject::<MultipartProvider>(rejection))
    }
}

impl<S> OptionalFromRequest<S> for Multipart
where
    S: Send + Sync,
{
    type Rejection = Rejected<axum::extract::multipart::MultipartRejection>;

    async fn from_request(mut req: Request, state: &S) -> Result<Option<Self>, Self::Rejection> {
        let configs = TakenConfigs::take(req.extensions_mut());
        <axum::extract::Multipart as OptionalFromRequest<S>>::from_request(req, state)
            .await
            .map(|value| value.map(Multipart))
            .map_err(|rejection| configs.reject::<MultipartProvider>(rejection))
    }
}

impl<T, S> FromRequestParts<S> for Path<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = Rejected<axum::extract::rejection::PathRejection>;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        <axum::extract::Path<T> as FromRequestParts<S>>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Path(value)| Path(value))
            .map_err(|rejection| reject::<PathProvider>(&parts.extensions, rejection))
    }
}

impl<T, S> OptionalFromRequestParts<S> for Path<T>
where
    T: DeserializeOwned + Send + 'static,
    S: Send + Sync,
{
    type Rejection = Rejected<axum::extract::rejection::PathRejection>;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Option<Self>, Self::Rejection> {
        <axum::extract::Path<T> as OptionalFromRequestParts<S>>::from_request_parts(parts, state)
            .await
            .map(|value| value.map(|axum::extract::Path(value)| Path(value)))
            .map_err(|rejection| reject::<PathProvider>(&parts.extensions, rejection))
    }
}

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Rejected<axum::extract::rejection::QueryRejection>;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        <axum::extract::Query<T> as FromRequestParts<S>>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Query(value))
            .map_err(|rejection| reject::<QueryProvider>(&parts.extensions, rejection))
    }
}

impl<T, S> FromRequestParts<S> for TypedHeader<T>
where
    T: axum_extra::headers::Header,
    S: Send + Sync,
{
    type Rejection = Rejected<axum_extra::typed_header::TypedHeaderRejection>;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        <axum_extra::TypedHeader<T> as FromRequestParts<S>>::from_request_parts(parts, state)
            .await
            .map(|axum_extra::TypedHeader(value)| TypedHeader(value))
            .map_err(|rejection| reject::<TypedHeaderProvider>(&parts.extensions, rejection))
    }
}

impl<T, S> OptionalFromRequestParts<S> for TypedHeader<T>
where
    T: axum_extra::headers::Header,
    S: Send + Sync,
{
    type Rejection = Rejected<axum_extra::typed_header::TypedHeaderRejection>;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Option<Self>, Self::Rejection> {
        <axum_extra::TypedHeader<T> as OptionalFromRequestParts<S>>::from_request_parts(parts, state)
            .await
            .map(|value| value.map(|axum_extra::TypedHeader(value)| TypedHeader(value)))
            .map_err(|rejection| reject::<TypedHeaderProvider>(&parts.extensions, rejection))
    }
}

#[cfg(feature = "openapi")]
mod openapi {
    use super::{Form, Json, Multipart, Path, Query, TypedHeader};
    use crate::{Either, ExtractorKind, ParameterProvider, Responsible, Schematic};

    /// Documentation is produced by Axum's extractors, so documents do not change when a handler
    /// switches between the two spellings. The Gotcha wrappers do add their rejection responses;
    /// Axum's originals report no extractor kinds and stay undocumented there.
    impl<T> ParameterProvider for Json<T>
    where
        axum::Json<T>: ParameterProvider,
    {
        fn generate(url: String) -> Either<Vec<oas::Parameter>, oas::RequestBody> {
            <axum::Json<T> as ParameterProvider>::generate(url)
        }

        fn is_request_body() -> bool {
            <axum::Json<T> as ParameterProvider>::is_request_body()
        }

        fn extractor_kinds() -> &'static [ExtractorKind] {
            &[ExtractorKind::Json]
        }
    }

    impl<T> ParameterProvider for Path<T>
    where
        axum::extract::Path<T>: ParameterProvider,
    {
        fn generate(url: String) -> Either<Vec<oas::Parameter>, oas::RequestBody> {
            <axum::extract::Path<T> as ParameterProvider>::generate(url)
        }

        fn extractor_kinds() -> &'static [ExtractorKind] {
            &[ExtractorKind::Path]
        }
    }

    impl<T> ParameterProvider for Query<T>
    where
        axum::extract::Query<T>: ParameterProvider,
    {
        fn generate(url: String) -> Either<Vec<oas::Parameter>, oas::RequestBody> {
            <axum::extract::Query<T> as ParameterProvider>::generate(url)
        }

        fn extractor_kinds() -> &'static [ExtractorKind] {
            &[ExtractorKind::Query]
        }
    }

    impl<T> ParameterProvider for Form<T>
    where
        axum::extract::Form<T>: ParameterProvider,
    {
        fn generate(url: String) -> Either<Vec<oas::Parameter>, oas::RequestBody> {
            <axum::extract::Form<T> as ParameterProvider>::generate(url)
        }

        fn is_request_body() -> bool {
            <axum::extract::Form<T> as ParameterProvider>::is_request_body()
        }

        fn extractor_kinds() -> &'static [ExtractorKind] {
            &[ExtractorKind::Form]
        }
    }

    impl ParameterProvider for Multipart {
        fn generate(url: String) -> Either<Vec<oas::Parameter>, oas::RequestBody> {
            <axum::extract::Multipart as ParameterProvider>::generate(url)
        }

        fn is_request_body() -> bool {
            <axum::extract::Multipart as ParameterProvider>::is_request_body()
        }

        fn extractor_kinds() -> &'static [ExtractorKind] {
            &[ExtractorKind::Multipart]
        }
    }

    impl<T> ParameterProvider for TypedHeader<T>
    where
        axum_extra::TypedHeader<T>: ParameterProvider,
    {
        fn generate(url: String) -> Either<Vec<oas::Parameter>, oas::RequestBody> {
            <axum_extra::TypedHeader<T> as ParameterProvider>::generate(url)
        }

        fn extractor_kinds() -> &'static [ExtractorKind] {
            &[ExtractorKind::TypedHeader]
        }
    }

    impl<T: Schematic> Responsible for Json<T> {
        fn response() -> oas::Responses {
            <axum::Json<T> as Responsible>::response()
        }
    }
}
