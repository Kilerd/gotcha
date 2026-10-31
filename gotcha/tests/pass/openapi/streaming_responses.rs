//! `skip_response` makes an explicit `responses(...)` list the complete contract, and the macro
//! never places the return type in a trait path. WebSocket handlers and SSE handlers returning
//! opaque types can therefore be documented: a `101` upgrade for the socket, and a `200
//! text/event-stream` response whose OpenAPI 3.2 `itemSchema` describes the parsed event.
//!
//! For `text/event-stream`, `itemSchema` describes the event envelope (`data`, plus optional
//! `event`, `id`, and `retry`), not the JSON some servers put inside `data`. A hand-written
//! `Schematic` can attach `contentMediaType` and `contentSchema` to that JSON field.

use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use gotcha::axum::http::Method;
use gotcha::axum::response::{IntoResponse, Response};
use gotcha::gotcha_core::{registry::schema_or_ref, EnhancedSchema};
use gotcha::oas::Schema;
use gotcha::{api, openapi::generate_openapi, Event, Json, Schematic, Sse, WebSocketUpgrade};
use serde::Serialize;

#[derive(Schematic, Serialize)]
struct User {
    id: u32,
}

/// The JSON payload carried inside an SSE event's `data` field.
#[derive(Schematic, Serialize)]
struct Tick {
    seq: u64,
}

/// A parsed server-sent event: the JSON in `data` is not itself the event.
#[derive(Schematic, Serialize)]
struct ChatSseEvent {
    data: String,
    event: Option<String>,
    id: Option<String>,
    retry: Option<u32>,
}

/// The same envelope with a schema for the JSON encoded in `data`. Hand-written because
/// `#[derive(Schematic)]` has no syntax for `contentMediaType` / `contentSchema`.
struct TickEvent;

impl Schematic for TickEvent {
    fn name() -> &'static str {
        "TickEvent"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn generate_schema() -> EnhancedSchema {
        schema_or_ref("TickEvent", true, || {
            let payload = <Tick as Schematic>::generate_schema().schema;
            EnhancedSchema {
                required: true,
                schema: Schema {
                    _type: Some("object".into()),
                    description: Some("A parsed event whose data is JSON".into()),
                    extras: BTreeMap::from([(
                        "properties".into(),
                        serde_json::json!({
                            "event": {"type": "string"},
                            "id": {"type": "string"},
                            "retry": {"type": "integer"},
                            "data": {
                                "type": "string",
                                "contentMediaType": "application/json",
                                "contentSchema": payload,
                            },
                        }),
                    )]),
                    ..Schema::default()
                },
            }
        })
    }
}

/// A stream that ends immediately; only the declaration is under test.
struct NoEvents;

impl futures_core::Stream for NoEvents {
    type Item = Result<Event, Infallible>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

/// A plain WebSocket handler: `WebSocketUpgrade` contributes no parameter, and the return type
/// is not consulted.
#[api(
    skip_response,
    responses(response(status = 101, description = "Switching protocols"))
)]
async fn socket(_upgrade: WebSocketUpgrade) -> Response {
    gotcha::axum::http::StatusCode::SWITCHING_PROTOCOLS.into_response()
}

/// `impl Stream` cannot appear in a trait path; the declared item schema describes each parsed
/// event envelope.
#[api(
    skip_response,
    responses(response(
        status = 200,
        content_type = "text/event-stream",
        item = "ChatSseEvent",
        description = "Server-sent events"
    ))
)]
async fn events() -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    Sse::new(NoEvents)
}

/// A hand-written item schema can describe the JSON inside `data` without reinterpreting it as
/// the event itself.
#[api(
    skip_response,
    responses(response(
        status = 200,
        content_type = "text/event-stream",
        item = "TickEvent"
    ))
)]
async fn json_events() -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    Sse::new(NoEvents)
}

/// An explicit `201` replaces the inferred `200` entirely, including for a normal JSON return.
#[api(skip_response, responses(response(status = 201, body = "User")))]
async fn create() -> Json<User> {
    Json(User { id: 7 })
}

/// `body` and `item` are independent fields; declaring both emits both schemas.
#[api(
    skip_response,
    responses(response(status = 200, content_type = "application/jsonl", body = "User", item = "User"))
)]
async fn records() -> Response {
    gotcha::axum::http::StatusCode::OK.into_response()
}

fn operable<H, T>(_handler: H) -> &'static gotcha::openapi::Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().expect("handler is registered")
}

fn statuses(responses: &serde_json::Value) -> Vec<&str> {
    responses.as_object().expect("responses object").keys().map(String::as_str).collect()
}

fn main() {
    let operables = HashMap::from([
        (("/events".to_string(), Method::GET), operable(events)),
        (("/json-events".to_string(), Method::GET), operable(json_events)),
        (("/records".to_string(), Method::POST), operable(records)),
        (("/socket".to_string(), Method::GET), operable(socket)),
        (("/users".to_string(), Method::POST), operable(create)),
    ]);
    let spec = serde_json::to_value(generate_openapi(operables)).unwrap();

    let socket = &spec["paths"]["/socket"]["get"];
    assert_eq!(statuses(&socket["responses"]), ["101"]);
    assert!(socket["responses"]["default"].is_null());
    assert_eq!(socket["parameters"].as_array().unwrap().len(), 0, "WebSocketUpgrade documents no parameter");
    assert!(socket["responses"]["101"]["content"].is_null(), "the upgrade carries no body");

    // The item schema describes the parsed event, not the JSON in its `data`.
    let events = &spec["paths"]["/events"]["get"];
    assert_eq!(statuses(&events["responses"]), ["200"]);
    let media = &events["responses"]["200"]["content"]["text/event-stream"];
    assert_eq!(media["itemSchema"]["$ref"], "#/components/schemas/ChatSseEvent");
    assert!(media.get("schema").is_none(), "item alone does not claim a whole-body schema");
    assert_eq!(events["responses"]["200"]["description"], "Server-sent events");
    let envelope = &spec["components"]["schemas"]["ChatSseEvent"]["properties"];
    assert_eq!(envelope["data"]["type"], "string");
    for field in ["event", "id"] {
        assert_eq!(envelope[field]["anyOf"][0]["type"], "string", "{field} is an optional string");
        assert_eq!(envelope[field]["anyOf"][1]["type"], "null");
    }
    assert_eq!(envelope["retry"]["anyOf"][0]["type"], "integer");
    assert_eq!(envelope["retry"]["anyOf"][1]["type"], "null");

    // JSON inside `data` keeps its own schema via contentMediaType/contentSchema.
    let json_events = &spec["paths"]["/json-events"]["get"];
    let media = &json_events["responses"]["200"]["content"]["text/event-stream"];
    assert_eq!(media["itemSchema"]["$ref"], "#/components/schemas/TickEvent");
    let data = &spec["components"]["schemas"]["TickEvent"]["properties"]["data"];
    assert_eq!(data["type"], "string", "the wire value is still a string");
    assert_eq!(data["contentMediaType"], "application/json");
    assert_eq!(data["contentSchema"]["$ref"], "#/components/schemas/Tick");
    assert_eq!(spec["components"]["schemas"]["Tick"]["properties"]["seq"]["type"], "integer");

    // Body and item are independent: both appear under the same media type.
    let records = &spec["paths"]["/records"]["post"];
    assert_eq!(statuses(&records["responses"]), ["200"]);
    let media = &records["responses"]["200"]["content"]["application/jsonl"];
    assert_eq!(media["schema"]["$ref"], "#/components/schemas/User");
    assert_eq!(media["itemSchema"]["$ref"], "#/components/schemas/User");

    let create = &spec["paths"]["/users"]["post"];
    assert_eq!(statuses(&create["responses"]), ["201"], "only the declared status is documented");
    assert!(create["responses"]["200"].is_null(), "the inferred 200 must be gone");
    assert_eq!(
        create["responses"]["201"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/User"
    );
}
