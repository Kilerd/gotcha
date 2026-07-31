//! A `Result<Json<T>, E>` handler return type documents both the success response (from `T`)
//! and the error response (from `E`, as the OpenAPI `default` response). Previously the error
//! variant was dropped entirely.

use gotcha::{Json, Responsible, Schematic};
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct User {
    id: u32,
}

/// User lookup failure
#[derive(Schematic, Serialize, Deserialize)]
struct ApiError {
    message: String,
}

fn main() {
    let responses = <Result<Json<User>, ApiError> as Responsible>::response();

    // Success side is still there.
    assert!(responses.data.contains_key("200"), "success (200) response is documented");

    // Error side is now documented as the `default` response, carrying `ApiError`'s schema.
    let default = responses.default.expect("error variant must be documented as the default response");
    if let gotcha::oas::Referenceable::Data(response) = default {
        // Description is taken from the error type's doc comment.
        assert!(response.description.contains("User lookup failure"), "error description comes from the doc comment: {:?}", response.description);
        let content = response.content.expect("error response has a body");
        assert!(content.contains_key("application/json"), "error body is application/json");
    } else {
        panic!("expected an inline default response");
    }

    // axum's `(StatusCode, Json<E>)` error idiom is documented too — the error type `E`'s body is
    // recorded even though the status code is only known at runtime.
    let tuple = <Result<Json<User>, (gotcha::axum::http::StatusCode, Json<ApiError>)> as Responsible>::response();
    assert!(tuple.data.contains_key("200"), "tuple-error idiom keeps the success response");
    assert!(tuple.default.is_some(), "tuple-error idiom documents the error as the default response");
}
