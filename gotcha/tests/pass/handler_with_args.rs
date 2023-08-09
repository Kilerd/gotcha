use gotcha::{ post, Schematic,web::{Json,Path, Query, Data}, Responder};
use serde::Deserialize;
#[derive(Clone)]
pub struct AppData {

}

#[derive(Clone, Deserialize, Schematic)]
pub struct PathArgs {
    id: String
}

#[derive(Clone, Deserialize, Schematic)]
pub struct QueryArgs {
    page: i32
}
#[derive(Clone, Deserialize, Schematic)]
pub struct RequestPayload {
    name: String,
    id: i32
}


#[post("/resources/{id}")]
async fn post_handler(_tuple_path: Path<(i32, )>, _struct_path: Query<QueryArgs>, _payload:Json<RequestPayload>, _data:Data<AppData>) -> impl Responder {
    "Hello world".to_string()
}

fn main() {
    use gotcha::Operable;
    let operation = post_handler.generate();
    assert!(operation.operation_id == Some("post_handler".to_string()));
    assert!(operation.description == None);
    assert!(operation.deprecated == Some(false));
    assert!(operation.tags.is_none());
}