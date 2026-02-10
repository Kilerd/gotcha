use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "kebab-case")]
pub enum ApiResponse {
    Success { data: String },
    Error { message: String, code: i32 },
    Loading,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "snake_case")]
pub enum Result {
    Ok { value: String },
    Err { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub enum WithMixedRename {
    #[serde(rename = "custom_success")]
    Success { data: String },
    NormalFailure { reason: String },
}

fn main() {
    // Test kebab-case external tagged enum
    let schema = ApiResponse::generate_schema();
    let one_of = schema.schema.extras.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 3);

    // Check first variant uses kebab-case key
    let first_variant = &one_of[0];
    let props = first_variant.get("properties").unwrap().as_object().unwrap();
    // External tagged enum uses variant name as property key
    assert!(props.contains_key("success"));

    let second_variant = &one_of[1];
    let props = second_variant.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("error"));

    // Test snake_case external tagged enum
    let schema = Result::generate_schema();
    let one_of = schema.schema.extras.get("oneOf").unwrap().as_array().unwrap();
    let first_variant = &one_of[0];
    let props = first_variant.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("ok"));

    // Test individual variant rename in external tagged
    let schema = WithMixedRename::generate_schema();
    let one_of = schema.schema.extras.get("oneOf").unwrap().as_array().unwrap();
    let first_variant = &one_of[0];
    let props = first_variant.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("custom_success"));

    println!("All serde rename tests for external tagged enum passed!");
}
