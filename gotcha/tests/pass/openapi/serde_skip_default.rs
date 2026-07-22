//! serde `skip` / `default` / `skip_serializing_if` are reflected in the schema:
//! skipped fields disappear, and defaulted / conditionally-skipped fields are not
//! marked required.

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct Item {
    name: String,
    #[serde(skip)]
    internal: String,
    #[serde(default)]
    count: i32,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    tag: String,
}

fn main() {
    let schema = serde_json::to_value(Item::generate_schema().schema).unwrap();
    let props = &schema["properties"];

    // `skip` → the field is not part of the schema at all.
    assert!(props.get("internal").is_none(), "skipped field must be absent");
    assert!(props.get("name").is_some());
    assert!(props.get("count").is_some());
    assert!(props.get("tag").is_some());

    let required = schema["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "name"), "plain field is required");
    assert!(!required.iter().any(|v| v == "count"), "#[serde(default)] field must be optional");
    assert!(!required.iter().any(|v| v == "tag"), "skip_serializing_if field must be optional");
    assert!(!required.iter().any(|v| v == "internal"), "skipped field must not be required");
}
