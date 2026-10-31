//! Request-body validation via the [`validator`](https://docs.rs/validator) crate.
//!
//! [`Valid`] wraps an extractor and validates its decoded value with [`validator::Validate`].
//! The common case is `Valid<Json<T>>`: it extracts the JSON body like [`crate::Json`], then
//! runs `T::validate()` and rejects with `422 Unprocessable Entity` when validation fails,
//! carrying the failures in the same [`ErrorBody`](crate::ErrorBody) shape as every other
//! provider, with the errors flattened into `fields`:
//!
//! ```json
//! {
//!   "error": "validation failed",
//!   "fields": {
//!     "address.city": ["length must be at least 1 and at most 64"],
//!     "items[0].name": ["length must be at least 1"]
//!   }
//! }
//! ```
//!
//! Nested struct fields are joined with `.`, list elements are indexed with `[n]`, and
//! struct-level (whole-object) validations keep validator's `__all__` key. Every message is
//! readable: the one declared via `#[validate(.., message = "..")]`, or a default derived from
//! the error's code and params (`validator` itself leaves `message` null in the serialized form).
//! [`flatten_errors`] exposes the same flattening to custom [`ValidConfig`](crate::ValidConfig)
//! handlers.
//!
//! `Valid<axum::Json<T>>` (using the original Axum extractor) is accepted as well and keeps
//! Axum's original JSON rejection response, so choosing that extractor opts out of the
//! framework's JSON error body.
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
//! #[validate(crate = "gotcha::validator")]
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
//!
//! The crate override uses gotcha's validator re-export, so no direct `validator`
//! dependency is needed. If gotcha is renamed in Cargo.toml, use that name in the
//! path (for example `#[validate(crate = "web::validator")]`).

use std::borrow::Cow;
use std::collections::BTreeMap;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;
use validator::{Validate, ValidationError, ValidationErrors, ValidationErrorsKind};

use crate::{Json, Rejected};

/// Extractor that validates another extractor's decoded value with [`validator::Validate`].
///
/// Use `Valid<Json<T>>` to extract and validate a JSON body: it behaves like [`crate::Json`],
/// then calls `T::validate()`. On success the handler receives `Valid(Json(value))`; otherwise
/// the request is rejected before the handler runs. A malformed body uses the
/// [`JsonConfig`](crate::JsonConfig) handler; a failed validation uses
/// [`ValidConfig`](crate::ValidConfig), which defaults to `422 Unprocessable Entity` with
/// `{"error": "validation failed", "fields": {...}}` (see [`flatten_errors`]).
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

/// Flatten [`ValidationErrors`] into a map from field path to readable messages.
///
/// Keys mirror validator's structure: struct fields are joined with `.` (`address.city`), list
/// elements are indexed with `[n]` (`items[0].name`), and struct-level (whole-object) validations
/// keep validator's own `__all__` key (`__all__` for the validated value itself, or
/// `address.__all__` for a nested struct).
///
/// Any error without a user-supplied message gets the readable default that the default
/// [`ValidConfig`](crate::ValidConfig) rejection uses, so no message is empty. The returned map is
/// ordered by key, so responses are deterministic.
///
/// The default `Valid` rejection body is this map in [`ErrorBody::fields`](crate::ErrorBody).
/// Custom [`ValidConfig::error_handler`](crate::ValidConfig::error_handler) handlers receive the
/// original `ValidationErrors` and can reuse the same flattening:
///
/// ```
/// use gotcha::validation::flatten_errors;
/// use gotcha::validator::{ValidationError, ValidationErrors};
/// use gotcha::{axum::http::StatusCode, ErrorBody};
///
/// fn handler(errors: &ValidationErrors) -> (StatusCode, ErrorBody) {
///     (
///         StatusCode::UNPROCESSABLE_ENTITY,
///         ErrorBody {
///             error: "invalid input".into(),
///             fields: Some(flatten_errors(errors)),
///         },
///     )
/// }
///
/// let mut errors = ValidationErrors::new();
/// errors.add("name", ValidationError::new("required"));
/// assert_eq!(flatten_errors(&errors)["name"], ["is required"]);
/// ```
pub fn flatten_errors(errors: &ValidationErrors) -> BTreeMap<String, Vec<String>> {
    // Reuse the default-message filling so callers get the same readable text as the default
    // rejection, without mutating the borrowed errors.
    let mut filled = errors.clone();
    fill_default_messages(&mut filled);

    let mut fields = BTreeMap::new();
    flatten_into(&filled, "", &mut fields);
    fields
}

fn flatten_into(errors: &ValidationErrors, prefix: &str, fields: &mut BTreeMap<String, Vec<String>>) {
    for (field, kind) in errors.0.iter() {
        let path = if prefix.is_empty() { field.to_string() } else { format!("{prefix}.{field}") };
        match kind {
            ValidationErrorsKind::Field(errors) => {
                let messages = fields.entry(path).or_default();
                messages.extend(errors.iter().map(|error| {
                    error
                        .message
                        .as_ref()
                        .map(|message| message.to_string())
                        .unwrap_or_else(|| default_message(error))
                }));
            }
            ValidationErrorsKind::Struct(nested) => flatten_into(nested, &path, fields),
            ValidationErrorsKind::List(items) => {
                for (index, nested) in items {
                    flatten_into(nested, &format!("{path}[{index}]"), fields);
                }
            }
        }
    }
}

/// The concrete rejection carried by [`Rejected`] from [`Valid`].
///
/// Both `Valid<Json<T>>` and `Valid<axum::extract::Json<T>>` use this type, so a handler can
/// match on the original JSON rejection and the validation errors regardless of flavor.
#[derive(Debug)]
pub enum ValidRejection {
    /// The JSON body could not be extracted.
    Json(JsonRejection),
    /// The body deserialized but failed validation.
    Invalid(ValidationErrors),
}

impl std::fmt::Display for ValidRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidRejection::Json(rejection) => rejection.fmt(formatter),
            ValidRejection::Invalid(_) => formatter.write_str("validation failed"),
        }
    }
}

/// The default `Valid` rejection: send the flattened errors inside an [`ErrorBody`](crate::ErrorBody)
/// as `422 Unprocessable Entity`.
pub(crate) fn validation_error_response(errors: &ValidationErrors) -> Response {
    let body = crate::ErrorBody {
        error: "validation failed".to_string(),
        fields: Some(flatten_errors(errors)),
    };
    crate::error::json_response(StatusCode::UNPROCESSABLE_ENTITY, body)
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
    type Rejection = Rejected<ValidRejection>;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // A malformed body is a `Json` rejection and is rendered by `JsonConfig`; keep that
        // rendering while exposing the original rejection.
        let config = crate::extract::config::configs(req.extensions());
        let Json(value) = <Json<T> as FromRequest<S>>::from_request(req, state).await.map_err(|rejected| {
            let (rejection, response) = rejected.into_parts();
            Rejected::new(ValidRejection::Json(rejection), response)
        })?;
        // Only a body that deserialized and failed validation reaches `ValidConfig`.
        value.validate().map_err(|errors| {
            let response = config.valid.render(&errors);
            Rejected::new(ValidRejection::Invalid(errors), response)
        })?;
        Ok(Valid(Json(value)))
    }
}

impl<S, T> FromRequest<S> for Valid<axum::extract::Json<T>>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = Rejected<ValidRejection>;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // Choosing the Axum original keeps its own rejection response.
        let config = crate::extract::config::configs(req.extensions());
        let axum::extract::Json(value) = <axum::extract::Json<T> as FromRequest<S>>::from_request(req, state)
            .await
            .map_err(|rejection| {
                // Axum's `IntoResponse` renders exactly `(status, body_text)` and consumes the
                // rejection, so reconstruct it to keep the rejection for the caller.
                let response = (rejection.status(), rejection.body_text()).into_response();
                Rejected::new(ValidRejection::Json(rejection), response)
            })?;
        value.validate().map_err(|errors| {
            let response = config.valid.render(&errors);
            Rejected::new(ValidRejection::Invalid(errors), response)
        })?;
        Ok(Valid(axum::extract::Json(value)))
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

#[cfg(feature = "openapi")]
impl<T> crate::ParameterProvider for Valid<axum::extract::Json<T>>
where
    T: crate::Schematic,
{
    fn generate(url: String) -> crate::Either<Vec<oas::Parameter>, oas::RequestBody> {
        <axum::extract::Json<T> as crate::ParameterProvider>::generate(url)
    }
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
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

    #[derive(Deserialize, Validate)]
    struct Address {
        #[validate(length(min = 1, max = 64))]
        city: String,
    }

    #[derive(Deserialize, Validate)]
    struct LineItem {
        #[validate(length(min = 1))]
        name: String,
    }

    #[derive(Deserialize, Validate)]
    #[validate(schema(function = "at_least_one_item"))]
    struct Order {
        #[validate(nested)]
        address: Address,
        #[validate(nested)]
        items: Vec<LineItem>,
    }

    fn at_least_one_item(order: &Order) -> Result<(), ValidationError> {
        if order.items.is_empty() {
            Err(ValidationError::new("required").with_message(Cow::Borrowed("at least one item is required")))
        } else {
            Ok(())
        }
    }

    #[derive(Deserialize, Validate)]
    #[validate(schema(function = "nested_schema_fails"))]
    struct NestedSchema {
        #[allow(dead_code)]
        value: String,
    }

    fn nested_schema_fails(_: &NestedSchema) -> Result<(), ValidationError> {
        Err(ValidationError::new("some_custom_rule"))
    }

    #[derive(Deserialize, Validate)]
    struct Parent {
        #[validate(nested)]
        child: NestedSchema,
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
    async fn invalid_body_is_rejected_with_the_422_error_body() {
        let rejected = Valid::<Json<Payload>>::from_request(json_request(r#"{"age": 200}"#), &())
            .await
            .err()
            .expect("out-of-range age must be rejected");
        assert!(matches!(rejected.rejection(), ValidRejection::Invalid(_)));
        let response = rejected.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response.headers()["content-type"], "application/json");
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "error": "validation failed",
                "fields": {"age": ["must be at least 0 and at most 150"]}
            })
        );
    }

    #[test]
    fn flatten_errors_uses_dotted_and_indexed_paths() {
        let order = Order {
            address: Address { city: String::new() },
            items: vec![LineItem { name: String::new() }, LineItem { name: "ok".into() }],
        };
        let errors = order.validate().unwrap_err();
        let fields = flatten_errors(&errors);
        assert_eq!(fields.len(), 2, "{fields:?}");
        assert_eq!(fields["address.city"], ["length must be at least 1 and at most 64"]);
        assert_eq!(fields["items[0].name"], ["length must be at least 1"]);
        assert!(!fields.contains_key("items[1].name"), "valid list elements contribute no entry");
    }

    #[test]
    fn flatten_errors_keeps_struct_level_errors_under_all() {
        // `validator` stores schema (whole-object) validations under its `__all__` key.
        let order = Order {
            address: Address { city: "Paris".into() },
            items: vec![],
        };
        let errors = order.validate().unwrap_err();
        let fields = flatten_errors(&errors);
        assert_eq!(
            fields,
            BTreeMap::from([("__all__".to_string(), vec!["at least one item is required".to_string()])])
        );
    }

    #[test]
    fn flatten_errors_prefixes_nested_struct_level_errors() {
        let parent = Parent {
            child: NestedSchema { value: "ok".into() },
        };
        let errors = parent.validate().unwrap_err();
        assert_eq!(
            flatten_errors(&errors),
            BTreeMap::from([("child.__all__".to_string(), vec!["failed validation: some_custom_rule".to_string()])])
        );
    }

    #[tokio::test]
    async fn malformed_json_keeps_axums_status_with_a_json_body() {
        let rejected = Valid::<Json<Payload>>::from_request(json_request("not json"), &())
            .await
            .err()
            .expect("malformed JSON must be rejected");
        assert!(matches!(rejected.rejection(), ValidRejection::Json(_)));
        // The default Json handler keeps Axum's status (400 for a syntax error).
        let response = rejected.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["content-type"], "application/json");
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(body["error"].is_string());
    }

    #[tokio::test]
    async fn axum_json_variant_extracts_and_keeps_axum_rejections() {
        let extracted = Valid::<axum::extract::Json<Payload>>::from_request(json_request(r#"{"age": 30}"#), &()).await;
        assert!(matches!(extracted, Ok(Valid(axum::extract::Json(Payload { age: 30 })))));

        let rejected = Valid::<axum::extract::Json<Payload>>::from_request(json_request("not json"), &())
            .await
            .err()
            .expect("malformed JSON must be rejected");
        assert!(matches!(rejected.rejection(), ValidRejection::Json(_)));
        let response = rejected.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            response.headers()["content-type"].to_str().unwrap().starts_with("text/plain"),
            "the Axum variant keeps Axum's rejection response"
        );

        // Validation failures still use the shared 422 error body.
        let rejected = Valid::<axum::extract::Json<Payload>>::from_request(json_request(r#"{"age": 200}"#), &())
            .await
            .err()
            .expect("out-of-range age must be rejected");
        assert!(matches!(rejected.rejection(), ValidRejection::Invalid(_)));
        assert_eq!(rejected.into_response().status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn errors_get_a_readable_default_message() {
        // `validator` leaves `message: None`; `flatten_errors` fills in a description.
        let errors = Payload { age: 200 }.validate().unwrap_err();
        assert!(field_error(&errors, "age").message.is_none(), "validator itself leaves message unset");
        assert_eq!(flatten_errors(&errors)["age"], ["must be at least 0 and at most 150"]);
    }

    #[test]
    fn user_supplied_message_is_preserved() {
        let errors = Annotated { name: "ab".to_string() }.validate().unwrap_err();
        assert_eq!(flatten_errors(&errors)["name"], ["name is too short"]);
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
