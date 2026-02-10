use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    UserCreated { user_id: String },
    UserDeleted { user_id: String, reason: Option<String> },
    OrderPlaced { order_id: String, total_amount: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    TextMessage { content: String },
    ImageMessage { url: String, width: u32, height: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "type")]
pub enum WithVariantRename {
    #[serde(rename = "custom_variant")]
    VariantA { data: String },
    NormalVariant { value: i32 },
}

fn main() {
    // Test kebab-case tagged enum
    let schema = Event::generate_schema();
    let one_of = schema.schema.extras.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 3);

    // Check that variant names are kebab-case in the discriminator enum
    let first_variant = &one_of[0];
    let props = first_variant.get("properties").unwrap().as_object().unwrap();
    let type_prop = props.get("type").unwrap().as_object().unwrap();
    let enum_values = type_prop.get("enum").unwrap().as_array().unwrap();
    assert!(enum_values.contains(&serde_json::json!("user-created")));

    // Test snake_case tagged enum
    let schema = Message::generate_schema();
    let one_of = schema.schema.extras.get("oneOf").unwrap().as_array().unwrap();
    let first_variant = &one_of[0];
    let props = first_variant.get("properties").unwrap().as_object().unwrap();
    let type_prop = props.get("type").unwrap().as_object().unwrap();
    let enum_values = type_prop.get("enum").unwrap().as_array().unwrap();
    assert!(enum_values.contains(&serde_json::json!("text_message")));

    // Test individual variant rename
    let schema = WithVariantRename::generate_schema();
    let one_of = schema.schema.extras.get("oneOf").unwrap().as_array().unwrap();
    let first_variant = &one_of[0];
    let props = first_variant.get("properties").unwrap().as_object().unwrap();
    let type_prop = props.get("type").unwrap().as_object().unwrap();
    let enum_values = type_prop.get("enum").unwrap().as_array().unwrap();
    assert!(enum_values.contains(&serde_json::json!("custom_variant")));

    println!("All serde rename tests for tagged enum passed!");
}
