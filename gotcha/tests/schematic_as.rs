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
    title: Option<MyUrl>,
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

    // An optional field keeps its nullable schema: `as = "Option<String>"` supplies the null
    // branch for an `Option<MyUrl>` field.
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

/// A newtype struct is transparent, so `as` on its inner field replaces what it delegates to.
#[derive(Schematic)]
#[allow(dead_code)]
struct UrlBox(#[schematic(as = "String")] MyUrl);

#[derive(Schematic)]
#[allow(dead_code)]
struct AddressBox(#[schematic(as = "Address")] MyUrl);

/// Externally tagged (serde's default).
#[derive(Schematic, serde::Serialize)]
#[allow(dead_code)]
enum External {
    Url(#[schematic(as = "String")] MyUrl),
    Record {
        #[schematic(as = "String")]
        value: MyUrl,
    },
}

/// Internally tagged: serde allows only struct and unit variants.
#[derive(Schematic, serde::Serialize)]
#[serde(tag = "kind")]
#[allow(dead_code)]
enum Internal {
    Record {
        #[schematic(as = "String")]
        value: MyUrl,
    },
}

/// Adjacently tagged: newtype and struct variants carry a `data` value.
#[derive(Schematic, serde::Serialize)]
#[serde(tag = "kind", content = "data")]
#[allow(dead_code)]
enum Adjacent {
    Url(#[schematic(as = "String")] MyUrl),
    Record {
        #[schematic(as = "String")]
        value: MyUrl,
    },
}

/// Untagged: newtype, tuple, and struct variants.
#[derive(Schematic, serde::Serialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum Untagged {
    Url(#[schematic(as = "String")] MyUrl),
    Pair(#[schematic(as = "String")] MyUrl, #[schematic(as = "String")] MyUrl),
    Record {
        #[schematic(as = "String")]
        value: MyUrl,
    },
}

fn branches<T: Schematic>() -> serde_json::Value {
    T::generate_schema().schema.to_value()["oneOf"].clone()
}

fn untagged_branches<T: Schematic>() -> serde_json::Value {
    T::generate_schema().schema.to_value()["anyOf"].clone()
}

#[test]
fn as_applies_to_newtype_structs() {
    assert_eq!(UrlBox::generate_schema().schema.to_value(), json!({"type": "string"}));
    assert_eq!(UrlBox::type_(), "string");
    // The `as` type also supplies the nested wire mapping.
    let nested = AddressBox::wire_fields();
    assert_eq!((nested[0].rust, nested[0].wire), ("first_name", "firstName"));
}

#[test]
fn as_applies_to_external_tagged_variants() {
    assert_eq!(
        branches::<External>(),
        json!([
            {
                "title": "Url",
                "type": "object",
                "properties": {"Url": {"type": "string"}},
                "required": ["Url"]
            },
            {
                "title": "Record",
                "type": "object",
                "properties": {
                    "Record": {
                        "type": "object",
                        "properties": {"value": {"type": "string"}},
                        "required": ["value"]
                    }
                },
                "required": ["Record"]
            }
        ])
    );
}

#[test]
fn as_applies_to_internally_tagged_variants() {
    assert_eq!(
        branches::<Internal>(),
        json!([{
            "title": "Record",
            "type": "object",
            "properties": {
                "kind": {"type": "string", "enum": ["Record"]},
                "value": {"type": "string"}
            },
            "required": ["kind", "value"]
        }])
    );
}

#[test]
fn as_applies_to_adjacently_tagged_variants() {
    assert_eq!(
        branches::<Adjacent>(),
        json!([
            {
                "title": "Url",
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["Url"]},
                    "data": {"type": "string"}
                },
                "required": ["kind", "data"]
            },
            {
                "title": "Record",
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["Record"]},
                    "data": {
                        "type": "object",
                        "properties": {"value": {"type": "string"}},
                        "required": ["value"]
                    }
                },
                "required": ["kind", "data"]
            }
        ])
    );
}

#[test]
fn as_applies_to_untagged_variants() {
    assert_eq!(
        untagged_branches::<Untagged>(),
        json!([
            {"type": "string"},
            {
                "type": "array",
                "prefixItems": [{"type": "string"}, {"type": "string"}],
                "minItems": 2,
                "maxItems": 2
            },
            {
                "title": "Record",
                "type": "object",
                "properties": {"value": {"type": "string"}},
                "required": ["value"]
            }
        ])
    );
}
