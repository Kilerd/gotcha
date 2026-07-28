//! `#[api(...)]` operation metadata: an explicit `summary`, the `deprecated` flag, and a
//! per-operation `security` scheme all land in the generated OpenAPI operation; when unset,
//! summary falls back to the id in Title Case and there is no security requirement.

use gotcha::{api, openapi::Operable};

#[api(id = "create_user", summary = "Create a new user", deprecated, security = "bearerAuth")]
async fn create_user() -> String {
    "ok".to_string()
}

#[api(id = "list_users")]
async fn list_users() -> String {
    "ok".to_string()
}

fn extract<H, T>(_handler: H) -> Option<&'static Operable>
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    use gotcha::router::extract_operable;
    extract_operable::<H, T, ()>()
}

fn main() {
    // Explicit metadata is carried through.
    let op = extract(create_user).unwrap().generate("/users".to_owned());
    assert_eq!(op.summary.as_deref(), Some("Create a new user"), "explicit summary is used");
    assert_eq!(op.deprecated, Some(true), "deprecated flag is honored");
    let security = op.security.expect("security requirement present");
    assert_eq!(security.len(), 1);
    assert!(security[0].data.contains_key("bearerAuth"), "the named scheme is required");

    // Defaults when the attributes are absent.
    let op2 = extract(list_users).unwrap().generate("/users".to_owned());
    assert_eq!(op2.summary.as_deref(), Some("List Users"), "summary defaults to the id in Title Case");
    assert_eq!(op2.deprecated, Some(false), "not deprecated by default");
    assert!(op2.security.is_none(), "no security requirement by default");
}
