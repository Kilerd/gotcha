//! Integration tests for serde(flatten) support in Schematic
#![cfg(feature = "openapi")]

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

/// Pagination parameters
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Pagination {
    /// Page number
    page: u32,
    /// Items per page
    per_page: u32,
}

/// Sorting parameters
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct Sorting {
    sort_by: String,
    sort_order: Option<String>,
}

/// Request with single flatten field
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct ListRequest {
    /// Search query
    query: String,
    #[serde(flatten)]
    pagination: Pagination,
}

/// Request with multiple flatten fields
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct AdvancedListRequest {
    query: String,
    #[serde(flatten)]
    pagination: Pagination,
    #[serde(flatten)]
    sorting: Sorting,
}

/// Request with flatten and rename_all
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "camelCase")]
pub struct CamelCaseRequest {
    user_name: String,
    #[serde(flatten)]
    pagination: Pagination,
}

/// Inner struct for nested flatten test
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct PaginatedSorting {
    #[serde(flatten)]
    pagination: Pagination,
    #[serde(flatten)]
    sorting: Sorting,
}

/// Request with nested flatten (flatten of flatten)
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct NestedFlattenRequest {
    query: String,
    #[serde(flatten)]
    paginated_sorting: PaginatedSorting,
}

/// Request with flatten and field rename
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct MixedRenameRequest {
    #[serde(rename = "searchQuery")]
    query: String,
    #[serde(flatten)]
    pagination: Pagination,
}

/// Request with optional flatten field
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct OptionalFields {
    name: String,
    #[serde(flatten)]
    pagination: Pagination,
    extra: Option<String>,
}

#[test]
fn test_single_flatten_field() {
    let schema = ListRequest::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();

    assert!(properties.contains_key("query"), "should have 'query' field");
    assert!(properties.contains_key("page"), "should have flattened 'page' field");
    assert!(properties.contains_key("per_page"), "should have flattened 'per_page' field");
    assert!(!properties.contains_key("pagination"), "should not have 'pagination' as nested object");
}

#[test]
fn test_multiple_flatten_fields() {
    let schema = AdvancedListRequest::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();

    assert!(properties.contains_key("query"));
    assert!(properties.contains_key("page"));
    assert!(properties.contains_key("per_page"));
    assert!(properties.contains_key("sort_by"));
    assert!(properties.contains_key("sort_order"));
    assert!(!properties.contains_key("pagination"));
    assert!(!properties.contains_key("sorting"));
}

#[test]
fn test_flatten_with_rename_all() {
    let schema = CamelCaseRequest::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();

    // userName should be camelCase (from rename_all)
    assert!(properties.contains_key("userName"), "should have camelCase 'userName'");
    assert!(!properties.contains_key("user_name"));
    // Flattened fields should keep their original names (from Pagination)
    assert!(properties.contains_key("page"));
    assert!(properties.contains_key("per_page"));
}

#[test]
fn test_nested_flatten() {
    let schema = NestedFlattenRequest::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();

    assert!(properties.contains_key("query"));
    assert!(properties.contains_key("page"));
    assert!(properties.contains_key("per_page"));
    assert!(properties.contains_key("sort_by"));
    assert!(properties.contains_key("sort_order"));
    assert!(!properties.contains_key("paginated_sorting"));
    assert!(!properties.contains_key("pagination"));
    assert!(!properties.contains_key("sorting"));
}

#[test]
fn test_flatten_with_field_rename() {
    let schema = MixedRenameRequest::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();

    assert!(properties.contains_key("searchQuery"), "should have renamed 'searchQuery'");
    assert!(!properties.contains_key("query"));
    assert!(properties.contains_key("page"));
    assert!(properties.contains_key("per_page"));
}

#[test]
fn test_required_fields_tracking() {
    let schema = OptionalFields::generate_schema();
    let required = schema.schema.extras.get("required").unwrap().as_array().unwrap();
    let required_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();

    assert!(required_names.contains(&"name"), "name should be required");
    assert!(required_names.contains(&"page"), "page should be required");
    assert!(required_names.contains(&"per_page"), "per_page should be required");
    assert!(!required_names.contains(&"extra"), "extra should not be required");
}

#[test]
fn test_fields_method() {
    let fields = ListRequest::fields();
    let field_names: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();

    assert!(field_names.contains(&"query"));
    assert!(field_names.contains(&"page"));
    assert!(field_names.contains(&"per_page"));
    assert_eq!(fields.len(), 3);
}

#[test]
fn test_documentation_preserved() {
    let fields = ListRequest::fields();
    let page_field = fields.iter().find(|(name, _)| *name == "page").unwrap();
    assert!(
        page_field.1.schema.description.as_ref().map(|s| s.contains("Page number")).unwrap_or(false),
        "page field should preserve documentation"
    );
}
