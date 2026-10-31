//! Request body contracts, independent of data schemas.
//!
//! [`Schematic`] describes the payload; a request body additionally fixes the media type and
//! whether the body is required. [`request_body`] builds one media type entry and
//! [`merge_request_bodies`] combines entries declared with `#[api(request_body(...))]`.

use std::collections::BTreeMap;

use oas::{MediaType, RequestBody};

use crate::Schematic;

/// Describe one media type of a request body.
///
/// The body type's schema is built inside the current collection scope, so it is registered as
/// a component exactly like a response body. `content_type` is the key in the OpenAPI
/// `requestBody.content` map, such as `multipart/form-data`; `description` documents the body as
/// a whole; `required` maps to `requestBody.required`.
pub fn request_body<T: Schematic>(content_type: &str, description: Option<&str>, required: bool) -> RequestBody {
    RequestBody {
        description: description.map(str::to_owned),
        required: Some(required),
        content: BTreeMap::from([(
            content_type.to_owned(),
            MediaType {
                schema: Some(T::generate_schema().schema.into()),
                item_schema: None,
                example: None,
                examples: None,
                encoding: None,
            },
        )]),
    }
}

/// Combine another declared media type into `body`.
///
/// The declarations behind one request body must agree on the description and requiredness, so
/// only the content map grows; the first body's values are kept if they disagree.
/// `#[api(request_body(...))]` enforces the agreement at compile time, while hand-built
/// descriptors hit a debug assertion. Panics when the media type is already present.
pub fn merge_request_bodies(body: &mut RequestBody, other: RequestBody) {
    debug_assert_eq!(body.description, other.description, "request bodies merged into one must agree on description");
    debug_assert_eq!(body.required, other.required, "request bodies merged into one must agree on required");
    for (content_type, media_type) in other.content {
        if body.content.insert(content_type.clone(), media_type).is_some() {
            panic!("duplicate request body content_type: {content_type}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merging_keeps_every_media_type() {
        let mut body = request_body::<String>("application/json", Some("payload"), true);
        merge_request_bodies(&mut body, request_body::<i32>("text/plain", Some("payload"), true));
        let value = serde_json::to_value(body).unwrap();
        assert_eq!(value["description"], "payload");
        assert_eq!(value["required"], true);
        assert_eq!(value["content"]["application/json"]["schema"]["type"], "string");
        assert_eq!(value["content"]["text/plain"]["schema"]["type"], "integer");
    }

    #[test]
    #[should_panic(expected = "duplicate request body content_type")]
    fn merging_the_same_media_type_twice_panics() {
        let mut body = request_body::<String>("application/json", None, true);
        merge_request_bodies(&mut body, request_body::<i32>("application/json", None, true));
    }
}
