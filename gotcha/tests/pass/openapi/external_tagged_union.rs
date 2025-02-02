use gotcha::Schematic;
use serde::Serialize;
use assert_json_diff::assert_json_include;

#[derive(Schematic)]
pub enum Union {
    One{inner: String},
    Two{ohter: i32},
}



fn main() {
    let schema = Union::generate_schema();

    let schema_json = serde_json::to_value(&schema).unwrap();
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("external_tagged_union.json")).unwrap();
    assert_json_include!(actual: schema_json, expected: expected_json);
}