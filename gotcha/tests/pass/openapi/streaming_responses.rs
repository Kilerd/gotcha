//! `skip_response` makes an explicit `responses(...)` list the complete contract, and the macro
//! never places the return type in a trait path. WebSocket handlers and SSE handlers returning
//! opaque types can therefore be documented: a `101` upgrade for the socket, and a `200
//! text/event-stream` response with an OpenAPI 3.2 `itemSchema` for the stream.

use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use gotcha::axum::http::Method;
use gotcha::axum::response::{IntoResponse, Response};
use gotcha::{api, openapi::generate_openapi, Event, Json, Schematic, Sse, WebSocketUpgrade};
use serde::Serialize;

#[derive(Schematic, Serialize)]
struct User {
    id: u32,
}

#[derive(Schematic, Serialize)]
struct Tick {
    seq: u64,
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

/// `impl Stream` cannot appear in a trait path; the declared item schema describes each event.
#[api(
    skip_response,
    responses(response(
        status = 200,
        content_type = "text/event-stream",
        item = "Tick",
        description = "Server-sent events"
    ))
)]
async fn events() -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    Sse::new(NoEvents)
}

/// An explicit `201` replaces the inferred `200` entirely, including for a normal JSON return.
#[api(skip_response, responses(response(status = 201, body = "User")))]
async fn create() -> Json<User> {
    Json(User { id: 7 })
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
        (("/socket".to_string(), Method::GET), operable(socket)),
        (("/users".to_string(), Method::POST), operable(create)),
    ]);
    let spec = serde_json::to_value(generate_openapi(operables)).unwrap();

    let socket = &spec["paths"]["/socket"]["get"];
    assert_eq!(statuses(&socket["responses"]), ["101"]);
    assert!(socket["responses"]["default"].is_null());
    assert_eq!(socket["parameters"].as_array().unwrap().len(), 0, "WebSocketUpgrade documents no parameter");
    assert!(socket["responses"]["101"]["content"].is_null(), "the upgrade carries no body");

    let events = &spec["paths"]["/events"]["get"];
    assert_eq!(statuses(&events["responses"]), ["200"]);
    let media = &events["responses"]["200"]["content"]["text/event-stream"];
    assert_eq!(media["itemSchema"]["$ref"], "#/components/schemas/Tick");
    assert!(media.get("schema").is_none(), "item alone does not claim a whole-body schema");
    assert_eq!(events["responses"]["200"]["description"], "Server-sent events");
    assert_eq!(spec["components"]["schemas"]["Tick"]["properties"]["seq"]["type"], "integer");

    let create = &spec["paths"]["/users"]["post"];
    assert_eq!(statuses(&create["responses"]), ["201"], "only the declared status is documented");
    assert!(create["responses"]["200"].is_null(), "the inferred 200 must be gone");
    assert_eq!(
        create["responses"]["201"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/User"
    );
}
