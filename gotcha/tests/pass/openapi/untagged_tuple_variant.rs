//! Untagged multi-field tuple variants `V(A, B)` serialize as `[a, b]`, so the
//! schema must be an array that preserves every element. Previously the codegen
//! overwrote a single slot per field, keeping only the last element's schema.

use gotcha::Schematic;
use serde::Serialize;

#[derive(Schematic, Serialize)]
#[serde(untagged)]
enum Untagged {
    Pair(i32, String),
    Flag(bool),
}

fn main() {
    let schema = serde_json::to_value(Untagged::generate_schema().schema).unwrap();
    let one_of = schema["oneOf"].as_array().expect("oneOf array");

    // The two-field tuple variant becomes an array preserving BOTH element schemas.
    let pair = one_of.iter().find(|b| b["type"] == "array").expect("array branch");
    let prefix = pair["prefixItems"].as_array().expect("prefixItems");
    assert_eq!(prefix.len(), 2, "both tuple elements must be present");
    assert_eq!(prefix[0]["type"], "integer");
    assert_eq!(prefix[1]["type"], "string");
    assert_eq!(pair["minItems"], 2);
    assert_eq!(pair["maxItems"], 2);

    // The newtype variant stays transparent to its inner type.
    assert!(one_of.iter().any(|b| b["type"] == "boolean"), "boolean branch present");
}
