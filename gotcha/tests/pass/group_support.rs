use gotcha::{api, openapi::Operable};

#[api(group="authentication")]
async fn handler() -> String {
    "Hello world".to_string()
}



fn extract<H, T>(handler: H) -> Option<&'static Operable> where  H: gotcha::axum::handler::Handler<T, ()>,
T: 'static, {
    use gotcha::extract_operable;
    extract_operable::<H,T, ()>()
}

fn main() {
    use gotcha::Operable;
    let operable = extract(handler).unwrap();
    let operation = operable.generate();
    assert!(operation.tags.unwrap().pop().unwrap().eq("authentication"));
}