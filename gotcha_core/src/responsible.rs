//! The [`Responsible`] trait maps a handler return type to its OpenAPI `Responses`.
//!
//! It lives here (rather than in the `gotcha` crate) because it carries a blanket
//! `impl<T: Schematic> Responsible for T`. For that blanket to coexist with the specific
//! `Json<T>` / `Result<T, E>` impls, the compiler must be able to prove that `Json<_>` and
//! `Result<_, _>` do not implement [`Schematic`] — which it can only do in the crate where
//! `Schematic` is defined. The axum-specific `Json<T>` impl is gated behind the optional
//! `axum` feature so this crate stays dependency-light by default.

use std::collections::BTreeMap;

use oas::{MediaType, Referenceable, Response, Responses};

use crate::Schematic;

pub trait Responsible {
    fn response() -> Responses;
}

/// Build a `200 application/json` response whose body schema is `T`'s schema.
fn json_response<T: Schematic>() -> Responses {
    let response_schema = T::generate_schema();
    let mut response = Responses {
        default: None,
        data: BTreeMap::default(),
    };
    response.data.insert(
        "200".to_string(),
        Referenceable::Data(Response {
            description: "default return".to_string(),
            headers: None,
            content: Some(BTreeMap::from([(
                "application/json".to_string(),
                MediaType {
                    schema: Some(Referenceable::Data(response_schema.schema)),
                    example: None,
                    examples: None,
                    encoding: None,
                },
            )])),
            links: None,
        }),
    );
    response
}

// todo: add response for ()

#[cfg(feature = "axum")]
impl<T> Responsible for axum::Json<T>
where
    T: Schematic,
{
    fn response() -> Responses {
        json_response::<T>()
    }
}

impl<T> Responsible for T
where
    T: Schematic,
{
    fn response() -> Responses {
        json_response::<T>()
    }
}

/// Documents the error side of a `Result<T, E>` handler return type.
///
/// It contributes the error response(s) to the operation's [`Responses`] — as the OpenAPI
/// `default` response, which covers any status code not otherwise listed. Implemented for any
/// `E: Schematic` and for axum's `(StatusCode, Json<E>)` idiom; custom error types can implement
/// it directly.
pub trait ErrorResponsible {
    fn error_responses(responses: &mut Responses);
}

/// A `default` response whose JSON body is `E`'s schema (description taken from `E`'s doc comment).
fn schema_error_response<E: Schematic>() -> Referenceable<Response> {
    let schema = E::generate_schema();
    Referenceable::Data(Response {
        description: E::doc().unwrap_or_else(|| "error response".to_string()),
        headers: None,
        content: Some(BTreeMap::from([(
            "application/json".to_string(),
            MediaType {
                schema: Some(Referenceable::Data(schema.schema)),
                example: None,
                examples: None,
                encoding: None,
            },
        )])),
        links: None,
    })
}

impl<E: Schematic> ErrorResponsible for E {
    fn error_responses(responses: &mut Responses) {
        responses.default = Some(schema_error_response::<E>());
    }
}

/// axum's `(StatusCode, Json<E>)` error idiom documents `E`'s body. The status is a runtime value,
/// so the body is recorded as the `default` response.
#[cfg(feature = "axum")]
impl<E: Schematic> ErrorResponsible for (axum::http::StatusCode, axum::Json<E>) {
    fn error_responses(responses: &mut Responses) {
        responses.default = Some(schema_error_response::<E>());
    }
}

impl<T, E> Responsible for Result<T, E>
where
    T: Responsible,
    E: ErrorResponsible,
{
    fn response() -> Responses {
        // Success side (`200`, ...) comes from `T`; the error side `E` documents its own response,
        // so a `Result<Json<T>, ApiError>` handler documents both.
        let mut responses = T::response();
        E::error_responses(&mut responses);
        responses
    }
}
