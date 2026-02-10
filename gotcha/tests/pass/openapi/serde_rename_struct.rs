use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    user_name: String,
    email_address: String,
    phone_number: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Constants {
    max_retry_count: u32,
    default_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct WithFieldRename {
    #[serde(rename = "customFieldName")]
    normal_field: String,
    #[serde(rename = "another-custom")]
    other_field: i32,
    untouched_field: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "kebab-case")]
pub struct MixedRename {
    normal_field: String,
    #[serde(rename = "overridden")]
    renamed_field: i32,
}

fn main() {
    // Test camelCase struct
    let schema = UserProfile::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("userName"));
    assert!(properties.contains_key("emailAddress"));
    assert!(properties.contains_key("phoneNumber"));
    assert!(properties.contains_key("createdAt"));
    assert!(!properties.contains_key("user_name"));
    assert!(!properties.contains_key("email_address"));

    // Test SCREAMING_SNAKE_CASE struct
    let schema = Constants::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("MAX_RETRY_COUNT"));
    assert!(properties.contains_key("DEFAULT_TIMEOUT"));

    // Test individual field rename
    let schema = WithFieldRename::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("customFieldName"));
    assert!(properties.contains_key("another-custom"));
    assert!(properties.contains_key("untouched_field"));

    // Test mixed rename (rename_all + individual rename override)
    let schema = MixedRename::generate_schema();
    let properties = schema.schema.extras.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("normal-field"));
    assert!(properties.contains_key("overridden"));
    assert!(!properties.contains_key("renamed_field"));
    assert!(!properties.contains_key("renamed-field"));

    println!("All serde rename tests for struct passed!");
}
