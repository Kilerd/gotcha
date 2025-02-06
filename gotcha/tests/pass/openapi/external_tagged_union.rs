use gotcha::Schematic;
use serde::Serialize;
use assert_json_diff::assert_json_eq;

#[derive(Schematic)]
pub enum Union {
    One{inner: String},
    Two{other: i32},
}



fn main() {
    let schema = Union::generate_schema();

    let schema_json = serde_json::to_value(&schema.schema).unwrap();
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("external_tagged_union.json")).unwrap();
    assert_json_eq!(schema_json,expected_json);
}