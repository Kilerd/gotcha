#![cfg(feature = "openapi")]
#![allow(dead_code)]

use std::collections::HashMap;

use gotcha::axum::http::Method;
use gotcha::{api, openapi::generate_openapi, Json, Nullable, Optional, Schematic};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Schematic, Serialize, Deserialize)]
struct Registered {
    id: u32,
}

#[derive(Schematic, Serialize, Deserialize)]
struct Contract {
    required_item: Nullable<Registered>,
    nullable_name: Nullable<String>,
    #[serde(skip_serializing_if = "Optional::is_none")]
    optional_name: Optional<String>,
    #[serde(skip_serializing_if = "Optional::is_none")]
    optional_item: Optional<Registered>,
    #[serde(skip_serializing_if = "Optional::is_none")]
    optional_payload: Optional<Value>,
}

#[api]
async fn contract() -> Json<Contract> {
    unimplemented!()
}

fn operable<H, T>(_: H) -> &'static gotcha::openapi::Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().unwrap()
}

#[test]
fn optional_and_nullable_fields_document_distinct_contracts() {
    let routes = HashMap::from([(("/contract".to_string(), Method::GET), operable(contract))]);
    let spec = serde_json::to_value(generate_openapi(routes)).unwrap();

    assert_eq!(
        spec["paths"]["/contract"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Contract"
    );
    let contract = &spec["components"]["schemas"]["Contract"];
    assert_eq!(contract["type"], "object");
    // `Nullable` fields are required; `Optional` fields may be absent.
    assert_eq!(contract["required"], json!(["required_item", "nullable_name"]));
    assert_eq!(
        contract["properties"]["required_item"],
        json!({"anyOf": [
            {"$ref": "#/components/schemas/Registered"},
            {"type": "null"}
        ]})
    );
    assert_eq!(
        contract["properties"]["nullable_name"],
        json!({"anyOf": [{"type": "string"}, {"type": "null"}]})
    );
    // `Optional` keeps the inner schema untouched and adds no null branch.
    assert_eq!(contract["properties"]["optional_name"], json!({"type": "string"}));
    assert_eq!(contract["properties"]["optional_item"], json!({"$ref": "#/components/schemas/Registered"}));
    assert_eq!(contract["properties"]["optional_payload"], json!({}));

    // Nullability lives at the use site; the referenced component stays unweakened.
    let registered = &spec["components"]["schemas"]["Registered"];
    assert_eq!(registered["type"], "object");
    assert_eq!(registered["required"], json!(["id"]));
    assert!(!spec.to_string().contains("\"nullable\":"));
    assert!(!spec.to_string().contains("gotcha:rust-type:"));
}
