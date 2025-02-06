use gotcha::Schematic;
use serde::Serialize;
use assert_json_diff::assert_json_eq;


#[derive(Schematic, Serialize)]
pub struct OneInner {
    pub inner: String,
}
#[derive(Schematic, Serialize)]
pub struct TwoInner {
    pub other: i32,
}

#[derive(Schematic, Serialize)]
pub enum Union {
    One(OneInner),
    Two(TwoInner),
}


fn main() {
    let schema = Union::generate_schema();

    let schema_json = serde_json::to_value(&schema.schema).unwrap();
    let expected_json: serde_json::Value = serde_json::from_str(include_str!("unamed_external_tagged_union.json")).unwrap();
    assert_json_eq!(schema_json,expected_json);
}