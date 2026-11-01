//! Validates serde's real serialization against the schema the `Schematic` derive generates.
//!
//! Each sample value is serialized with serde and checked against the generated schema, with
//! component `$ref`s resolved exactly as the OpenAPI assembly resolves them (the type's schema
//! plus every collected component under `components/schemas`). Validation is strict: undeclared
//! properties are rejected through `unevaluatedProperties: false`, which the generated documents
//! do not set themselves.
//!
//! Add a case by defining a type and calling [`assert_serialization_matches`]. Known mismatches
//! are deliberately not asserted here; they are listed at the bottom of this file.

use std::collections::{BTreeMap, HashMap};

use gotcha::Schematic;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The schema document for `T`: the type's schema with the collected components it references.
///
/// `collect` registers derived types by Rust identity and returns `$ref`s into
/// `#/components/schemas`; embedding the components makes those references resolvable.
fn schema_document<T: Schematic>() -> Value {
    let (root, components) = gotcha::gotcha_core::registry::collect(|| T::generate_schema());
    let mut schema = root.schema.to_value();
    let schemas = components
        .into_iter()
        .map(|(name, schema)| (name, schema.to_value()))
        .collect::<serde_json::Map<_, _>>();
    schema
        .as_object_mut()
        .expect("a type's schema serializes to an object")
        .insert("components".into(), json!({ "schemas": schemas }));
    schema
}

/// Return `document` with unknown properties rejected for validation.
///
/// The generated schema leaves undeclared properties accepted, so a misspelled optional field
/// would validate even though serde would never write it under that name. Every declared-field
/// object schema gets `unevaluatedProperties: false`; schemas inside an `allOf` branch are left
/// alone because their properties are evaluated by the enclosing schema together with the
/// sibling branches. This changes the document only for validation, not what the derive emits.
fn strict(document: Value) -> Value {
    fn visit(value: &mut Value, in_conjunction: bool) {
        match value {
            Value::Object(object) => {
                let composes = object.contains_key("allOf");
                let declares_fields = object.contains_key("properties");
                if !in_conjunction && !object.contains_key("$ref") && (declares_fields || composes) && !object.contains_key("unevaluatedProperties") {
                    object.insert("unevaluatedProperties".into(), Value::Bool(false));
                }
                for (key, child) in object.iter_mut() {
                    match key.as_str() {
                        "properties" | "patternProperties" | "$defs" | "definitions" => {
                            if let Value::Object(schemas) = child {
                                for schema in schemas.values_mut() {
                                    visit(schema, false);
                                }
                            }
                        }
                        // A conjunction's branches are evaluated together by the enclosing schema.
                        "allOf" => {
                            if let Value::Array(branches) = child {
                                for branch in branches {
                                    visit(branch, true);
                                }
                            }
                        }
                        "anyOf" | "oneOf" => {
                            if let Value::Array(branches) = child {
                                for branch in branches {
                                    // Inside an `allOf` the enclosing schema evaluates the
                                    // branches together with the sibling fields.
                                    visit(branch, in_conjunction);
                                }
                            }
                        }
                        "items" | "additionalProperties" | "not" | "contains" | "propertyNames" | "if" | "then" | "else" => visit(child, false),
                        "components" => {
                            if let Some(schemas) = child.get_mut("schemas").and_then(Value::as_object_mut) {
                                for schema in schemas.values_mut() {
                                    visit(schema, false);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, in_conjunction);
                }
            }
            _ => {}
        }
    }
    let mut document = document;
    visit(&mut document, false);
    document
}

/// Serialize `value` with serde and assert the JSON validates against `T`'s generated schema.
///
/// Validation uses [`strict`], so an undeclared property fails even when the schema makes every
/// declared field optional.
#[track_caller]
fn assert_serialization_matches<T>(value: &T)
where
    T: Schematic + Serialize,
{
    let document = strict(schema_document::<T>());
    let instance = serde_json::to_value(value).expect("sample value serializes");
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    if let Err(error) = validator.validate(&instance) {
        panic!("serde output does not match the generated schema: {error}\nsample: {instance}\nschema: {document}");
    }
}

/// `rename`, `rename_all`, `default`, `skip_serializing_if` and `flatten` on a struct.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    #[serde(rename = "user-id")]
    user_id: u32,
    #[serde(default)]
    retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    nickname: Option<String>,
    #[serde(flatten)]
    audit: Audit,
}

#[derive(Schematic, Serialize, Deserialize)]
struct Audit {
    created_by: String,
    #[serde(default)]
    version: u32,
}

/// A container-level `default` makes every field optional in the schema; serde still writes them.
#[derive(Schematic, Serialize, Deserialize, Default)]
#[serde(default)]
struct ContainerDefault {
    name: String,
    count: u32,
}

/// A newtype struct is transparent to its inner type.
#[derive(Schematic, Serialize, Deserialize)]
struct UserId(u32);

/// `Option`, `Vec`, `HashMap` and `BTreeMap` fields.
#[derive(Schematic, Serialize, Deserialize)]
struct Collections {
    optional: Option<String>,
    items: Vec<u32>,
    map: HashMap<String, u32>,
    tree: BTreeMap<String, bool>,
    nested: Option<Vec<Option<String>>>,
}

/// Nested derived types are referenced through `components/schemas`.
#[derive(Schematic, Serialize, Deserialize)]
struct Wrapper {
    profile: Profile,
    ids: Vec<UserId>,
}

/// Structs that flatten a tagged enum: the variant's fields sit next to the struct's own.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum InternallyTagged {
    A { x: u32 },
    B { y: String },
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum AdjacentlyTagged {
    A(u32),
    B { y: String },
}

#[derive(Schematic, Serialize, Deserialize)]
enum ExternallyTagged {
    A(u32),
    B { y: String },
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensInternallyTagged {
    id: u32,
    #[serde(flatten)]
    kind: InternallyTagged,
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensAdjacentlyTagged {
    id: u32,
    #[serde(flatten)]
    kind: AdjacentlyTagged,
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensExternallyTagged {
    id: u32,
    #[serde(flatten)]
    kind: ExternallyTagged,
}

/// An all-unit enum: the wire form is a plain string.
#[derive(Schematic, Serialize, Deserialize)]
enum SimpleEnum {
    Alpha,
    Beta,
}

/// The default (externally tagged) representation, without a unit variant.
#[derive(Schematic, Serialize, Deserialize)]
enum External {
    Number(u32),
    Named { id: u32, score: Option<f64> },
}

/// Internally tagged, with a unit variant next to a struct variant.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum Internal {
    Unit,
    Created { id: u32, active: bool },
}

/// Adjacently tagged, with every variant shape.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum Adjacent {
    Count(u32),
    Named { id: u32 },
    Unit,
}

/// Untagged newtype variants.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum Untagged {
    Number(u32),
    Text(String),
}

#[test]
fn structs_match_serde() {
    let audit = || Audit {
        created_by: "system".into(),
        version: 3,
    };
    assert_serialization_matches(&Profile {
        user_id: 1,
        retry_count: 2,
        nickname: Some("ada".into()),
        audit: audit(),
    });
    // The optional field is omitted, and `default` fields are still written.
    assert_serialization_matches(&Profile {
        user_id: 1,
        retry_count: 2,
        nickname: None,
        audit: audit(),
    });
    assert_serialization_matches(&ContainerDefault::default());
    assert_serialization_matches(&ContainerDefault {
        name: "example".into(),
        count: 2,
    });
}

#[test]
fn strict_validation_rejects_undeclared_properties() {
    // `Profile` documents `nickname` as optional; a misspelled key is not silently accepted.
    let mut instance = serde_json::to_value(Profile {
        user_id: 1,
        retry_count: 2,
        nickname: Some("ada".into()),
        audit: Audit {
            created_by: "system".into(),
            version: 3,
        },
    })
    .expect("sample serializes");
    instance["nickName"] = json!("ada");

    let document = strict(schema_document::<Profile>());
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    assert!(validator.validate(&instance).is_err(), "strict validation must reject an undeclared property");
}

#[test]
fn newtypes_match_serde() {
    assert_serialization_matches(&UserId(7));
}

#[test]
fn collections_match_serde() {
    assert_serialization_matches(&Collections {
        optional: Some("x".into()),
        items: vec![1, 2],
        map: HashMap::from([("a".to_owned(), 1)]),
        tree: BTreeMap::from([("b".to_owned(), true)]),
        nested: Some(vec![Some("y".into()), None]),
    });
    assert_serialization_matches(&Collections {
        optional: None,
        items: vec![],
        map: HashMap::new(),
        tree: BTreeMap::new(),
        nested: None,
    });
}

#[test]
fn enums_match_serde() {
    // Externally tagged (the default) without a unit variant.
    assert_serialization_matches(&External::Number(1));
    assert_serialization_matches(&External::Named { id: 2, score: Some(1.5) });
    assert_serialization_matches(&External::Named { id: 2, score: None });

    // All-unit enums serialize as strings.
    assert_serialization_matches(&SimpleEnum::Alpha);

    // Internally tagged, including a unit variant.
    assert_serialization_matches(&Internal::Unit);
    assert_serialization_matches(&Internal::Created { id: 1, active: true });

    // Adjacently tagged, every shape.
    assert_serialization_matches(&Adjacent::Count(1));
    assert_serialization_matches(&Adjacent::Named { id: 1 });
    assert_serialization_matches(&Adjacent::Unit);

    // Untagged.
    assert_serialization_matches(&Untagged::Number(1));
    assert_serialization_matches(&Untagged::Text("x".into()));
}

#[test]
fn flattened_tagged_enums_match_serde() {
    assert_serialization_matches(&FlattensInternallyTagged {
        id: 1,
        kind: InternallyTagged::A { x: 2 },
    });
    assert_serialization_matches(&FlattensInternallyTagged {
        id: 1,
        kind: InternallyTagged::B { y: "z".into() },
    });
    assert_serialization_matches(&FlattensAdjacentlyTagged {
        id: 1,
        kind: AdjacentlyTagged::A(2),
    });
    assert_serialization_matches(&FlattensAdjacentlyTagged {
        id: 1,
        kind: AdjacentlyTagged::B { y: "z".into() },
    });
    assert_serialization_matches(&FlattensExternallyTagged {
        id: 1,
        kind: ExternallyTagged::A(2),
    });
    assert_serialization_matches(&FlattensExternallyTagged {
        id: 1,
        kind: ExternallyTagged::B { y: "z".into() },
    });
}

#[test]
fn component_references_resolve() {
    let wrapper = Wrapper {
        profile: Profile {
            user_id: 1,
            retry_count: 0,
            nickname: None,
            audit: Audit {
                created_by: "x".into(),
                version: 1,
            },
        },
        ids: vec![UserId(1), UserId(2)],
    };
    assert_serialization_matches(&wrapper);

    // The document is a schema for the root type with components in place, so the `$ref`s inside
    // it are resolvable rather than dangling.
    let document = schema_document::<Wrapper>();
    let rendered = serde_json::to_string(&document).expect("schema document serializes");
    assert!(rendered.contains("#/components/schemas/"), "expected component references: {rendered}");
    assert!(document["components"]["schemas"].as_object().is_some_and(|schemas| !schemas.is_empty()));
}

// Known mismatches, deliberately not asserted (follow-up work; arch review A4/A14):
//
// - A unit variant in a mixed externally tagged enum: the schema documents `{"Unit": {}}`, while
//   serde writes `"Unit"`.
// - An all-unit internally tagged, adjacently tagged or untagged enum: the schema documents a
//   string enum, while serde writes a tagged object (or `null` when untagged).
// - Container-level `rename_all` is applied to untagged variant fields by the schema, but serde
//   uses it for variant names (which untagged serialization does not write).
// - Struct-variant fields ignore `#[serde(skip)]`, `#[serde(flatten)]` and
//   `#[serde(rename_all_fields = "..")]`.
// - `#[serde(transparent)]` types are documented as objects with the inner field as a property.
// - A multi-field tuple variant in an externally or adjacently tagged enum is documented as an
//   empty object, while serde writes an array.
// - A unit variant in a mixed untagged enum is documented as an object, while serde writes null.
// - Untagged enums use `oneOf`, so a value matching several branches fails validation (`u32|f64`
//   with `1`). Struct variants whose fields are subsets only pass because strict validation
//   rejects the superset branch's extra fields; the schema itself still overlaps.
// - `#[serde(flatten)]` on an `Option<Struct>` drops the flattened fields from the schema, so
//   serialized values carry undeclared properties (arch review A4). Strict validation catches it.
// - A `#[serde(flatten)]` field whose type is a map (for example `HashMap<String, String>`) is
//   dropped from the schema; serde merges its entries into the containing object.
// - An internally tagged enum with a newtype variant wrapping a map documents only the tag; serde
//   merges the map's entries next to the tag.
