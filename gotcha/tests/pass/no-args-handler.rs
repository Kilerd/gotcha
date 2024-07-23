use gotcha::{api, Responder};

#[api]
async fn handler() -> String {
    "Hello world".to_string()
}
fn extract<H, T>(handler: H) -> Option<&'static gotcha::openapi::Operable>
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::extract_operable::<H, T, ()>()
}
fn main() {
    let operable = extract(handler).unwrap();

    let operation = operable.generate("/".to_owned());
    assert!(operation.operation_id == Some("handler".to_string()));
    assert!(operation.description == None);
    assert!(operation.deprecated == Some(false));
    assert!(operation.tags.is_none());
}