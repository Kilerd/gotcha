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
async fn path_tuple(_tuple_path: Path<(i32,)>) -> impl Responder {
    "Hello world".to_string()
}
#[api]
async fn query_params(_struct_path: Query<QueryArgs>) -> impl Responder {}
#[api]
async fn json_payload(_payload: Json<RequestPayload>) -> impl Responder {}
#[api]
async fn state_extract(_data: State<AppData>) -> impl Responder {}


fn extract<H, T>(handler: H) -> Option<&'static gotcha::openapi::Operable>
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>()
}


fn extract_with_state<H, T>(handler: H) -> Option<&'static gotcha::openapi::Operable>
where
    H: gotcha::axum::handler::Handler<T, AppData>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, AppData>()
}
fn main() {
    let operable = extract(path_tuple).unwrap();
    let operation = operable.generate("/pets/:pet_id".to_string());
    assert!(operation.operation_id == Some("path_tuple".to_string()));
    assert!(operation.description == None);
    assert!(operation.deprecated == Some(false));
    assert!(operation.tags.is_none());


    let operable = extract(query_params).unwrap();
    let operation = operable.generate("/pets/:pet_id".to_string());


    let operable = extract(json_payload).unwrap();
    let operation = operable.generate("/pets/:pet_id".to_string());

    let operable = extract_with_state(state_extract).unwrap();
    let operation = operable.generate("/pets/:pet_id".to_string());
}