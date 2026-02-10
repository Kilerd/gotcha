use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    Backlog,
    Todo,
    InProgress,
    InReview,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    VeryLow,
    Low,
    Medium,
    High,
    VeryHigh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Schematic)]
pub enum WithRename {
    #[serde(rename = "custom-one")]
    VariantOne,
    #[serde(rename = "custom_two")]
    VariantTwo,
    NormalVariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Schematic)]
#[serde(rename_all = "camelCase")]
pub enum CamelCaseEnum {
    FirstOption,
    SecondOption,
    ThirdOption,
}

fn main() {
    // Test kebab-case
    let schema = TaskStatus::generate_schema();
    let enum_values = schema.schema.extras.get("enum").unwrap().as_array().unwrap();
    assert_eq!(enum_values.len(), 5);
    assert!(enum_values.contains(&serde_json::json!("backlog")));
    assert!(enum_values.contains(&serde_json::json!("todo")));
    assert!(enum_values.contains(&serde_json::json!("in-progress")));
    assert!(enum_values.contains(&serde_json::json!("in-review")));
    assert!(enum_values.contains(&serde_json::json!("done")));

    // Test snake_case
    let schema = Priority::generate_schema();
    let enum_values = schema.schema.extras.get("enum").unwrap().as_array().unwrap();
    assert!(enum_values.contains(&serde_json::json!("very_low")));
    assert!(enum_values.contains(&serde_json::json!("very_high")));

    // Test SCREAMING_SNAKE_CASE
    let schema = LogLevel::generate_schema();
    let enum_values = schema.schema.extras.get("enum").unwrap().as_array().unwrap();
    assert!(enum_values.contains(&serde_json::json!("DEBUG")));
    assert!(enum_values.contains(&serde_json::json!("WARNING")));

    // Test individual rename
    let schema = WithRename::generate_schema();
    let enum_values = schema.schema.extras.get("enum").unwrap().as_array().unwrap();
    assert!(enum_values.contains(&serde_json::json!("custom-one")));
    assert!(enum_values.contains(&serde_json::json!("custom_two")));
    assert!(enum_values.contains(&serde_json::json!("NormalVariant")));

    // Test camelCase
    let schema = CamelCaseEnum::generate_schema();
    let enum_values = schema.schema.extras.get("enum").unwrap().as_array().unwrap();
    assert!(enum_values.contains(&serde_json::json!("firstOption")));
    assert!(enum_values.contains(&serde_json::json!("secondOption")));
    assert!(enum_values.contains(&serde_json::json!("thirdOption")));

    println!("All serde rename tests for simple enum passed!");
}
