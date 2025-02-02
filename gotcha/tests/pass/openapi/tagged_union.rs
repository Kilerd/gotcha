use gotcha::Schematic;
use serde::Serialize;
use assert_json_diff::assert_json_eq;


#[derive(Schematic, Serialize)]
#[serde(tag = "type")]
pub enum TaggedUnion {
    One{inner: String},
    Two{ohter: i32},
}

fn main() {
    let schema = TaggedUnion::generate_schema();

    let schema_json = serde_json::to_value(&schema).unwrap();
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("tagged_union.json")).unwrap();
    assert_json_eq!(schema_json, expected_json);
}