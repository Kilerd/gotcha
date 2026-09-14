//! Response contracts and wrappers that keep runtime behavior and documentation together.
use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

#[cfg(feature = "openapi")]
pub use gotcha_core::responsible::{default_response, empty_response, merge_responses, response, Responsible};

#[cfg(feature = "openapi")]
#[doc(hidden)]
pub use gotcha_core::responsible::ResultResponse;

/// Override a response's status both at runtime and in OpenAPI.
///
/// ```rust
/// use gotcha::{Json, WithStatus};
/// let created: WithStatus<_, 201> = WithStatus::new(Json("created"));
/// let no_content: WithStatus<_, 204> = WithStatus::new(());
/// ```
///
/// Statuses must be in 100..=599. Informational responses, 204, 205, and 304 discard the body.
/// Like Axum's `(StatusCode, T)`, this overrides the inner status, including an inner error.
/// Prefer `Result<WithStatus<T, 201>, E>` to keep error statuses independent.
///
/// ```compile_fail
/// use gotcha::WithStatus;
/// let invalid: WithStatus<_, 999> = WithStatus::new(());
/// ```
#[derive(Clone, Debug)]
pub struct WithStatus<T, const STATUS: u16>(T);

impl<T, const STATUS: u16> WithStatus<T, STATUS> {
    const VALID: () = assert!(STATUS >= 100 && STATUS <= 599, "status must be in 100..=599");
    /// Wrap a response with a fixed status.
    pub const fn new(inner: T) -> Self {
        let () = Self::VALID;
        Self(inner)
    }
    /// Recover the wrapped value.
    pub fn into_inner(self) -> T {
        self.0
    }
    fn has_body() -> bool {
        STATUS >= 200 && !matches!(STATUS, 204 | 205 | 304)
    }
}

impl<T: IntoResponse, const STATUS: u16> IntoResponse for WithStatus<T, STATUS> {
    fn into_response(self) -> Response {
        let () = Self::VALID;
        let mut response = self.0.into_response();
        *response.status_mut() = StatusCode::from_u16(STATUS).expect("validated status");
        if !Self::has_body() {
            *response.body_mut() = Body::empty();
            response.headers_mut().remove(header::CONTENT_TYPE);
            response.headers_mut().remove(header::CONTENT_LENGTH);
            response.headers_mut().remove(header::TRANSFER_ENCODING);
        }
        response
    }
}

#[cfg(feature = "openapi")]
impl<T: Responsible, const STATUS: u16> Responsible for WithStatus<T, STATUS> {
    fn response() -> oas::Responses {
        let () = Self::VALID;
        if Self::has_body() {
            gotcha_core::responsible::with_status(T::response(), STATUS)
        } else {
            empty_response(STATUS, "no content")
        }
    }
}
