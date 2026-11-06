//! HTTP response contracts, independent of data schemas.
//! `Schematic` alone does not imply a status or media type. Implement [`Responsible`] for
//! custom HTTP types using [`response`], [`default_response`], and [`empty_response`].

use crate::{registry, Schematic};
use oas::{MediaType, Referenceable, Response, Responses, Schema, SchemaValue};
use std::collections::{btree_map::Entry, BTreeMap};
use std::convert::Infallible;

/// Maps a handler's HTTP return type to its OpenAPI responses.
pub trait Responsible {
    /// The fixed statuses [`Self::response`] places in `Responses::data`.
    ///
    /// The `#[api(responses(STATUS = "description"))]` shorthand reads this in a `const`
    /// context to reject an override of a status the return type never infers, so a custom
    /// implementation that documents fixed statuses must declare them here. The default is
    /// empty.
    const STATUSES: Statuses = Statuses::NONE;

    /// Whether [`Self::response`] returns a `default` response for statuses known only at
    /// runtime.
    ///
    /// The `#[api(errors(STATUS = "description"))]` shorthand reads this in a `const` context:
    /// it accepts a status that is not in [`Self::STATUSES`] when the error type has a default
    /// response to move to that status. The default is `false`.
    const HAS_DEFAULT: bool = false;

    /// The statuses, media types, and bodies this type can produce.
    fn response() -> Responses;
}

/// The fixed statuses a [`Responsible`] implementation documents.
///
/// Kept as a bitset so the set can be unioned and queried in a `const` context, where the
/// `#[api]` response shorthand performs its compile-time checks. Statuses outside
/// `100..=599` are ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Statuses {
    /// One bit per status, offset from 100: status 100 is bit 0 and status 599 is bit 499.
    bits: [u64; 8],
}

impl Statuses {
    /// No fixed statuses.
    pub const NONE: Self = Self { bits: [0; 8] };

    /// Build a set from status codes.
    pub const fn of(statuses: &[u16]) -> Self {
        let mut set = Self::NONE;
        let mut index = 0;
        while index < statuses.len() {
            set = set.with(statuses[index]);
            index += 1;
        }
        set
    }

    /// Add one status.
    pub const fn with(self, status: u16) -> Self {
        if status < 100 || status > 599 {
            return self;
        }
        let offset = (status - 100) as usize;
        let mut bits = self.bits;
        bits[offset / 64] |= 1 << (offset % 64);
        Self { bits }
    }

    /// Combine two sets.
    pub const fn union(self, other: Self) -> Self {
        let mut bits = self.bits;
        let mut index = 0;
        while index < bits.len() {
            bits[index] |= other.bits[index];
            index += 1;
        }
        Self { bits }
    }

    /// Whether `status` is in the set.
    pub const fn contains(self, status: u16) -> bool {
        if status < 100 || status > 599 {
            return false;
        }
        let offset = (status - 100) as usize;
        self.bits[offset / 64] & (1 << (offset % 64)) != 0
    }

    /// Whether the set is empty.
    pub const fn is_empty(self) -> bool {
        let mut index = 0;
        while index < self.bits.len() {
            if self.bits[index] != 0 {
                return false;
            }
            index += 1;
        }
        true
    }
}

/// Macro support for documenting only the `Ok` branch of a `Result`.
/// Using trait resolution also supports type aliases without inspecting their spelling.
#[doc(hidden)]
pub trait ResultResponse {
    /// The success branch's fixed statuses.
    const SUCCESS_STATUSES: Statuses = Statuses::NONE;

    fn success_responses() -> Responses;
}

impl<T: Responsible, E> ResultResponse for Result<T, E> {
    const SUCCESS_STATUSES: Statuses = T::STATUSES;

    fn success_responses() -> Responses {
        T::response()
    }
}

/// Macro support for documenting only the `Err` branch of a `Result`.
/// Using trait resolution also supports type aliases without inspecting their spelling.
#[doc(hidden)]
pub trait ErrorResponse {
    /// The error branch's fixed statuses.
    const STATUSES: Statuses = Statuses::NONE;

    /// Whether the error branch has a `default` response.
    const HAS_DEFAULT: bool = false;

    fn error_responses() -> Responses;
}

impl<T, E: Responsible> ErrorResponse for Result<T, E> {
    const STATUSES: Statuses = E::STATUSES;
    const HAS_DEFAULT: bool = E::HAS_DEFAULT;

    fn error_responses() -> Responses {
        E::response()
    }
}

fn content_response(schema: Option<SchemaValue>, item_schema: Option<SchemaValue>, media_type: &str, description: impl Into<String>) -> Response {
    Response {
        description: description.into(),
        headers: None,
        content: Some(BTreeMap::from([(
            media_type.into(),
            MediaType {
                schema,
                example: None,
                examples: None,
                encoding: None,
                item_schema,
            },
        )])),
        links: None,
    }
}

fn body_response(schema: Schema, media_type: &str, description: impl Into<String>) -> Response {
    content_response(Some(schema.into()), None, media_type, description)
}

/// Describe a body with a fixed HTTP status. The caller's `IntoResponse` must match this contract.
/// Panics for statuses outside the OpenAPI range 100..=599 or statuses that cannot carry a body;
/// use [`empty_response`] for those statuses.
pub fn response<T: Schematic>(status: u16, media_type: &str, description: impl Into<String>) -> Responses {
    assert!(status >= 200 && !matches!(status, 204 | 205 | 304), "use empty_response for a bodyless status");
    single_response(status, body_response(T::generate_schema().schema, media_type, description))
}

/// Describe a sequential response with a fixed status, where `T` is the OpenAPI 3.2
/// `itemSchema` for each parsed item rather than a whole-body schema.
///
/// Use this for server-sent events (`text/event-stream`) and newline-delimited payloads such as
/// `application/jsonl`. Set the media type explicitly: the media type name does not imply an item
/// schema, and an item schema alone does not describe the complete body.
pub fn item_response<T: Schematic>(status: u16, media_type: &str, description: impl Into<String>) -> Responses {
    assert!(status >= 200 && !matches!(status, 204 | 205 | 304), "use empty_response for a bodyless status");
    single_response(
        status,
        content_response(None, Some(T::generate_schema().schema.into()), media_type, description),
    )
}

/// Describe a response with both a whole-body schema (`B`) and a per-item schema (`I`).
///
/// The two schemas are independent, matching OpenAPI 3.2's separate `schema` and `itemSchema`
/// fields. A media type may use either one alone; use [`response`] or [`item_response`] then.
pub fn response_with_item<B: Schematic, I: Schematic>(status: u16, media_type: &str, description: impl Into<String>) -> Responses {
    assert!(status >= 200 && !matches!(status, 204 | 205 | 304), "use empty_response for a bodyless status");
    single_response(
        status,
        content_response(
            Some(B::generate_schema().schema.into()),
            Some(I::generate_schema().schema.into()),
            media_type,
            description,
        ),
    )
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

/// Replace the description of the response inferred for `status`.
///
/// Backs the `#[api(responses(STATUS = "description"))]` shorthand. Panics when the inferred
/// contract has no response for that status or when that response is a `$ref`; the panic message
/// says which declaration to use instead.
#[doc(hidden)]
pub fn describe_response(responses: &mut Responses, status: u16, description: impl Into<String>) {
    let Some(response) = responses.data.get_mut(&status.to_string()) else {
        panic!("no response is inferred for status {status}; declare it with response(status = {status}, ...)");
    };
    let Referenceable::Data(response) = response else {
        panic!("the response inferred for status {status} is a reference; declare it with response(status = {status}, ...)");
    };
    response.description = description.into();
}

/// Merge the error type's documented response for `status`, replacing its description.
///
/// Backs the `#[api(errors(STATUS = "description"))]` shorthand. The body and content type come
/// from the error type's fixed response for `status`, or from its `default` response when the
/// error type has no fixed status. Panics when the error type documents neither.
///
/// The response is built through a closure so that, inside a schema collection scope, the
/// schemas it registers and the chosen entry does not reference can be discarded again. Building
/// every error response would otherwise register components a `response(...)` declaration with
/// the same body never emits.
#[doc(hidden)]
pub fn describe_error_response(responses: &mut Responses, error_responses: impl FnOnce() -> Responses, status: u16, description: impl Into<String>) {
    let checkpoint = registry::checkpoint();
    let error_responses = error_responses();
    let response = error_responses
        .data
        .get(&status.to_string())
        .cloned()
        .or_else(|| error_responses.default.clone());
    let Some(mut response) = response else {
        panic!("the error type documents no response for status {status} and no default response");
    };
    registry::discard_unreferenced(checkpoint, &response);
    let Referenceable::Data(response_data) = &mut response else {
        panic!("the error type's response for status {status} is a reference");
    };
    response_data.description = description.into();
    merge_responses(
        responses,
        Responses {
            default: None,
            data: BTreeMap::from([(status.to_string(), response)]),
        },
    );
}

#[cfg(feature = "axum")]
fn dynamic_status(responses: Responses) -> Responses {
    let mut result = with_status(responses, 200);
    result.default = result.data.remove("200");
    result
}

impl Responsible for () {
    const STATUSES: Statuses = Statuses::of(&[200]);

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
        const STATUSES: Statuses = Statuses::of(&[200]);

        fn response() -> Responses { response::<String>(200, "text/plain", "text response") }
    })* };
}
text_response!(String, &str, Box<str>, std::borrow::Cow<'_, str>);

#[cfg(feature = "axum")]
mod axum_responses {
    use super::*;
    impl<T: Schematic> Responsible for axum::Json<T> {
        const STATUSES: Statuses = Statuses::of(&[200]);

        fn response() -> Responses {
            response::<T>(200, "application/json", T::doc().unwrap_or_else(|| "JSON response".into()))
        }
    }
    impl<T> Responsible for axum::response::Html<T> {
        const STATUSES: Statuses = Statuses::of(&[200]);

        fn response() -> Responses {
            response::<String>(200, "text/html", "HTML response")
        }
    }
    macro_rules! binary_response {
        ($($ty:ty),* $(,)?) => { $(impl Responsible for $ty {
            const STATUSES: Statuses = Statuses::of(&[200]);

            fn response() -> Responses {
                // In OpenAPI 3.2 the media type describes unencoded binary content.
                // A string schema would constrain it as a JSON string instead.
                single_response(200, body_response(Schema::default(), "application/octet-stream", "binary response"))
            }
        })* };
    }
    binary_response!(axum::body::Bytes, Vec<u8>, &[u8], Box<[u8]>, std::borrow::Cow<'_, [u8]>);
    impl<T: Responsible> Responsible for (axum::http::StatusCode, T) {
        // `dynamic_status` moves everything to `default`, so this documents no fixed status,
        // and it documents nothing at all when the inner type documents nothing.
        const HAS_DEFAULT: bool = !T::STATUSES.is_empty() || T::HAS_DEFAULT;

        fn response() -> Responses {
            dynamic_status(T::response())
        }
    }
    impl Responsible for axum::http::StatusCode {
        const HAS_DEFAULT: bool = true;

        fn response() -> Responses {
            dynamic_status(empty_response(200, "runtime status, no content"))
        }
    }
    impl Responsible for axum::response::Response {
        const HAS_DEFAULT: bool = true;

        fn response() -> Responses {
            dynamic_status(empty_response(200, "runtime response; body unspecified"))
        }
    }
}

impl<T: Responsible, E: Responsible> Responsible for Result<T, E> {
    const STATUSES: Statuses = T::STATUSES.union(E::STATUSES);
    const HAS_DEFAULT: bool = T::HAS_DEFAULT || E::HAS_DEFAULT;

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
    fn status_sets_union_and_contain_only_response_statuses() {
        let set = Statuses::of(&[200, 404]).union(Statuses::of(&[201]));
        for status in [200, 201, 404] {
            assert!(set.contains(status), "{status}");
        }
        for status in [99, 500, 600] {
            assert!(!set.contains(status), "{status}");
        }
        assert_eq!(Statuses::NONE, Statuses::of(&[99, 600]));
    }

    #[test]
    fn describe_response_reworks_an_inferred_entry_and_reports_a_missing_one() {
        let mut responses = response::<String>(200, "text/plain", "old");
        describe_response(&mut responses, 200, "new");
        assert_eq!(serde_json::to_value(&responses).unwrap()["200"]["description"], "new");

        let missing = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut responses = response::<String>(200, "text/plain", "old");
            describe_response(&mut responses, 404, "new");
        }));
        assert!(missing.is_err());
    }

    #[test]
    fn describe_error_response_uses_the_error_types_default_response() {
        struct WidgetError;
        impl Responsible for WidgetError {
            const HAS_DEFAULT: bool = true;

            fn response() -> Responses {
                default_response::<String>("application/json", "error")
            }
        }

        let mut responses = response::<String>(200, "application/json", "ok");
        describe_error_response(&mut responses, WidgetError::response, 404, "missing");
        let json = serde_json::to_value(&responses).unwrap();
        assert_eq!(json["404"]["description"], "missing");
        assert_eq!(json["404"]["content"]["application/json"]["schema"]["type"], "string");
        // The error's runtime status was moved to the requested status, not kept as a default.
        assert!(json.get("default").is_none());
    }

    #[test]
    fn result_statuses_union_both_branches() {
        struct Failure;
        impl Responsible for Failure {
            const STATUSES: Statuses = Statuses::of(&[409]);

            fn response() -> Responses {
                response::<String>(409, "application/json", "conflict")
            }
        }

        let set = <Result<String, Failure> as Responsible>::STATUSES;
        assert!(set.contains(200));
        assert!(set.contains(409));
        assert!(!<Result<String, Failure> as Responsible>::HAS_DEFAULT);
        assert_eq!(<Result<String, Failure> as ResultResponse>::SUCCESS_STATUSES, Statuses::of(&[200]));
        assert_eq!(<Result<String, Failure> as ErrorResponse>::STATUSES, Statuses::of(&[409]));
        assert!(!<Result<String, Failure> as ErrorResponse>::HAS_DEFAULT);
    }

    #[cfg(feature = "axum")]
    #[test]
    fn a_dynamic_status_documents_a_default_only_when_the_inner_type_does() {
        assert!(<(axum::http::StatusCode, String) as Responsible>::HAS_DEFAULT);
        assert!(<(axum::http::StatusCode, Infallible) as Responsible>::STATUSES.is_empty());
        assert!(!<(axum::http::StatusCode, Infallible) as Responsible>::HAS_DEFAULT);
    }

    #[test]
    fn the_error_branch_reports_its_default_response_to_the_macro() {
        struct Failure;
        impl Responsible for Failure {
            const HAS_DEFAULT: bool = true;

            fn response() -> Responses {
                default_response::<String>("application/json", "runtime status")
            }
        }

        assert!(<Result<String, Failure> as ErrorResponse>::HAS_DEFAULT);
        assert!(<Result<String, Failure> as Responsible>::HAS_DEFAULT);
        assert!(<Result<String, Failure> as ErrorResponse>::STATUSES.is_empty());
    }

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
            const STATUSES: Statuses = Statuses::of(&[201]);
            const HAS_DEFAULT: bool = true;

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
    fn item_schemas_are_independent_of_whole_body_schemas() {
        use serde_json::json;
        let item_only = item_response::<String>(200, "text/event-stream", "events");
        let mut value = serde_json::to_value(&item_only).unwrap();
        let media = &value["200"]["content"]["text/event-stream"];
        assert!(media.get("schema").is_none());
        assert_eq!(media["itemSchema"], json!({"type": "string"}));

        let both = response_with_item::<String, i32>(200, "application/jsonl", "records");
        value = serde_json::to_value(&both).unwrap();
        let media = &value["200"]["content"]["application/jsonl"];
        assert_eq!(media["schema"], json!({"type": "string"}));
        assert_eq!(media["itemSchema"], json!({"type": "integer"}));
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
