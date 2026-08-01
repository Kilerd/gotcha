//! Typed header and cookie parameters are documented as OpenAPI `header` / `cookie` parameters,
//! and axum's untyped `HeaderMap` is accepted by `#[api]` handlers (it documents nothing).

use gotcha::axum::http::HeaderMap;
use gotcha::{api, headers, openapi::Operable, Cookie, CookieParam, Header, HeaderParam, Schematic, TypedHeader};
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct RequestId(String);

impl HeaderParam for RequestId {
    const NAME: &'static str = "x-request-id";
    const DESCRIPTION: Option<&'static str> = Some("Correlation id propagated across services");
    fn parse(raw: &str) -> Result<Self, String> {
        Ok(RequestId(raw.to_string()))
    }
}

#[derive(Schematic, Serialize, Deserialize)]
struct Session(String);

impl CookieParam for Session {
    const NAME: &'static str = "session";
    const REQUIRED: bool = false;
    fn parse(raw: &str) -> Result<Self, String> {
        Ok(Session(raw.to_string()))
    }
    fn missing() -> Option<Self> {
        Some(Session(String::new()))
    }
}

#[api(id = "whoami")]
async fn whoami(Header(id): Header<RequestId>, Cookie(session): Cookie<Session>) -> String {
    format!("{} {}", id.0, session.0)
}

/// The untyped map is allowed, it just contributes no documented parameters.
#[api(id = "raw_headers")]
async fn raw_headers(_headers: HeaderMap) -> String {
    "ok".to_string()
}

/// axum's own typed headers need no extra declaration — `headers::Header` carries the name —
/// and wrapping one in `Option` makes it an optional parameter.
#[api(id = "typed_headers")]
async fn typed_headers(
    TypedHeader(agent): TypedHeader<headers::UserAgent>, host: Option<TypedHeader<headers::Host>>,
) -> String {
    format!("{} {:?}", agent.as_str(), host.is_some())
}

fn extract<H, T>(_handler: H) -> &'static Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().expect("handler is registered")
}

fn main() {
    let operation = extract(whoami).generate("/whoami".to_owned());
    let params = operation.parameters.expect("handler has parameters");

    let named = |name: &str| {
        params
            .iter()
            .find_map(|p| match p {
                gotcha::oas::Referenceable::Data(p) if p.name == name => Some(p.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("`{name}` is documented: {params:?}"))
    };

    // The header parameter lands in the right place, is required, and carries its description.
    let header = named("x-request-id");
    assert!(matches!(header._in, gotcha::oas::ParameterIn::Header));
    assert_eq!(header.required, Some(true));
    assert_eq!(header.description.as_deref(), Some("Correlation id propagated across services"));
    // A newtype wrapper is transparent, so the schema is the inner type's.
    let schema = serde_json::to_value(header.schema.unwrap()).unwrap();
    assert_eq!(schema["type"], "string");

    // The cookie parameter is separate from headers, and `REQUIRED = false` is reflected.
    let cookie = named("session");
    assert!(matches!(cookie._in, gotcha::oas::ParameterIn::Cookie));
    assert_eq!(cookie.required, Some(false));

    // `HeaderMap` compiles as a handler argument and documents nothing.
    let raw = extract(raw_headers).generate("/raw".to_owned());
    assert!(raw.parameters.unwrap_or_default().is_empty(), "the untyped map documents no parameters");

    // axum's typed headers name themselves; `Option<..>` marks the parameter optional.
    let typed = extract(typed_headers).generate("/typed".to_owned());
    let typed_params: Vec<_> = typed
        .parameters
        .expect("typed headers are documented")
        .into_iter()
        .filter_map(|p| match p {
            gotcha::oas::Referenceable::Data(p) => Some(p),
            _ => None,
        })
        .collect();

    let agent = typed_params.iter().find(|p| p.name == "user-agent").expect("user-agent documented");
    assert!(matches!(agent._in, gotcha::oas::ParameterIn::Header));
    assert_eq!(agent.required, Some(true));

    let host = typed_params.iter().find(|p| p.name == "host").expect("host documented");
    assert_eq!(host.required, Some(false), "Option<TypedHeader<..>> is not required");
}
