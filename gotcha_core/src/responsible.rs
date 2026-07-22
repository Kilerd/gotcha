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

impl<T, E> Responsible for Result<T, E>
where
    T: Responsible,
{
    fn response() -> Responses {
        // todo: add error response
        T::response()
    }
}
