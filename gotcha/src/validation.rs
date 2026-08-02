//! Request-body validation via the [`validator`](https://docs.rs/validator) crate.
//!
//! [`Valid`] wraps an extractor and validates its decoded value with [`validator::Validate`].
//! The common case is `Valid<Json<T>>`: it extracts the JSON body like [`axum::Json`], then
//! runs `T::validate()` and rejects with `422 Unprocessable Entity` when validation fails,
//! carrying the errors as JSON:
//!
//! ```json
//! { "age": [ { "code": "range", "message": "must be at least 0 and at most 150",
//!             "params": { "value": 200, "min": 0.0, "max": 150.0 } } ] }
//! ```
//!
//! Every error carries a readable `message`: the one declared via
//! `#[validate(.., message = "..")]`, or a default derived from the error's code and params
//! (`validator` itself leaves `message` null in the serialized form).
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
//! // Rejected with 422 before the handler runs if `name`/`age` are out of bounds.
//! async fn create(Valid(Json(user)): Valid<Json<CreateUser>>) -> String {
//!     format!("created {}", user.name)
//! }
//! ```

use std::borrow::Cow;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use validator::{Validate, ValidationError, ValidationErrors, ValidationErrorsKind};

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
    /// The body deserialized but failed validation. Rendered as `422 Unprocessable Entity` with
    /// the errors as JSON.
    Invalid(ValidationErrors),
}

impl IntoResponse for ValidRejection {
    fn into_response(self) -> Response {
        match self {
            ValidRejection::Json(rejection) => rejection.into_response(),
            ValidRejection::Invalid(mut errors) => {
                // The body is syntactically valid JSON that failed semantic validation, so `422`
                // is a better fit than `400` (which axum already uses for malformed bodies).
                fill_default_messages(&mut errors);
                (StatusCode::UNPROCESSABLE_ENTITY, Json(errors)).into_response()
            }
        }
    }
}

/// Give every error a non-null `message`.
///
/// `validator` only auto-describes an error in its `Display` impl; the serialized form leaves
/// `message: null` unless the field declares `#[validate(.., message = "..")]`. Since the
/// serialized form is what API clients actually see, fill in a readable default derived from the
/// error's code and params, leaving any user-supplied message untouched.
fn fill_default_messages(errors: &mut ValidationErrors) {
    for kind in errors.0.values_mut() {
        match kind {
            ValidationErrorsKind::Field(field_errors) => {
                for error in field_errors.iter_mut() {
                    if error.message.is_none() {
                        let message = default_message(error);
                        error.message = Some(Cow::Owned(message));
                    }
                }
            }
            ValidationErrorsKind::Struct(nested) => fill_default_messages(nested),
            ValidationErrorsKind::List(items) => {
                for nested in items.values_mut() {
                    fill_default_messages(nested);
                }
            }
        }
    }
}

/// A human-readable description of a validation failure, built from its code and params.
fn default_message(error: &ValidationError) -> String {
    let param = |name: &str| error.params.get(name).map(render_param);

    match error.code.as_ref() {
        "length" => {
            if let Some(equal) = param("equal") {
                return format!("length must be {equal}");
            }
            let lower = param("min").map(|v| format!("at least {v}"));
            let upper = param("max").map(|v| format!("at most {v}"));
            match (lower, upper) {
                (Some(l), Some(u)) => format!("length must be {l} and {u}"),
                (Some(l), None) => format!("length must be {l}"),
                (None, Some(u)) => format!("length must be {u}"),
                (None, None) => "invalid length".to_string(),
            }
        }
        "range" => {
            let lower = param("min")
                .map(|v| format!("at least {v}"))
                .or_else(|| param("exclusive_min").map(|v| format!("greater than {v}")));
            let upper = param("max")
                .map(|v| format!("at most {v}"))
                .or_else(|| param("exclusive_max").map(|v| format!("less than {v}")));
            match (lower, upper) {
                (Some(l), Some(u)) => format!("must be {l} and {u}"),
                (Some(l), None) => format!("must be {l}"),
                (None, Some(u)) => format!("must be {u}"),
                (None, None) => "value out of range".to_string(),
            }
        }
        "email" => "must be a valid email address".to_string(),
        "url" => "must be a valid URL".to_string(),
        "required" | "required_nested" => "is required".to_string(),
        "must_match" => "values do not match".to_string(),
        other => format!("failed validation: {other}"),
    }
}

/// Render a param for display: strings without their JSON quotes, everything else as JSON.
fn render_param(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

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

    #[derive(Deserialize, Validate)]
    struct Annotated {
        #[validate(length(min = 3, message = "name is too short"))]
        name: String,
    }

    /// The single field error recorded for `field`.
    fn field_error<'a>(errors: &'a ValidationErrors, field: &str) -> &'a ValidationError {
        match errors.0.get(field).expect("field has errors") {
            ValidationErrorsKind::Field(errs) => &errs[0],
            _ => panic!("expected field-level errors"),
        }
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
    async fn invalid_body_is_rejected_with_422() {
        let extracted = Valid::<Json<Payload>>::from_request(json_request(r#"{"age": 200}"#), &()).await;
        let rejection = extracted.err().expect("out-of-range age must be rejected");
        assert!(matches!(rejection, ValidRejection::Invalid(_)));
        assert_eq!(rejection.into_response().status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn malformed_json_delegates_to_json_rejection() {
        let extracted = Valid::<Json<Payload>>::from_request(json_request("not json"), &()).await;
        assert!(matches!(extracted, Err(ValidRejection::Json(_))));
    }

    #[test]
    fn errors_get_a_readable_default_message() {
        // `validator` leaves `message: None`; the rejection fills in a description.
        let mut errors = Payload { age: 200 }.validate().unwrap_err();
        assert!(field_error(&errors, "age").message.is_none(), "validator itself leaves message unset");

        fill_default_messages(&mut errors);
        assert_eq!(field_error(&errors, "age").message.as_deref(), Some("must be at least 0 and at most 150"));
    }

    #[test]
    fn user_supplied_message_is_preserved() {
        let mut errors = Annotated { name: "ab".to_string() }.validate().unwrap_err();
        fill_default_messages(&mut errors);
        assert_eq!(field_error(&errors, "name").message.as_deref(), Some("name is too short"));
    }

    #[test]
    fn default_messages_cover_common_codes() {
        let message = |code: &'static str, params: &[(&'static str, serde_json::Value)]| {
            let mut error = ValidationError::new(code);
            for (name, value) in params {
                error.params.insert(Cow::Borrowed(name), value.clone());
            }
            default_message(&error)
        };

        assert_eq!(message("length", &[("equal", 3.into())]), "length must be 3");
        assert_eq!(message("length", &[("min", 1.into())]), "length must be at least 1");
        assert_eq!(
            message("range", &[("exclusive_min", 0.into()), ("exclusive_max", 10.into())]),
            "must be greater than 0 and less than 10"
        );
        assert_eq!(message("email", &[]), "must be a valid email address");
        assert_eq!(message("required", &[]), "is required");
        // Unknown codes (e.g. `custom`) still produce a non-null message.
        assert_eq!(message("some_custom_rule", &[]), "failed validation: some_custom_rule");
    }
}
