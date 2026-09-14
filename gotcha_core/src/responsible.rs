//! HTTP response contracts, independent of data schemas.
//! `Schematic` alone does not imply a status or media type. Implement [`Responsible`] for
//! custom HTTP types using [`response`], [`default_response`], and [`empty_response`].

use crate::Schematic;
use oas::{MediaType, Referenceable, Response, Responses, Schema, SchemaValue};
use std::collections::{btree_map::Entry, BTreeMap};
use std::convert::Infallible;

/// Maps a handler's HTTP return type to its OpenAPI responses.
pub trait Responsible {
    /// The statuses, media types, and bodies this type can produce.
    fn response() -> Responses;
}

/// Macro support for documenting only the `Ok` branch of a `Result`.
/// Using trait resolution also supports type aliases without inspecting their spelling.
#[doc(hidden)]
pub trait ResultResponse {
    fn success_responses() -> Responses;
}

impl<T: Responsible, E> ResultResponse for Result<T, E> {
    fn success_responses() -> Responses {
        T::response()
    }
}

fn body_response(schema: Schema, media_type: &str, description: impl Into<String>) -> Response {
    Response {
        description: description.into(),
        headers: None,
        content: Some(BTreeMap::from([(
            media_type.into(),
            MediaType {
                schema: Some(schema.into()),
                example: None,
                examples: None,
                encoding: None,
                item_schema: None,
            },
        )])),
        links: None,
    }
}

/// Describe a body with a fixed HTTP status. The caller's `IntoResponse` must match this contract.
/// Panics for statuses outside the OpenAPI range 100..=599 or statuses that cannot carry a body;
/// use [`empty_response`] for those statuses.
pub fn response<T: Schematic>(status: u16, media_type: &str, description: impl Into<String>) -> Responses {
    assert!(status >= 200 && !matches!(status, 204 | 205 | 304), "use empty_response for a bodyless status");
    single_response(status, body_response(T::generate_schema().schema, media_type, description))
}

/// Describe a body whose status is only known at runtime, using OpenAPI's `default` entry.
pub fn default_response<T: Schematic>(media_type: &str, description: impl Into<String>) -> Responses {
    Responses {
        default: Some(Referenceable::Data(body_response(T::generate_schema().schema, media_type, description))),
        data: BTreeMap::new(),
    }
}

/// Describe a fixed-status response without a body.
pub fn empty_response(status: u16, description: impl Into<String>) -> Responses {
    single_response(
        status,
        Response {
            description: description.into(),
            headers: None,
            content: None,
            links: None,
        },
    )
}

fn single_response(status: u16, response: Response) -> Responses {
    assert!((100..=599).contains(&status), "response status must be in 100..=599");
    Responses {
        default: None,
        data: BTreeMap::from([(status.to_string(), Referenceable::Data(response))]),
    }
}

fn merge_schemas(target: &mut Option<SchemaValue>, incoming: Option<SchemaValue>) {
    let left = serde_json::to_value(&*target).unwrap();
    let right = serde_json::to_value(&incoming).unwrap();
    if left == right {
        return;
    }
    // An absent schema admits everything. Boolean schemas are actual constraints,
    // and anyOf preserves them along with object schemas and reference siblings.
    *target = if left.is_null() || right.is_null() {
        None
    } else {
        Some(
            Schema {
                extras: BTreeMap::from([("anyOf".into(), serde_json::json!([left, right]))]),
                ..Schema::default()
            }
            .into(),
        )
    };
}

fn merge_response(target: &mut Referenceable<Response>, other: Referenceable<Response>) {
    if serde_json::to_value(&*target).unwrap() == serde_json::to_value(&other).unwrap() {
        return;
    }
    let (Referenceable::Data(target), Referenceable::Data(other)) = (target, other) else {
        panic!("cannot combine distinct response-level references; use inline response definitions");
    };
    if target.description != other.description {
        target.description.push_str(" / ");
        target.description.push_str(&other.description);
    }
    if let Some(headers) = other.headers {
        target.headers.get_or_insert_default().extend(headers);
    }
    if let Some(links) = other.links {
        target.links.get_or_insert_default().extend(links);
    }
    for (media_type, incoming) in other.content.unwrap_or_default() {
        match target.content.get_or_insert_default().entry(media_type) {
            Entry::Vacant(entry) => {
                entry.insert(incoming);
            }
            Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                merge_schemas(&mut current.schema, incoming.schema);
                merge_schemas(&mut current.item_schema, incoming.item_schema);
                if let Some(examples) = incoming.examples {
                    current.examples.get_or_insert_default().extend(examples);
                }
                if let Some(encoding) = incoming.encoding {
                    current.encoding.get_or_insert_default().extend(encoding);
                }
                if incoming.example.is_some() {
                    current.example = incoming.example;
                }
            }
        }
    }
}

/// Union response alternatives. Different schemas for the same status/media type become `anyOf`;
/// identical definitions are reused. This applies to whole-body and stream-item schemas.
/// Later header, link, and example keys win. Distinct response-level
/// `$ref`s cannot be combined and panic; schema references inside inline responses are supported.
pub fn merge_responses(target: &mut Responses, other: Responses) {
    for (status, response) in other.data {
        match target.data.entry(status) {
            Entry::Vacant(entry) => {
                entry.insert(response);
            }
            Entry::Occupied(mut entry) => merge_response(entry.get_mut(), response),
        }
    }
    if let Some(response) = other.default {
        match &mut target.default {
            Some(current) => merge_response(current, response),
            slot => *slot = Some(response),
        }
    }
}

/// Move all response alternatives to one fixed status, combining their bodies.
pub fn with_status(responses: Responses, status: u16) -> Responses {
    assert!((100..=599).contains(&status), "response status must be in 100..=599");
    let mut result = Responses {
        default: None,
        data: BTreeMap::new(),
    };
    for response in responses.data.into_values().chain(responses.default) {
        merge_responses(
            &mut result,
            Responses {
                default: None,
                data: BTreeMap::from([(status.to_string(), response)]),
            },
        );
    }
    result
}

#[cfg(feature = "axum")]
fn dynamic_status(responses: Responses) -> Responses {
    let mut result = with_status(responses, 200);
    result.default = result.data.remove("200");
    result
}

impl Responsible for () {
    fn response() -> Responses {
        empty_response(200, "no content")
    }
}
impl Responsible for Infallible {
    fn response() -> Responses {
        Responses {
            default: None,
            data: BTreeMap::new(),
        }
    }
}

macro_rules! text_response {
    ($($ty:ty),* $(,)?) => { $(impl Responsible for $ty {
        fn response() -> Responses { response::<String>(200, "text/plain", "text response") }
    })* };
}
text_response!(String, &str, Box<str>, std::borrow::Cow<'_, str>);

#[cfg(feature = "axum")]
mod axum_responses {
    use super::*;
    impl<T: Schematic> Responsible for axum::Json<T> {
        fn response() -> Responses {
            response::<T>(200, "application/json", T::doc().unwrap_or_else(|| "JSON response".into()))
        }
    }
    impl<T> Responsible for axum::response::Html<T> {
        fn response() -> Responses {
            response::<String>(200, "text/html", "HTML response")
        }
    }
    macro_rules! binary_response {
        ($($ty:ty),* $(,)?) => { $(impl Responsible for $ty {
            fn response() -> Responses {
                // In OpenAPI 3.2 the media type describes unencoded binary content.
                // A string schema would constrain it as a JSON string instead.
                single_response(200, body_response(Schema::default(), "application/octet-stream", "binary response"))
            }
        })* };
    }
    binary_response!(axum::body::Bytes, Vec<u8>, &[u8], Box<[u8]>, std::borrow::Cow<'_, [u8]>);
    impl<T: Responsible> Responsible for (axum::http::StatusCode, T) {
        fn response() -> Responses {
            dynamic_status(T::response())
        }
    }
    impl Responsible for axum::http::StatusCode {
        fn response() -> Responses {
            dynamic_status(empty_response(200, "runtime status, no content"))
        }
    }
    impl Responsible for axum::response::Response {
        fn response() -> Responses {
            dynamic_status(empty_response(200, "runtime response; body unspecified"))
        }
    }
}

impl<T: Responsible, E: Responsible> Responsible for Result<T, E> {
    fn response() -> Responses {
        let mut responses = T::response();
        merge_responses(&mut responses, E::response());
        responses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_union_preserves_boolean_and_stream_item_schemas() {
        use serde_json::json;
        let make = |schema, item| {
            serde_json::from_value::<Responses>(json!({"200": {
                "description": "stream",
                "content": {"application/jsonl": {"schema": schema, "itemSchema": item}}
            }}))
            .unwrap()
        };
        let item = json!({"$ref": "#/components/schemas/Item", "description": "First item"});
        let mut responses = make(json!(true), item.clone());
        merge_responses(&mut responses, make(json!(false), json!({"type": "null"})));
        let value = serde_json::to_value(&responses).unwrap();
        let media = &value["200"]["content"]["application/jsonl"];
        assert_eq!(media["schema"], json!({"anyOf": [true, false]}));
        assert_eq!(media["itemSchema"], json!({"anyOf": [item, {"type": "null"}]}));

        merge_responses(&mut responses, make(json!(null), json!(null)));
        let value = serde_json::to_value(responses).unwrap();
        let media = &value["200"]["content"]["application/jsonl"];
        assert!(media.get("schema").is_none());
        assert!(media.get("itemSchema").is_none());
    }

    #[test]
    fn success_inference_preserves_the_contract_without_consulting_the_error() {
        struct Success;
        impl Responsible for Success {
            fn response() -> Responses {
                let mut responses = response::<String>(201, "text/plain", "created");
                merge_responses(&mut responses, default_response::<String>("text/plain", "runtime status"));
                responses
            }
        }
        struct Error;
        impl Responsible for Error {
            fn response() -> Responses {
                panic!("explicit errors must bypass the error contract");
            }
        }
        assert_eq!(
            serde_json::to_value(<Result<Success, Error> as ResultResponse>::success_responses()).unwrap(),
            serde_json::to_value(Success::response()).unwrap()
        );
    }

    #[test]
    fn colliding_statuses_union_media_types_and_schemas() {
        let mut responses = response::<String>(200, "application/json", "value");
        merge_responses(&mut responses, response::<String>(200, "application/json", "value"));
        let unchanged = serde_json::to_value(&responses).unwrap();
        assert!(unchanged["200"]["content"]["application/json"]["schema"]["anyOf"].is_null());
        merge_responses(&mut responses, response::<i32>(200, "application/json", "number"));
        merge_responses(&mut responses, response::<String>(200, "text/plain", "text"));
        let json = serde_json::to_value(responses).unwrap();
        assert_eq!(
            json["200"]["content"]["application/json"]["schema"]["anyOf"],
            serde_json::json!([
                {"type":"string"}, {"type":"integer"}
            ])
        );
        assert_eq!(json["200"]["content"]["text/plain"]["schema"]["type"], "string");
    }

    #[test]
    fn fixed_status_collapses_all_alternatives_including_default() {
        let mut responses = response::<String>(200, "text/plain", "success");
        merge_responses(&mut responses, default_response::<i32>("application/json", "dynamic"));
        let responses = with_status(responses, 201);
        assert!(responses.default.is_none());
        assert_eq!(responses.data.keys().collect::<Vec<_>>(), ["201"]);
        let json = serde_json::to_value(responses).unwrap();
        assert_eq!(json["201"]["content"].as_object().unwrap().len(), 2);
    }

    #[test]
    fn unconstrained_schema_stays_unconstrained_when_merged() {
        let mut responses = response::<String>(200, "application/json", "value");
        let mut unknown = responses.clone();
        let Referenceable::Data(response) = unknown.data.get_mut("200").unwrap() else {
            unreachable!()
        };
        response.content.as_mut().unwrap().get_mut("application/json").unwrap().schema = None;
        merge_responses(&mut responses, unknown);
        let json = serde_json::to_value(responses).unwrap();
        assert!(json["200"]["content"]["application/json"]["schema"].is_null());
    }
}
