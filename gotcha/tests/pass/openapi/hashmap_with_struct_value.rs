use gotcha::Schematic;
use std::collections::HashMap;
use assert_json_diff::assert_json_eq;

#[derive(Schematic)]
struct StructValue {
    name: String,
    age: u32,
}

fn main() {
    let schema = HashMap::<String, StructValue>::generate_schema();
    let schema_json = serde_json::to_value(&schema.schema).unwrap();
    println!("schema_json: {:?}", schema_json);
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("hashmap_with_struct_value.json")).unwrap();
    assert_json_eq!(schema_json,expected_json);
}
