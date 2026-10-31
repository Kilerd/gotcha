//! `#[schematic(as = "Type")]` describes a field as another type.

use gotcha::Schematic;
use serde_json::json;

/// Deliberately implements no `Schematic`: `as` must shield it from the derive.
#[derive(Default, serde::Deserialize, serde::Serialize)]
#[allow(dead_code)]
struct MyUrl(String);

#[derive(Schematic, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Address {
    #[allow(dead_code)]
    first_name: String,
}

#[derive(Schematic, serde::Deserialize, serde::Serialize)]
#[allow(dead_code)]
struct Link {
    #[schematic(as = "String")]
    url: MyUrl,
    #[schematic(as = "Option<String>")]
    title: MyUrl,
    #[schematic(as = "Address")]
    address: MyUrl,
    // Serde presence still applies on top of `as`.
    #[serde(default)]
    #[schematic(as = "String")]
    fallback: MyUrl,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schematic(as = "Option<String>")]
    subtitle: Option<MyUrl>,
}

fn schema(index: usize) -> serde_json::Value {
    serde_json::to_value(&Link::fields()[index].1.schema).unwrap()
}

#[test]
fn as_replaces_the_schema_type_without_requiring_schematic_on_the_original() {
    let fields = Link::fields();
    assert_eq!(fields[0].0, "url");
    assert_eq!(schema(0), json!({"type": "string"}));
    assert!(fields[0].1.required);

    // `as = "Option<String>"` is how a non-option field keeps nullability.
    assert_eq!(fields[1].0, "title");
    assert_eq!(schema(1), json!({"anyOf": [{"type": "string"}, {"type": "null"}]}));
    assert!(!fields[1].1.required);
}

#[test]
fn serde_presence_applies_on_top_of_as() {
    let fields = Link::fields();

    // `#[serde(default)]` makes the field optional, and `as = "String"` still supplies the type.
    assert_eq!(fields[3].0, "fallback");
    assert_eq!(schema(3), json!({"type": "string"}));
    assert!(!fields[3].1.required);

    // `Option::is_none` skips the null branch the `as = "Option<String>"` type would add.
    assert_eq!(fields[4].0, "subtitle");
    assert_eq!(schema(4), json!({"type": "string"}));
    assert!(!fields[4].1.required);
}

#[test]
fn as_type_supplies_the_nested_wire_mapping() {
    let table = Link::wire_fields();
    let address = table.iter().find(|field| field.rust == "address").expect("address is mapped");
    assert_eq!((address.wire, address.flatten), ("address", false));
    let nested = (address.nested.expect("the as type has a mapping"))();
    assert_eq!(nested[0].rust, "first_name");
    assert_eq!(nested[0].wire, "firstName");
}

#[test]
fn as_leaves_the_original_field_type_out_of_the_generated_code() {
    // The proof is the module compiling: `MyUrl` implements neither `Schematic` nor any of the
    // methods the derive could have delegated to. This test keeps the intent explicit.
    assert_eq!(<Link as Schematic>::type_(), "object");
}
