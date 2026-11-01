//! `#[api]` reads literal doc comments without parsing `#[doc = include_str!(..)]`, whose value
//! does not exist at macro-expansion time. The literal doc lines are still collected.

use gotcha::{api, openapi::Operable};

/// Load a user.
#[doc = include_str!("../../fixtures/doc_comment.md")]
#[api(id = "documented")]
async fn documented() -> String {
    String::new()
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
    let op = extract(documented).unwrap().generate("/documented".to_owned());
    assert_eq!(op.description.as_deref(), Some("Load a user."));
}
