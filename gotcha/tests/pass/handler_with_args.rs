use gotcha::{api, Schematic, Json, Path, Query, State, Responder};
use serde::Deserialize;
#[derive(Clone)]
pub struct AppData {}

#[derive(Clone, Deserialize, Schematic)]
pub struct PathArgs {
    id: String,
}

#[derive(Clone, Deserialize, Schematic)]
pub struct QueryArgs {
    page: i32,
}
#[derive(Clone, Deserialize, Schematic)]
pub struct RequestPayload {
    name: String,
    id: i32,
}


#[api]
async fn post_handler(_tuple_path: Path<(i32,)>, _struct_path: Query<QueryArgs>, _payload: Json<RequestPayload>, _data: State<AppData>) -> impl Responder {
    "Hello world".to_string()
}


fn extract<H, T>(handler: H) -> Option<&'static gotcha::openapi::Operable>
where
    H: gotcha::axum::handler::Handler<T, AppData>,
    T: 'static,
{
    gotcha::extract_operable::<H, T, AppData>()
}

fn main() {
    let operable = extract(post_handler).unwrap();
    let operation = operable.generate();
    assert!(operation.operation_id == Some("post_handler".to_string()));
    assert!(operation.description == None);
    assert!(operation.deprecated == Some(false));
    assert!(operation.tags.is_none());
}