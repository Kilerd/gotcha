use gotcha::Schematic;
use assert_json_diff::assert_json_eq;

#[derive(Schematic, Debug)]
pub struct ResponseWrapper<T: Schematic> {
    pub code: i32,
    pub message: String,
    pub data: T,
}

#[derive(Schematic,  Debug)]
pub struct Pet {
    pub id: i32,
    pub name: String,
}


fn main() {
    let schema = ResponseWrapper::<Pet>::generate_schema();
    let schema_json = serde_json::to_value(&schema).unwrap();
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("generic_struct.json")).unwrap();
    assert_json_eq!(schema_json,expected_json);
}
