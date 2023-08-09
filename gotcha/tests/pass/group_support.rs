use gotcha::{get, Operable,Schematic};
use oas::{Parameter, Schema};

#[get("/hello-world", group="authentication")]
async fn handler() -> String {
    "Hello world".to_string()
}


fn main() {
    let operation = handler.generate();
    assert!(operation.tags.unwrap().pop().unwrap().eq("authentication"));
}