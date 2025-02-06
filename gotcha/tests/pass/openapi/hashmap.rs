use gotcha::Schematic;
use std::collections::HashMap;
use assert_json_diff::assert_json_eq;

fn main() {
    let schema = HashMap::<String, String>::generate_schema();
    let schema_json = serde_json::to_value(&schema.schema).unwrap();
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("hashmap.json")).unwrap();
    assert_json_eq!(schema_json,expected_json);
}
