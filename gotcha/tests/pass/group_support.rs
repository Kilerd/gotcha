use gotcha::{get,Schematic};
use oas::{Parameter, Schema};

#[get("/hello-world", group="authentication")]
async fn handler() -> String {
    "Hello world".to_string()
}


fn main() {
    use gotcha::Operable;
    let operation = handler.generate();
    assert!(operation.tags.unwrap().pop().unwrap().eq("authentication"));
}