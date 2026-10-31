use assert_json_diff::assert_json_eq;
use gotcha::Schematic;
use std::collections::{BTreeMap, HashMap};

#[derive(Schematic)]
struct StructValue {
    name: String,
    age: u32,
}

#[derive(Schematic)]
struct BTreeMapField {
    values: BTreeMap<String, StructValue>,
}

#[derive(Schematic)]
struct HashMapField {
    values: HashMap<String, StructValue>,
}

fn main() {
    let schema = BTreeMap::<String, String>::generate_schema();
    let schema_json = serde_json::to_value(&schema.schema).unwrap();
    println!("schema_json: {:?}", schema_json);
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("btreemap.json")).unwrap();
    assert_json_eq!(schema_json, expected_json);

    // A struct field holding either map type documents identically.
    let btree = serde_json::to_value(BTreeMapField::generate_schema().schema).unwrap();
    let hash = serde_json::to_value(HashMapField::generate_schema().schema).unwrap();
    assert_json_eq!(btree, hash);
    assert_eq!(btree["properties"]["values"]["additionalProperties"]["properties"]["name"]["type"], "string");
}
