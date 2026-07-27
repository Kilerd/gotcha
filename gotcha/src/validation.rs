//! Request-body validation via the [`validator`](https://docs.rs/validator) crate.
//!
//! [`Valid`] wraps an extractor and validates its decoded value with [`validator::Validate`].
//! The common case is `Valid<Json<T>>`: it extracts the JSON body like [`axum::Json`], then
//! runs `T::validate()` and rejects with `400 Bad Request` (carrying the errors as JSON) when
//! validation fails.
//!
//! Wrapping the extractor (rather than the bare `T`) keeps `Json<T>` unchanged for callers that
//! don't want validation, and leaves room to validate other extractors later. A blanket
//! `impl FromRequest for T where T: Validate` is impossible anyway — `FromRequest` is a foreign
//! trait and `T` is an uncovered type parameter, so the orphan rule (E0210) forbids it.
//!
//! ```ignore
//! use gotcha::{Json, Schematic, Valid, Validate};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize, Schematic, Validate)]
//! struct CreateUser {
//!     #[validate(length(min = 1, max = 64))]
//!     name: String,
//!     #[validate(range(min = 0, max = 150))]
//!     age: u8,
//! }
//!
//! // Rejected with 400 before the handler runs if `name`/`age` are out of bounds.
//! async fn create(Valid(Json(user)): Valid<Json<CreateUser>>) -> String {
//!     format!("created {}", user.name)
//! }
//! ```

use async_trait::async_trait;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use validator::{Validate, ValidationErrors};

/// Extractor that validates another extractor's decoded value with [`validator::Validate`].
///
/// Use `Valid<Json<T>>` to extract and validate a JSON body: it behaves like [`axum::Json`],
/// then calls `T::validate()`. On success the handler receives `Valid(Json(value))`; otherwise
/// the request is rejected with [`ValidRejection`] before the handler runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Valid<E>(pub E);

impl<E> std::ops::Deref for Valid<E> {
    type Target = E;
    fn deref(&self) -> &E {
        &self.0
    }
}

impl<E> std::ops::DerefMut for Valid<E> {
    fn deref_mut(&mut self) -> &mut E {
        &mut self.0
    }
}

/// Rejection produced by the [`Valid`] extractor.
#[derive(Debug)]
pub enum ValidRejection {
    /// The body could not be deserialized; delegates to axum's JSON rejection response.
    Json(JsonRejection),
    /// The body deserialized but failed validation. Rendered as `400` with the errors as JSON.
    Invalid(ValidationErrors),
}

impl IntoResponse for ValidRejection {
    fn into_response(self) -> Response {
        match self {
            ValidRejection::Json(rejection) => rejection.into_response(),
            ValidRejection::Invalid(errors) => (StatusCode::BAD_REQUEST, Json(errors)).into_response(),
        }
    }
}

#[async_trait]
impl<S, T> FromRequest<S> for Valid<Json<T>>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ValidRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let json = Json::<T>::from_request(req, state).await.map_err(ValidRejection::Json)?;
        // `Json<T>` derefs to `T`, so this resolves to `T::validate`.
        json.validate().map_err(ValidRejection::Invalid)?;
        Ok(Valid(json))
    }
}

// For OpenAPI, `Valid<Json<T>>` documents exactly like `Json<T>` — same request body schema.
#[cfg(feature = "openapi")]
impl<T> crate::ParameterProvider for Valid<Json<T>>
where
    T: crate::Schematic,
{
    fn generate(url: String) -> crate::Either<Vec<oas::Parameter>, oas::RequestBody> {
        <crate::Json<T> as crate::ParameterProvider>::generate(url)
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize, Validate)]
    struct Payload {
        #[validate(range(min = 0, max = 150))]
        age: u8,
    }

    fn json_request(body: &str) -> Request {
        Request::builder()
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn valid_body_is_accepted() {
        let extracted = Valid::<Json<Payload>>::from_request(json_request(r#"{"age": 30}"#), &()).await;
        assert!(matches!(extracted, Ok(Valid(Json(Payload { age: 30 })))));
    }

    #[tokio::test]
    async fn invalid_body_is_rejected_with_400() {
        let extracted = Valid::<Json<Payload>>::from_request(json_request(r#"{"age": 200}"#), &()).await;
        let rejection = extracted.err().expect("out-of-range age must be rejected");
        assert!(matches!(rejection, ValidRejection::Invalid(_)));
        assert_eq!(rejection.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn malformed_json_delegates_to_json_rejection() {
        let extracted = Valid::<Json<Payload>>::from_request(json_request("not json"), &()).await;
        assert!(matches!(extracted, Err(ValidRejection::Json(_))));
    }
}
