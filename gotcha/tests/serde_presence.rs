#![cfg(feature = "openapi")]
#![allow(dead_code)]

use gotcha::{Either, Json, ParameterProvider, Query, Schematic};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

fn document<T: Schematic>() -> Value {
    serde_json::to_value(T::generate_schema().schema).unwrap()
}

fn required(schema: &Value) -> Vec<String> {
    schema["required"]
        .as_array()
        .map(|values| values.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

// Plain `Option<T>`: optional and nullable, unchanged from before.
#[derive(Debug, Serialize, Deserialize, Schematic)]
struct PlainOption {
    value: Option<String>,
}

#[test]
fn plain_option_stays_optional_and_nullable() {
    let schema = document::<PlainOption>();
    assert_eq!(schema["properties"]["value"], json!({"anyOf": [{"type": "string"}, {"type": "null"}]}));
    assert!(required(&schema).is_empty());

    assert_eq!(serde_json::from_str::<PlainOption>("{}").unwrap().value, None);
    assert_eq!(serde_json::from_str::<PlainOption>(r#"{"value": null}"#).unwrap().value, None);
    assert_eq!(serde_json::from_str::<PlainOption>(r#"{"value": "x"}"#).unwrap().value.as_deref(), Some("x"));
    assert_eq!(serde_json::to_string(&PlainOption { value: None }).unwrap(), r#"{"value":null}"#);
}

// `deserialize_with` without `default`: serde requires the key, even for `Option<T>`.
#[derive(Debug, Serialize, Deserialize, Schematic)]
struct RequiredOption {
    #[serde(deserialize_with = "Option::deserialize")]
    value: Option<String>,
}

#[test]
fn deserialize_with_makes_option_required_and_nullable() {
    let schema = document::<RequiredOption>();
    assert_eq!(schema["properties"]["value"], json!({"anyOf": [{"type": "string"}, {"type": "null"}]}));
    assert_eq!(required(&schema), ["value"]);

    let error = serde_json::from_str::<RequiredOption>("{}").unwrap_err();
    assert!(error.to_string().contains("value"), "{error}");
    assert_eq!(serde_json::from_str::<RequiredOption>(r#"{"value": null}"#).unwrap().value, None);
    assert_eq!(serde_json::from_str::<RequiredOption>(r#"{"value": "x"}"#).unwrap().value.as_deref(), Some("x"));
    assert_eq!(serde_json::to_string(&RequiredOption { value: None }).unwrap(), r#"{"value":null}"#);
}

mod transparent_u32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u32, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(*value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
        u32::deserialize(deserializer)
    }
}

// `with` on a non-Option field is also required; this was already the case for primitives, but
// custom `required() == false` types now follow serde as well.
#[derive(Debug, Serialize, Deserialize, Schematic)]
struct RequiredWith {
    #[serde(with = "transparent_u32")]
    count: u32,
}

#[test]
fn with_makes_a_field_required() {
    let schema = document::<RequiredWith>();
    assert_eq!(required(&schema), ["count"]);
    let error = serde_json::from_str::<RequiredWith>("{}").unwrap_err();
    assert!(error.to_string().contains("count"), "{error}");
    assert_eq!(serde_json::from_str::<RequiredWith>(r#"{"count": 3}"#).unwrap().count, 3);
}

// `Option::is_none` skips: optional, and the schema drops the null branch.
#[derive(Debug, Serialize, Deserialize, Schematic)]
struct SkipsNone {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
struct SkipsNoneStd {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
struct SkipsNoneCore {
    #[serde(skip_serializing_if = "core::option::Option::is_none")]
    value: Option<String>,
}

#[test]
fn option_is_none_skip_is_optional_without_null() {
    for schema in [document::<SkipsNone>(), document::<SkipsNoneStd>(), document::<SkipsNoneCore>()] {
        assert_eq!(schema["properties"]["value"], json!({"type": "string"}));
        assert!(required(&schema).is_empty());
    }

    assert_eq!(serde_json::from_str::<SkipsNone>("{}").unwrap().value, None);
    assert_eq!(serde_json::from_str::<SkipsNone>(r#"{"value": null}"#).unwrap().value, None);
    assert_eq!(serde_json::to_string(&SkipsNone { value: None }).unwrap(), "{}");
    assert_eq!(serde_json::to_string(&SkipsNone { value: Some("x".into()) }).unwrap(), r#"{"value":"x"}"#);
}

fn skips_empty(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(str::is_empty)
}

// A predicate other than the recognized `Option::is_none` keeps the null branch.
#[derive(Debug, Serialize, Deserialize, Schematic)]
struct OtherSkip {
    #[serde(skip_serializing_if = "skips_empty")]
    value: Option<String>,
}

#[test]
fn other_skip_predicates_keep_the_null_branch() {
    let schema = document::<OtherSkip>();
    assert_eq!(schema["properties"]["value"], json!({"anyOf": [{"type": "string"}, {"type": "null"}]}));
    assert!(required(&schema).is_empty());

    assert_eq!(serde_json::to_string(&OtherSkip { value: None }).unwrap(), "{}");
    assert_eq!(serde_json::to_string(&OtherSkip { value: Some(String::new()) }).unwrap(), "{}");
}

// Tri-state: missing, explicit null and a value.
fn deserialize_some<'de, T: Deserialize<'de>, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schematic)]
struct TriState {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    attributes: Option<Option<String>>,
}

#[test]
fn nested_option_has_one_null_branch_and_round_trips() {
    let schema = document::<TriState>();
    assert_eq!(schema["properties"]["attributes"], json!({"anyOf": [{"type": "string"}, {"type": "null"}]}));
    assert!(required(&schema).is_empty());

    assert_eq!(serde_json::from_str::<TriState>("{}").unwrap().attributes, None);
    assert_eq!(serde_json::from_str::<TriState>(r#"{"attributes": null}"#).unwrap().attributes, Some(None));
    assert_eq!(
        serde_json::from_str::<TriState>(r#"{"attributes": "x"}"#).unwrap().attributes,
        Some(Some("x".into()))
    );
    assert_eq!(serde_json::to_string(&TriState { attributes: None }).unwrap(), "{}");
    assert_eq!(serde_json::to_string(&TriState { attributes: Some(None) }).unwrap(), r#"{"attributes":null}"#);
    assert_eq!(
        serde_json::to_string(&TriState {
            attributes: Some(Some("x".into()))
        })
        .unwrap(),
        r#"{"attributes":"x"}"#
    );
}

// A custom deserializer requires the key while `Option::is_none` omits it on output. One schema
// cannot express both directions, so the previous optional + nullable contract is kept.
#[derive(Debug, Serialize, Deserialize, Schematic)]
struct AsymmetricOption {
    #[serde(deserialize_with = "Option::deserialize", skip_serializing_if = "Option::is_none")]
    value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
struct AsymmetricNested {
    #[serde(deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    attributes: Option<Option<String>>,
}

#[test]
fn deserialize_with_and_option_is_none_skip_stay_optional_and_nullable() {
    let schema = document::<AsymmetricOption>();
    assert_eq!(schema["properties"]["value"], json!({"anyOf": [{"type": "string"}, {"type": "null"}]}));
    assert!(required(&schema).is_empty());

    let error = serde_json::from_str::<AsymmetricOption>("{}").unwrap_err();
    assert!(error.to_string().contains("value"), "{error}");
    assert_eq!(serde_json::from_str::<AsymmetricOption>(r#"{"value": null}"#).unwrap().value, None);
    assert_eq!(
        serde_json::from_str::<AsymmetricOption>(r#"{"value": "x"}"#).unwrap().value.as_deref(),
        Some("x")
    );
    assert_eq!(serde_json::to_string(&AsymmetricOption { value: None }).unwrap(), "{}");

    let schema = document::<AsymmetricNested>();
    assert_eq!(schema["properties"]["attributes"], json!({"anyOf": [{"type": "string"}, {"type": "null"}]}));
    assert!(required(&schema).is_empty());

    assert!(serde_json::from_str::<AsymmetricNested>("{}").is_err());
    assert_eq!(
        serde_json::from_str::<AsymmetricNested>(r#"{"attributes": null}"#).unwrap().attributes,
        Some(None)
    );
    assert_eq!(serde_json::to_string(&AsymmetricNested { attributes: None }).unwrap(), "{}");
    assert_eq!(
        serde_json::to_string(&AsymmetricNested { attributes: Some(None) }).unwrap(),
        r#"{"attributes":null}"#
    );
}

// Container `#[serde(default)]` makes every field optional.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize, Schematic)]
#[serde(default)]
struct ContainerDefault {
    name: String,
    count: u32,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize, Schematic)]
#[serde(default)]
struct ContainerDefaultWith {
    #[serde(deserialize_with = "Option::deserialize")]
    value: Option<String>,
}

#[test]
fn container_default_makes_every_field_optional() {
    let schema = document::<ContainerDefault>();
    assert!(required(&schema).is_empty());
    assert_eq!(serde_json::from_str::<ContainerDefault>("{}").unwrap(), ContainerDefault::default());
    assert_eq!(
        serde_json::to_string(&serde_json::from_str::<ContainerDefault>("{}").unwrap()).unwrap(),
        r#"{"name":"","count":0}"#
    );

    // A container default supplies missing values even for `deserialize_with` fields.
    let schema = document::<ContainerDefaultWith>();
    assert!(required(&schema).is_empty());
    assert_eq!(serde_json::from_str::<ContainerDefaultWith>("{}").unwrap().value, None);
}

// Struct-like enum variants use the same presence rules.
#[derive(Debug, Serialize, Deserialize, Schematic)]
#[serde(tag = "kind")]
enum InternalTagged {
    Ready {
        #[serde(deserialize_with = "Option::deserialize")]
        value: Option<String>,
    },
    Skipped {
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
#[serde(tag = "kind", content = "data")]
enum AdjacentTagged {
    Named {
        #[serde(deserialize_with = "Option::deserialize")]
        value: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
enum ExternalTagged {
    Named {
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
#[serde(untagged)]
enum Untagged {
    Named {
        #[serde(deserialize_with = "Option::deserialize")]
        value: Option<String>,
    },
}

fn branch<'a>(schema: &'a Value, title: &str) -> &'a Value {
    schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .find(|branch| branch["title"] == title)
        .unwrap_or_else(|| panic!("missing branch {title}: {schema}"))
}

#[test]
fn struct_like_variants_follow_the_same_presence_rules() {
    let schema = document::<InternalTagged>();
    assert_eq!(required(branch(&schema, "Ready")), ["kind", "value"]);
    assert_eq!(required(branch(&schema, "Skipped")), ["kind"]);
    assert_eq!(branch(&schema, "Skipped")["properties"]["value"], json!({"type": "string"}));

    let error = serde_json::from_str::<InternalTagged>(r#"{"kind": "Ready"}"#).unwrap_err();
    assert!(error.to_string().contains("value"), "{error}");
    assert_eq!(
        serde_json::from_str::<InternalTagged>(r#"{"kind": "Ready", "value": null}"#)
            .unwrap()
            .into_value(),
        None
    );

    let schema = document::<AdjacentTagged>();
    assert_eq!(branch(&schema, "Named")["properties"]["data"]["required"], json!(["value"]));

    let schema = document::<ExternalTagged>();
    assert_eq!(branch(&schema, "Named")["properties"]["Named"]["required"], json!([]));
    assert_eq!(
        branch(&schema, "Named")["properties"]["Named"]["properties"]["value"],
        json!({"type": "string"})
    );

    let schema = document::<Untagged>();
    assert_eq!(branch(&schema, "Named")["required"], json!(["value"]));
    assert_eq!(
        branch(&schema, "Named")["properties"]["value"],
        json!({"anyOf": [{"type": "string"}, {"type": "null"}]})
    );
}

impl InternalTagged {
    fn into_value(self) -> Option<String> {
        match self {
            Self::Ready { value } | Self::Skipped { value } => value,
        }
    }
}

// `Query` documents the `fields()` requiredness, which now follows serde.
#[derive(Debug, Deserialize, Schematic)]
struct QueryPresence {
    #[serde(deserialize_with = "Option::deserialize")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<String>,
}

#[test]
fn query_parameters_follow_field_presence() {
    let Either::Left(parameters) = <Query<QueryPresence> as ParameterProvider>::generate(String::new()) else {
        panic!("query parameters expected");
    };
    let value = parameters.iter().find(|parameter| parameter.name == "value").unwrap();
    assert_eq!(value.required, Some(true));
    assert_eq!(
        serde_json::to_value(value.schema.as_ref().unwrap().as_object().unwrap()).unwrap(),
        json!({"anyOf": [{"type": "string"}, {"type": "null"}]})
    );
    let filter = parameters.iter().find(|parameter| parameter.name == "filter").unwrap();
    assert_eq!(filter.required, Some(false));
    assert_eq!(
        serde_json::to_value(filter.schema.as_ref().unwrap().as_object().unwrap()).unwrap(),
        json!({"type": "string"})
    );
}

// A bare `Json<T>` handler still compiles with the documented fields.
#[allow(unused)]
async fn echo(body: Json<TriState>) -> Json<TriState> {
    body
}
