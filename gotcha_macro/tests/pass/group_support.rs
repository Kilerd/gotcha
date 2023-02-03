use gotcha_core::Operable;
use gotcha_macro::get;

#[get("/hello-world", group="authentication")]
async fn handler() -> String {
    "Hello world".to_string()
}


fn main() {
    let operation = handler.generate();
    assert!(operation.tags.unwrap().pop().unwrap().eq("authentication"));
}