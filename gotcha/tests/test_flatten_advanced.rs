//! Advanced tests for flatten functionality
#![cfg(feature = "openapi")]

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

// === Only flatten fields (no own properties) ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Base {
    id: i32,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct OnlyFlatten {
    #[serde(flatten)]
    base: Base,
}

#[test]
fn test_struct_with_only_flatten() {
    let schema = OnlyFlatten::generate_schema();
    let json = schema.schema.to_value();
    println!("Only flatten: {}", serde_json::to_string_pretty(&json).unwrap());

    // Should have id and name from base
    let props = json.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("id"));
    assert!(props.contains_key("name"));
    assert_eq!(props.len(), 2);
}

// === Deep nested flatten (3 levels) ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Level1 {
    field1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Level2 {
    field2: String,
    #[serde(flatten)]
    level1: Level1,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Level3 {
    field3: String,
    #[serde(flatten)]
    level2: Level2,
}

#[test]
fn test_deep_nested_flatten() {
    let schema = Level3::generate_schema();
    let json = schema.schema.to_value();
    println!("Deep nested: {}", serde_json::to_string_pretty(&json).unwrap());

    let props = json.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("field1"), "Should have level1 field");
    assert!(props.contains_key("field2"), "Should have level2 field");
    assert!(props.contains_key("field3"), "Should have level3 field");
    assert_eq!(props.len(), 3);
}

// === Flatten with conflicting field names (last wins) ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct First {
    name: String,
    first_only: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Second {
    name: String,
    second_only: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Conflicting {
    #[serde(flatten)]
    first: First,
    #[serde(flatten)]
    second: Second,
}

#[test]
fn test_flatten_with_conflicting_names() {
    let schema = Conflicting::generate_schema();
    let json = schema.schema.to_value();
    println!("Conflicting: {}", serde_json::to_string_pretty(&json).unwrap());

    let props = json.get("properties").unwrap().as_object().unwrap();
    // Both should contribute their unique fields
    assert!(props.contains_key("name"));
    assert!(props.contains_key("first_only"));
    assert!(props.contains_key("second_only"));
}

// === Flatten with rename_all interaction ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct SnakeCaseBase {
    user_name: String,
    user_age: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "camelCase")]
pub struct CamelCaseWrapper {
    wrapper_field: String,
    #[serde(flatten)]
    base: SnakeCaseBase,
}

#[test]
fn test_flatten_rename_all_interaction() {
    let schema = CamelCaseWrapper::generate_schema();
    let json = schema.schema.to_value();
    println!("Rename interaction: {}", serde_json::to_string_pretty(&json).unwrap());

    let props = json.get("properties").unwrap().as_object().unwrap();

    // Own field should be camelCase
    assert!(props.contains_key("wrapperField"), "Own field should be camelCase");

    // Flattened fields keep their original names from the base struct
    assert!(props.contains_key("user_name"), "Flattened field keeps original name");
    assert!(props.contains_key("user_age"), "Flattened field keeps original name");
}

// === Multiple struct flatten + one enum flatten ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Timestamps {
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Audit {
    created_by: String,
    modified_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type")]
pub enum EntityStatus {
    Active,
    Archived { archived_at: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Entity {
    id: i32,
    name: String,
    #[serde(flatten)]
    timestamps: Timestamps,
    #[serde(flatten)]
    audit: Audit,
    #[serde(flatten)]
    status: EntityStatus,
}

#[test]
fn test_multiple_struct_and_enum_flatten() {
    let schema = Entity::generate_schema();
    let json = schema.schema.to_value();
    println!("Multiple flatten: {}", serde_json::to_string_pretty(&json).unwrap());

    // Should have allOf because of enum
    let all_of = json.get("allOf").unwrap().as_array().unwrap();

    // Base should have all struct fields merged
    let base = &all_of[0];
    let props = base.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("id"));
    assert!(props.contains_key("name"));
    assert!(props.contains_key("created_at"));
    assert!(props.contains_key("updated_at"));
    assert!(props.contains_key("created_by"));
    assert!(props.contains_key("modified_by"));

    // Should have enum oneOf
    let enum_schema = &all_of[1];
    assert!(enum_schema.get("oneOf").is_some());
}

// === Empty struct flatten ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Empty {}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct WithEmpty {
    id: i32,
    #[serde(flatten)]
    empty: Empty,
}

#[test]
fn test_empty_struct_flatten() {
    let schema = WithEmpty::generate_schema();
    let json = schema.schema.to_value();
    println!("Empty flatten: {}", serde_json::to_string_pretty(&json).unwrap());

    let props = json.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("id"));
    assert_eq!(props.len(), 1, "Empty struct adds no properties");
}

// === Flatten with doc comments preserved ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Documented {
    /// The user's email address
    email: String,
    /// The user's phone number
    phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct WithDocumented {
    /// Primary identifier
    id: i32,
    #[serde(flatten)]
    contact: Documented,
}

#[test]
fn test_flatten_preserves_docs() {
    let fields = WithDocumented::fields();

    // Find the email field
    let email_field = fields.iter().find(|(name, _)| *name == "email");
    assert!(email_field.is_some());

    let (_, schema) = email_field.unwrap();
    assert!(
        schema.schema.description.as_ref().map(|d| d.contains("email address")).unwrap_or(false),
        "Should preserve doc comment"
    );
}

// === Flatten Option<Struct> ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct OptionalData {
    extra: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct WithOptionalFlatten {
    required_field: String,
    #[serde(flatten)]
    optional: Option<OptionalData>,
}

#[test]
fn test_flatten_option_struct() {
    let schema = WithOptionalFlatten::generate_schema();
    let json = schema.schema.to_value();
    println!("Optional flatten: {}", serde_json::to_string_pretty(&json).unwrap());

    // Option<T> uses T's fields() which returns the inner type's fields
    let props = json.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("required_field"));
    // Option flatten behavior: fields from Option<T> are optional
}

// === Flatten with skip ===
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct WithSkip {
    visible: String,
    #[serde(skip)]
    hidden: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct FlattenWithSkip {
    own: String,
    #[serde(flatten)]
    inner: WithSkip,
}

#[test]
fn test_flatten_with_skip() {
    // Note: This test documents current behavior
    // The skip field handling depends on how serde processes it
    let schema = FlattenWithSkip::generate_schema();
    let json = schema.schema.to_value();
    println!("Flatten with skip: {}", serde_json::to_string_pretty(&json).unwrap());

    let props = json.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("own"));
    assert!(props.contains_key("visible"));
    // Note: Schematic doesn't currently handle #[serde(skip)]
    // The hidden field may or may not appear depending on implementation
}
