//! Externally-tagged newtype variants wrapping a SCALAR: `V(T)` serializes as
//! `{"V": <T>}`, so the content must be `T`'s schema. Previously the content was
//! reconstructed from `T::fields()`, which is empty for scalars, yielding a bogus
//! `{"type":"object","properties":{}}`.

use gotcha::Schematic;

#[derive(Schematic)]
enum Scalar {
    Int(i32),
    Text(String),
}

fn main() {
    let schema = serde_json::to_value(Scalar::generate_schema().schema).unwrap();
    let one_of = schema["oneOf"].as_array().expect("oneOf array");

    let int_branch = one_of.iter().find(|b| b["title"] == "Int").expect("Int branch");
    assert_eq!(int_branch["properties"]["Int"]["type"], "integer");

    let text_branch = one_of.iter().find(|b| b["title"] == "Text").expect("Text branch");
    assert_eq!(text_branch["properties"]["Text"]["type"], "string");
}
