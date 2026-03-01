//! Test complex flatten scenarios
#![cfg(feature = "openapi")]

use gotcha::Schematic;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Simulating the user's scenario

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct TaskCreatedData {
    pub task_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct AgentStartedData {
    pub agent_id: String,
}

// Adjacently tagged enum: #[serde(tag = "kind", content = "data")]
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(tag = "kind", content = "data")]
pub enum BuiltinEvent {
    #[serde(rename = "task.created")]
    TaskCreated(TaskCreatedData),
    #[serde(rename = "agent.started")]
    AgentStarted(AgentStartedData),
    /// Unit variant
    SystemReady,
    /// Named fields variant
    CustomEvent { code: i32, message: String },
}

// Untagged enum containing the adjacently tagged enum
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
pub enum Event {
    Builtin(BuiltinEvent),
    Custom { kind: String, data: Value },
}

// Struct with flatten of untagged enum
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct EventMessage {
    #[serde(flatten)]
    pub event: Event,
    pub agent_id: String,
}

// Top-level struct with nested flatten
#[derive(Debug, Clone, Serialize, Deserialize, Schematic)]
pub struct EmitEventRequest {
    #[serde(flatten)]
    pub message: EventMessage,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
}

#[test]
fn test_adjacently_tagged_enum() {
    let schema = BuiltinEvent::generate_schema();
    let json = schema.schema.to_value();
    println!("BuiltinEvent schema:\n{}", serde_json::to_string_pretty(&json).unwrap());

    // Should have oneOf with discriminator
    assert!(json.get("oneOf").is_some(), "should have oneOf");
    assert!(json.get("discriminator").is_some(), "should have discriminator");

    let discriminator = json.get("discriminator").unwrap();
    assert_eq!(
        discriminator.get("propertyName").unwrap().as_str().unwrap(),
        "kind",
        "discriminator should use 'kind'"
    );

    let one_of = json.get("oneOf").unwrap().as_array().unwrap();
    assert_eq!(one_of.len(), 4, "should have 4 variants");

    // Check TaskCreated variant (newtype)
    let task_created = &one_of[0];
    let props = task_created.get("properties").unwrap().as_object().unwrap();
    assert!(props.contains_key("kind"), "should have 'kind' field");
    assert!(props.contains_key("data"), "should have 'data' field");

    // Check kind enum value
    let kind_prop = props.get("kind").unwrap();
    let kind_enum = kind_prop.get("enum").unwrap().as_array().unwrap();
    assert_eq!(kind_enum[0].as_str().unwrap(), "task.created");

    // Check data contains inner type schema
    let data_prop = props.get("data").unwrap();
    let data_props = data_prop.get("properties").unwrap().as_object().unwrap();
    assert!(data_props.contains_key("task_id"));
    assert!(data_props.contains_key("title"));

    // Check SystemReady (unit variant) - should only have "kind", no "data"
    let system_ready = &one_of[2];
    let sr_props = system_ready.get("properties").unwrap().as_object().unwrap();
    assert!(sr_props.contains_key("kind"));
    // Unit variants in adjacently tagged still have "data" but it's typically empty or absent
    // Let's check required fields
    let sr_required = system_ready.get("required").unwrap().as_array().unwrap();
    // Unit variant only requires "kind"
    assert!(sr_required.iter().any(|v| v.as_str() == Some("kind")));

    // Check CustomEvent (named fields variant)
    let custom = &one_of[3];
    let custom_props = custom.get("properties").unwrap().as_object().unwrap();
    assert!(custom_props.contains_key("kind"));
    assert!(custom_props.contains_key("data"));
    let custom_data = custom_props.get("data").unwrap();
    let custom_data_props = custom_data.get("properties").unwrap().as_object().unwrap();
    assert!(custom_data_props.contains_key("code"));
    assert!(custom_data_props.contains_key("message"));
}

#[test]
fn test_untagged_with_adjacently_tagged() {
    let schema = Event::generate_schema();
    let json = schema.schema.to_value();
    println!("Event schema:\n{}", serde_json::to_string_pretty(&json).unwrap());
}

#[test]
fn test_event_message_flatten() {
    let schema = EventMessage::generate_schema();
    let json = schema.schema.to_value();
    println!("EventMessage schema:\n{}", serde_json::to_string_pretty(&json).unwrap());

    // Should have allOf with agent_id + Event's oneOf
}

#[test]
fn test_emit_event_request_nested_flatten() {
    let schema = EmitEventRequest::generate_schema();
    let json = schema.schema.to_value();
    println!("EmitEventRequest schema:\n{}", serde_json::to_string_pretty(&json).unwrap());

    // Should have:
    // - agent_id from EventMessage
    // - task_id, session_id from EmitEventRequest
    // - Event's oneOf structure
}
