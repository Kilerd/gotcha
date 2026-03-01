//! Comprehensive tests for untagged enum support
#![cfg(feature = "openapi")]

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

// === Basic untagged enum ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
pub enum BasicUntagged {
    Text(String),
    Number(i32),
    Boolean(bool),
}

#[test]
fn test_basic_untagged_multiple_types() {
    let schema = BasicUntagged::generate_schema();
    let json = schema.schema.to_value();

    assert!(json.get("oneOf").is_some());
    assert!(json.get("discriminator").is_none());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 3);

    // Check each variant type
    let types: Vec<&str> = one_of
        .iter()
        .filter_map(|v| v.get("type").and_then(|t| t.as_str()))
        .collect();
    assert!(types.contains(&"string"));
    assert!(types.contains(&"integer"));
    assert!(types.contains(&"boolean"));
}

// === Untagged with named variants ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
pub enum UntaggedNamed {
    User { name: String, age: u32 },
    Product { title: String, price: f64 },
}

#[test]
fn test_untagged_named_variants() {
    let schema = UntaggedNamed::generate_schema();
    let json = schema.schema.to_value();

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 2);

    // First variant should have name and age
    let user = &one_of[0];
    let user_props = user.get("properties").unwrap().as_object().unwrap();
    assert!(user_props.contains_key("name"));
    assert!(user_props.contains_key("age"));

    // Second variant should have title and price
    let product = &one_of[1];
    let product_props = product.get("properties").unwrap().as_object().unwrap();
    assert!(product_props.contains_key("title"));
    assert!(product_props.contains_key("price"));
}

// === Untagged with mixed variants ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
pub enum UntaggedMixed {
    Simple(String),
    Complex { id: i32, data: String },
}

#[test]
fn test_untagged_mixed_variants() {
    let schema = UntaggedMixed::generate_schema();
    let json = schema.schema.to_value();
    println!("Mixed untagged: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 2);

    // Simple variant should be just a string
    let simple = &one_of[0];
    assert_eq!(simple.get("type").unwrap().as_str().unwrap(), "string");

    // Complex variant should be an object
    let complex = &one_of[1];
    assert_eq!(complex.get("type").unwrap().as_str().unwrap(), "object");
}

// === Untagged with rename_all on variant fields ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged, rename_all = "camelCase")]
pub enum UntaggedWithRename {
    Request { user_name: String, request_id: i32 },
    Response { status_code: i32, response_body: String },
}

#[test]
fn test_untagged_with_rename_all() {
    let schema = UntaggedWithRename::generate_schema();
    let json = schema.schema.to_value();
    println!("Untagged with rename: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();

    // Check camelCase field names
    let request = &one_of[0];
    let req_props = request.get("properties").unwrap().as_object().unwrap();
    assert!(req_props.contains_key("userName"), "should have camelCase userName");
    assert!(req_props.contains_key("requestId"), "should have camelCase requestId");

    let response = &one_of[1];
    let resp_props = response.get("properties").unwrap().as_object().unwrap();
    assert!(resp_props.contains_key("statusCode"));
    assert!(resp_props.contains_key("responseBody"));
}

// === Untagged with nested struct ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Address {
    street: String,
    city: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
pub enum Location {
    Simple(String),
    Detailed(Address),
}

#[test]
fn test_untagged_with_nested_struct() {
    let schema = Location::generate_schema();
    let json = schema.schema.to_value();
    println!("Untagged with nested: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 2);

    // Detailed variant should have Address properties
    let detailed = &one_of[1];
    let props = detailed.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("street"));
    assert!(props.contains_key("city"));
}

// === Untagged with Vec ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
pub enum ArrayOrSingle {
    Single(String),
    Multiple(Vec<String>),
}

#[test]
fn test_untagged_with_vec() {
    let schema = ArrayOrSingle::generate_schema();
    let json = schema.schema.to_value();
    println!("Untagged with vec: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();

    // One should be string, one should be array
    let types: Vec<&str> = one_of
        .iter()
        .filter_map(|v| v.get("type").and_then(|t| t.as_str()))
        .collect();
    assert!(types.contains(&"string"));
    assert!(types.contains(&"array"));
}

// === Untagged with Option fields ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
pub enum MaybeDetailed {
    Minimal { id: i32 },
    Full { id: i32, name: String, description: Option<String> },
}

#[test]
fn test_untagged_with_optional_fields() {
    let schema = MaybeDetailed::generate_schema();
    let json = schema.schema.to_value();
    println!("Untagged with optional: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    let full = &one_of[1];

    // Check required fields
    let required = full.get("required").unwrap().as_array().unwrap();
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();

    assert!(required_names.contains(&"id"));
    assert!(required_names.contains(&"name"));
    assert!(!required_names.contains(&"description"), "Optional field should not be required");
}

// === Flatten untagged enum in struct ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Container {
    id: String,
    #[serde(flatten)]
    content: UntaggedMixed,
}

#[test]
fn test_flatten_untagged_in_struct() {
    let schema = Container::generate_schema();
    let json = schema.schema.to_value();
    println!("Flatten untagged: {}", serde_json::to_string_pretty(&json).unwrap());

    // Should use allOf
    let all_of = json.get("allOf").unwrap().as_array().unwrap();
    assert_eq!(all_of.len(), 2);

    // Base should have id
    let base = &all_of[0];
    let props = base.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("id"));

    // Second should be oneOf
    let enum_schema = &all_of[1];
    assert!(enum_schema.get("oneOf").is_some());
}
