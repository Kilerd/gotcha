//! Tests for flatten with enum types
#![cfg(feature = "openapi")]

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

// === Internally Tagged Enum ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type")]
pub enum Status {
    Active { since: String },
    Inactive { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct RequestWithTaggedEnum {
    name: String,
    #[serde(flatten)]
    status: Status,
}

#[test]
fn test_flatten_tagged_enum_uses_allof() {
    let schema = RequestWithTaggedEnum::generate_schema();
    let json = schema.schema.to_value();
    println!("Schema: {}", serde_json::to_string_pretty(&json).unwrap());

    // Should have allOf at top level
    assert!(json.get("allOf").is_some(), "should have allOf for flattened enum");

    let all_of = json.get("allOf").unwrap().as_array().unwrap();
    assert_eq!(all_of.len(), 2, "allOf should have 2 elements");

    // First element: own properties
    let base = &all_of[0];
    let props = base.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("name"), "base should have name property");

    // Second element: oneOf from enum
    let enum_schema = &all_of[1];
    assert!(enum_schema.get("oneOf").is_some(), "second element should be oneOf");
}

// === Untagged Enum ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
pub enum Event {
    Message { text: String },
    Number(i32),
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct EventWrapper {
    id: String,
    #[serde(flatten)]
    event: Event,
}

#[test]
fn test_untagged_enum_schema() {
    let schema = Event::generate_schema();
    let json = schema.schema.to_value();
    println!("Untagged enum schema: {}", serde_json::to_string_pretty(&json).unwrap());

    // Should have oneOf without discriminator
    assert!(json.get("oneOf").is_some(), "untagged enum should have oneOf");
    assert!(json.get("discriminator").is_none(), "untagged enum should NOT have discriminator");

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 2);
}

#[test]
fn test_flatten_untagged_enum() {
    let schema = EventWrapper::generate_schema();
    let json = schema.schema.to_value();
    println!("Flatten untagged enum: {}", serde_json::to_string_pretty(&json).unwrap());

    // Should have allOf
    assert!(json.get("allOf").is_some());

    let all_of = json.get("allOf").unwrap().as_array().unwrap();

    // Base properties
    let base = &all_of[0];
    let props = base.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("id"));

    // Untagged enum schema
    let enum_schema = &all_of[1];
    assert!(enum_schema.get("oneOf").is_some());
    assert!(enum_schema.get("discriminator").is_none());
}

// === External Tagged Enum (default) ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub enum Shape {
    Circle { radius: f32 },
    Rectangle { width: f32, height: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Drawing {
    title: String,
    #[serde(flatten)]
    shape: Shape,
}

#[test]
fn test_flatten_external_tagged_enum() {
    let schema = Drawing::generate_schema();
    let json = schema.schema.to_value();
    println!("Flatten external tagged: {}", serde_json::to_string_pretty(&json).unwrap());

    assert!(json.get("allOf").is_some());
}

// === Custom Tag Name ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "kind")]
pub enum Animal {
    Dog { name: String },
    Cat { name: String },
}

#[test]
fn test_custom_tag_name() {
    let schema = Animal::generate_schema();
    let json = schema.schema.to_value();
    println!("Custom tag: {}", serde_json::to_string_pretty(&json).unwrap());

    let discriminator = json.get("discriminator").unwrap();
    let prop_name = discriminator.get("propertyName").unwrap().as_str().unwrap();
    assert_eq!(prop_name, "kind", "discriminator should use custom tag name");

    // Check variant has "kind" property
    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    let first_variant = &one_of[0];
    let props = first_variant.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("kind"), "variant should have 'kind' property");
}

// === Multiple Flatten Enums ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "status_type")]
pub enum StatusEnum {
    Online,
    Offline { since: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "role_type")]
pub enum RoleEnum {
    Admin,
    User { permissions: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct UserProfile {
    id: String,
    #[serde(flatten)]
    status: StatusEnum,
    #[serde(flatten)]
    role: RoleEnum,
}

#[test]
fn test_multiple_flatten_enums() {
    let schema = UserProfile::generate_schema();
    let json = schema.schema.to_value();
    println!("Multiple flatten enums: {}", serde_json::to_string_pretty(&json).unwrap());

    let all_of = json.get("allOf").unwrap().as_array().unwrap();
    // Should have: base properties + 2 enum schemas
    assert_eq!(all_of.len(), 3, "allOf should have 3 elements for multiple flatten enums");
}

// === Mixed Flatten (Struct + Enum) ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Metadata {
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct MixedFlatten {
    id: String,
    #[serde(flatten)]
    metadata: Metadata,
    #[serde(flatten)]
    status: Status,
}

#[test]
fn test_mixed_flatten_struct_and_enum() {
    let schema = MixedFlatten::generate_schema();
    let json = schema.schema.to_value();
    println!("Mixed flatten: {}", serde_json::to_string_pretty(&json).unwrap());

    // Should have allOf because of enum
    let all_of = json.get("allOf").unwrap().as_array().unwrap();

    // Base schema should include both id and metadata fields (struct flatten is merged)
    let base = &all_of[0];
    let props = base.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("id"));
    assert!(props.contains_key("created_at"), "should have flattened struct field");
    assert!(props.contains_key("updated_at"), "should have flattened struct field");
}
