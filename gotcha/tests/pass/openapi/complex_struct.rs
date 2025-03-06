use gotcha::Schematic;
use serde::Serialize;
use assert_json_diff::assert_json_eq;


#[derive(Schematic)]
pub struct Pagination {
    page: usize,
    size: usize,
    option_string: Option<String>,
    data: Option<Vec<u8>>,

}

fn main() {
    let schema = Pagination::generate_schema();
    let schema_json = serde_json::to_value(&schema.schema).unwrap();
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("complex_struct.json")).unwrap();
    dbg!(&schema_json);
    assert_json_eq!(schema_json,expected_json);
}