//! Advanced tests for internally tagged enum
#![cfg(feature = "openapi")]

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

// === Tagged enum with rename_all on variants ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskStatus {
    InProgress { started_at: String },
    Completed { finished_at: String },
    Failed { error_message: String },
}

#[test]
fn test_tagged_enum_variant_rename() {
    let schema = TaskStatus::generate_schema();
    let json = schema.schema.to_value();
    println!("Tagged with rename_all: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();

    // Check that variant names are snake_case
    for variant in one_of {
        let props = variant.get("properties").unwrap().as_object().unwrap();
        let type_prop = props.get("type").unwrap();
        let enum_values = type_prop.get("enum").unwrap().as_array().unwrap();
        let variant_name = enum_values[0].as_str().unwrap();

        // All variant names should be snake_case
        assert!(
            variant_name == "in_progress" || variant_name == "completed" || variant_name == "failed",
            "Variant name should be snake_case: {}",
            variant_name
        );
    }
}

// === Tagged enum with custom tag and rename ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "event_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SystemEvent {
    UserCreated { user_id: i32 },
    UserDeleted { user_id: i32 },
    SystemRestart,
}

#[test]
fn test_tagged_enum_custom_tag_with_rename() {
    let schema = SystemEvent::generate_schema();
    let json = schema.schema.to_value();
    println!("Custom tag with rename: {}", serde_json::to_string_pretty(&json).unwrap());

    // Check discriminator uses custom tag name
    let discriminator = json.get("discriminator").unwrap();
    assert_eq!(
        discriminator.get("propertyName").unwrap().as_str().unwrap(),
        "event_type"
    );

    // Check variant names are SCREAMING_SNAKE_CASE
    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    let first_variant = &one_of[0];
    let props = first_variant.get("properties").unwrap().as_object().unwrap();

    // Should have event_type property
    assert!(props.contains_key("event_type"));

    let type_prop = props.get("event_type").unwrap();
    let enum_values = type_prop.get("enum").unwrap().as_array().unwrap();
    let variant_name = enum_values[0].as_str().unwrap();
    assert_eq!(variant_name, "USER_CREATED");
}

// === Tagged enum with individual variant rename ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type")]
pub enum ApiResponse {
    #[serde(rename = "ok")]
    Success { data: String },
    #[serde(rename = "err")]
    Error { message: String, code: i32 },
}

#[test]
fn test_tagged_enum_individual_variant_rename() {
    let schema = ApiResponse::generate_schema();
    let json = schema.schema.to_value();
    println!("Individual rename: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();

    // Collect all variant type values
    let variant_names: Vec<&str> = one_of
        .iter()
        .filter_map(|v| {
            v.get("properties")
                .and_then(|p| p.get("type"))
                .and_then(|t| t.get("enum"))
                .and_then(|e| e.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
        })
        .collect();

    assert!(variant_names.contains(&"ok"), "Should have renamed variant 'ok'");
    assert!(variant_names.contains(&"err"), "Should have renamed variant 'err'");
}

// === Tagged enum with nested struct (no flatten inside variant) ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Metadata {
    created_by: String,
    version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type")]
pub enum Document {
    Draft {
        content: String,
        metadata: Metadata,
    },
    Published {
        content: String,
        url: String,
        metadata: Metadata,
    },
}

#[test]
fn test_tagged_enum_with_nested_struct_in_variant() {
    let schema = Document::generate_schema();
    let json = schema.schema.to_value();
    println!("Tagged with nested struct: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();

    // Draft variant should have content + metadata as nested object
    let draft = &one_of[0];
    let props = draft.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("type"));
    assert!(props.contains_key("content"));
    assert!(props.contains_key("metadata"), "Should have metadata as nested object");

    // Metadata should be an object with its own properties
    let metadata_schema = props.get("metadata").unwrap();
    let metadata_props = metadata_schema.get("properties").unwrap().as_object().unwrap();
    assert!(metadata_props.contains_key("created_by"));
    assert!(metadata_props.contains_key("version"));
}

// === Tagged enum with Vec fields ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type")]
pub enum Collection {
    List { items: Vec<String> },
    Set { values: Vec<i32> },
}

#[test]
fn test_tagged_enum_with_vec_fields() {
    let schema = Collection::generate_schema();
    let json = schema.schema.to_value();
    println!("Tagged with vec: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();

    // List variant should have items array
    let list = &one_of[0];
    let props = list.get("properties").unwrap().as_object().unwrap();
    let items = props.get("items").unwrap();
    assert_eq!(items.get("type").unwrap().as_str().unwrap(), "array");
}

// === Unit variant in tagged enum ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "status")]
pub enum ProcessStatus {
    Pending,
    Running { pid: i32 },
    Stopped,
    Error { message: String },
}

#[test]
fn test_tagged_enum_with_unit_variants() {
    let schema = ProcessStatus::generate_schema();
    let json = schema.schema.to_value();
    println!("Tagged with unit variants: {}", serde_json::to_string_pretty(&json).unwrap());

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 4);

    // Unit variant (Pending) should only have the tag field
    let pending = &one_of[0];
    let props = pending.get("properties").unwrap().as_object().unwrap();
    let required = pending.get("required").unwrap().as_array().unwrap();

    assert!(props.contains_key("status"));
    assert_eq!(required.len(), 1, "Unit variant should only require the tag");
}

// === Flatten tagged enum ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Task {
    id: String,
    name: String,
    #[serde(flatten)]
    status: TaskStatus,
}

#[test]
fn test_flatten_tagged_enum() {
    let schema = Task::generate_schema();
    let json = schema.schema.to_value();
    println!("Flatten tagged enum: {}", serde_json::to_string_pretty(&json).unwrap());

    // Should have allOf
    let all_of = json.get("allOf").unwrap().as_array().unwrap();

    // Base properties
    let base = &all_of[0];
    let props = base.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("id"));
    assert!(props.contains_key("name"));

    // Enum schema
    let enum_schema = &all_of[1];
    assert!(enum_schema.get("oneOf").is_some());
    assert!(enum_schema.get("discriminator").is_some());
}
