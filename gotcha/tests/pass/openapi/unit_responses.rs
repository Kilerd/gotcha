//! A handler that returns nothing documents a response with no content — and, more basically,
//! compiles at all. `#[api]` on a handler with no return type used to fail with `E0782: expected a
//! type, found a trait`, and `()` used to document as `{"type": "void"}`, which is not a valid
//! OpenAPI type.
//!
//! The documented status is whatever axum actually sends, which is `200` with an empty body — not
//! `204`. The test pins that against the real `IntoResponse` impl so the document cannot drift
//! away from the runtime behaviour.

use gotcha::axum::response::IntoResponse;
use gotcha::{api, openapi::Operable, Json};

// The case that did not compile before.
#[api(id = "no_return")]
async fn no_return() {}

// The explicit spelling of the same thing.
#[api(id = "explicit_unit")]
async fn explicit_unit() -> () {}

// Wrapping the unit in `Json` is genuinely different: that sends `null` with a 200.
#[api(id = "json_unit")]
async fn json_unit() -> Json<()> {
    Json(())
}

fn extract<H, T>(_handler: H) -> &'static Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().expect("handler is registered")
}

fn main() {
    // What axum actually sends for `()` — the documented status has to match this.
    let actual_status = ().into_response().status();
    assert_eq!(actual_status.as_u16(), 200, "axum sends 200 with an empty body for the unit type");

    for operable in [extract(no_return), extract(explicit_unit)] {
        let responses = operable.generate("/x".to_owned()).responses;
        let rendered = serde_json::to_string(&responses).unwrap();

        let documented = actual_status.as_u16().to_string();
        assert!(responses.data.contains_key(&documented), "documents the status axum sends: {rendered}");

        let gotcha::oas::Referenceable::Data(response) = &responses.data[&documented] else {
            panic!("expected an inline response");
        };
        assert!(response.content.is_none(), "an empty body carries no content: {rendered}");
    }

    // `Json<()>` still documents a 200 body, and its schema is valid (no bogus `"void"` type).
    let json_responses = extract(json_unit).generate("/j".to_owned()).responses;
    let rendered = serde_json::to_string(&json_responses).unwrap();
    assert!(json_responses.data.contains_key("200"), "Json<()> keeps its 200: {rendered}");
    assert!(!rendered.contains("void"), "`void` is not a valid OpenAPI type: {rendered}");
}
