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

#[derive(Schematic, Serialize, Deserialize)]
struct Paging {
    page: u32,
    per_page: u32,
}

/// A whole flattened struct may be absent; serde then writes none of its fields.
#[derive(Schematic, Serialize, Deserialize)]
struct OptionalPaging {
    query: String,
    #[serde(flatten)]
    paging: Option<Paging>,
}

/// The same for a flattened tagged enum: absence means no tag and no variant fields.
#[derive(Schematic, Serialize, Deserialize)]
struct OptionalKind {
    query: String,
    #[serde(flatten)]
    kind: Option<InternallyTagged>,
}

/// Unlike `flatten`, a newtype variant still requires the fields of the struct it wraps, even
/// when that struct is inside an `Option`.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "t")]
enum OptionalNewtypeVariant {
    A(Option<Paging>),
}

/// Externally tagged, mixing a unit variant with other shapes.
#[derive(Schematic, Serialize, Deserialize)]
enum ExternalWithUnit {
    Unit,
    Number(u32),
}

/// An all-unit enum with each tag form.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum InternalUnitOnly {
    Unit,
    Other,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum AdjacentUnitOnly {
    Unit,
    Other,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum UntaggedUnitOnly {
    Unit,
}

/// Untagged, mixing a unit variant with a newtype variant.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum UntaggedWithUnit {
    Unit,
    Number(u32),
}

/// Zero-field struct and tuple variants are not unit variants: serde keeps the external
/// object/array wrappers and the untagged object/array bodies. A non-empty variant keeps these
/// enums out of the all-unit string-enum dispatch (an all-zero-field enum is a known mismatch).
#[derive(Schematic, Serialize, Deserialize)]
enum ZeroFieldExternal {
    Named {},
    Tuple(),
    Other(u32),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum ZeroFieldUntagged {
    Named {},
    Tuple(),
    Other(u32),
}

/// Tuple variants serialize as arrays of their fields, including zero-field `V()`.
#[derive(Schematic, Serialize, Deserialize)]
enum ExternalTuple {
    Pair(u8, String),
    Named { id: u32 },
    Empty {},
    Tuple(),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum AdjacentTuple {
    Pair(u8, String),
    Named { id: u32 },
    Empty {},
    Tuple(),
}

/// `#[serde(transparent)]` named structs are their single inner field on the wire.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(transparent)]
struct TransparentId {
    value: u32,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(transparent)]
struct TransparentAudit {
    inner: Audit,
}

/// serde allows extra `#[serde(skip)]` fields on a transparent struct (the typed-ID pattern).
#[derive(Schematic, Serialize, Deserialize)]
#[serde(transparent)]
struct TransparentTypedId<T> {
    value: u32,
    #[serde(skip)]
    marker: std::marker::PhantomData<T>,
}

/// A type flattened into a struct variant.
#[derive(Schematic, Serialize, Deserialize)]
struct VariantExtra {
    extra: u32,
    #[serde(default)]
    optional_extra: Option<String>,
}

/// Struct-variant fields honour skip/skip_serializing/flatten, the variant's `rename_all` and
/// the container's `rename_all_fields`, in every tag form.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
enum VariantFieldsInternal {
    Created {
        some_field: u32,
        #[serde(skip)]
        hidden: String,
        #[serde(skip_serializing)]
        output_only_hidden: String,
        #[serde(flatten)]
        extra: VariantExtra,
    },
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    Updated { some_field: u32 },
    #[serde(skip_deserializing)]
    OutputOnly { value: u32 },
}

#[derive(Schematic, Serialize, Deserialize)]
enum VariantFieldsExternal {
    Named {
        #[serde(skip)]
        hidden: String,
        some_field: u32,
        #[serde(flatten)]
        extra: VariantExtra,
    },
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum VariantFieldsAdjacent {
    Named {
        #[serde(skip)]
        hidden: String,
        some_field: u32,
        #[serde(flatten)]
        extra: VariantExtra,
    },
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum VariantFieldsUntagged {
    Named {
        #[serde(skip)]
        hidden: String,
        some_field: u32,
        #[serde(flatten)]
        extra: VariantExtra,
    },
}

/// A flattened map adds `additionalProperties` to the containing object.
#[derive(Schematic, Serialize, Deserialize)]
struct FlattenMap {
    id: u32,
    #[serde(flatten)]
    extra: HashMap<String, String>,
}

/// An internally tagged newtype variant wrapping a map merges the entries next to the tag.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum TaggedNewtypeMap {
    V(HashMap<String, u32>),
    Named { id: u32 },
}

/// A flattened enum and a flattened map in one struct: the map must not constrain the parent's
/// own fields, so it becomes `unevaluatedProperties` next to the `allOf`.
#[derive(Schematic, Serialize, Deserialize)]
struct FlattenEnumAndMap {
    id: u32,
    #[serde(flatten)]
    kind: InternallyTagged,
    #[serde(flatten)]
    extra: HashMap<String, String>,
}

/// A map may be flattened through `Option` or an intermediate struct.
#[derive(Schematic, Serialize, Deserialize)]
struct FlattenOptionalMap {
    id: u32,
    #[serde(flatten)]
    extra: Option<HashMap<String, String>>,
}

#[derive(Schematic, Serialize, Deserialize)]
struct MapHolder {
    #[serde(flatten)]
    extra: HashMap<String, String>,
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattenIntermediateMap {
    id: u32,
    #[serde(flatten)]
    inner: MapHolder,
}

/// An empty flattened struct contributes nothing and must not register a component.
#[derive(Schematic, Serialize, Deserialize)]
struct EmptyFlat {}

#[derive(Schematic, Serialize, Deserialize)]
struct WithEmptyFlat {
    id: u32,
    #[serde(flatten)]
    empty: EmptyFlat,
}

/// A named map newtype forwards the map's value schema without registering the wrapper.
#[derive(Schematic, Serialize, Deserialize)]
#[schematic(name = "NamedMapWrap")]
struct NamedMapWrap(HashMap<String, String>);

#[derive(Schematic, Serialize, Deserialize)]
struct WithNamedMapWrap {
    id: u32,
    #[serde(flatten)]
    map: NamedMapWrap,
}

/// Flattening an optional map of a derived value must reference that value, not orphan it.
#[derive(Schematic, Serialize, Deserialize)]
struct FlattenOptionalAuditMap {
    id: u32,
    #[serde(flatten)]
    extra: Option<HashMap<String, Audit>>,
}

/// A struct flattening an internally tagged enum whose newtype variant wraps a map: the map's
/// `additionalProperties` is stripped from the composed branch so it cannot constrain `name`.
#[derive(Schematic, Serialize, Deserialize)]
struct FlattensTaggedMap {
    name: String,
    #[serde(flatten)]
    map: TaggedNewtypeMap,
}

/// A map flattened into a struct variant contributes `additionalProperties` to the branch.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum VariantWithMap {
    Mapped {
        id: u32,
        #[serde(flatten)]
        extra: HashMap<String, String>,
    },
}

/// An untagged struct variant flattening a map: flattening the enum must strip the branch's
/// `additionalProperties`, or it would constrain the parent's own `name` field.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum UntaggedVariantWithMap {
    Mapped {
        id: u32,
        #[serde(flatten)]
        extra: HashMap<String, u32>,
    },
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensUntaggedMapVariant {
    name: String,
    #[serde(flatten)]
    variant: UntaggedVariantWithMap,
}

/// A different type with the same name as [`Audit`], to make component names collide.
mod other {
    use gotcha::Schematic;
    use serde::{Deserialize, Serialize};

    #[derive(Schematic, Serialize, Deserialize)]
    pub struct Audit {
        pub source: String,
    }
}

/// An internally tagged newtype variant over a map of the local [`Audit`].
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "t")]
enum MapOfAudit {
    W(BTreeMap<String, Audit>),
}

/// Flattening `MapOfAudit` next to a field of the same-named `other::Audit` must not collect the
/// map's value type: the flattened branch is stripped, so that component would be an orphan and
/// its name would collide with `other::Audit`, renaming both.
#[derive(Schematic, Serialize, Deserialize)]
struct Outer {
    audit: other::Audit,
    #[serde(flatten)]
    m: MapOfAudit,
}

/// Untagged branches that accept the same value: `anyOf` accepts it, `oneOf` did not.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum Overlapping {
    Integer(u32),
    Number(f64),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum SubsetVariants {
    A { x: u32 },
    B { x: u32, y: u32 },
}

/// Flattening the subset enum is the case that fails on main: the value matches both branches
/// (the extra field is allowed without sealing), which `oneOf` rejects.
#[derive(Schematic, Serialize, Deserialize)]
struct FlattenedSubset {
    id: u32,
    #[serde(flatten)]
    u: SubsetVariants,
}

/// Several unit variants all serialize as null; `anyOf` accepts them.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum MultiUnit {
    A,
    B,
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

/// Untagged with a container `rename_all`: serde applies it to variant names, which untagged
/// serialization does not write, so the fields keep their Rust names.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
enum UntaggedRenamed {
    Named { some_field: u32 },
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
    assert_serialization_matches(&TransparentId { value: 7 });
    assert_serialization_matches(&TransparentAudit {
        inner: Audit {
            created_by: "system".into(),
            version: 3,
        },
    });
    assert_serialization_matches(&TransparentTypedId::<()> {
        value: 7,
        marker: std::marker::PhantomData,
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
    assert_serialization_matches(&UntaggedRenamed::Named { some_field: 1 });

    // Zero-field struct variants are not units: the external wrapper and untagged body stay
    // objects. (The zero-field tuple variant of the same enum is a known mismatch below.)
    assert_serialization_matches(&ZeroFieldExternal::Named {});
    assert_serialization_matches(&ZeroFieldUntagged::Named {});
    // A zero-field tuple variant is an empty array in the external and untagged forms.
    assert_serialization_matches(&ZeroFieldExternal::Tuple {});
    assert_serialization_matches(&ZeroFieldUntagged::Tuple {});

    // Unit variants follow the tag form: a string in the default representation, an object with
    // just the tag when tagged, and null when untagged.
    assert_serialization_matches(&ExternalWithUnit::Unit);
    assert_serialization_matches(&ExternalWithUnit::Number(1));
    assert_serialization_matches(&InternalUnitOnly::Unit);
    assert_serialization_matches(&InternalUnitOnly::Other);
    assert_serialization_matches(&AdjacentUnitOnly::Unit);
    assert_serialization_matches(&AdjacentUnitOnly::Other);
    assert_serialization_matches(&UntaggedUnitOnly::Unit);
    assert_serialization_matches(&UntaggedWithUnit::Unit);
    assert_serialization_matches(&UntaggedWithUnit::Number(1));

    // Several branches accept the same value; serde picks the first match, so the schema must
    // accept any branch rather than require exactly one.
    assert_serialization_matches(&Overlapping::Integer(1));
    assert_serialization_matches(&Overlapping::Number(1.5));
    assert_serialization_matches(&FlattenedSubset {
        id: 1,
        u: SubsetVariants::B { x: 2, y: 3 },
    });
    assert_serialization_matches(&MultiUnit::A);
    assert_serialization_matches(&MultiUnit::B);
}

#[test]
fn tuple_variants_match_serde() {
    assert_serialization_matches(&ExternalTuple::Pair(1, "x".into()));
    assert_serialization_matches(&ExternalTuple::Named { id: 2 });
    assert_serialization_matches(&ExternalTuple::Empty {});
    assert_serialization_matches(&ExternalTuple::Tuple {});
    assert_serialization_matches(&AdjacentTuple::Pair(1, "x".into()));
    assert_serialization_matches(&AdjacentTuple::Named { id: 2 });
    assert_serialization_matches(&AdjacentTuple::Empty {});
    assert_serialization_matches(&AdjacentTuple::Tuple {});
}

#[test]
fn struct_variant_fields_match_serde() {
    let extra = || VariantExtra {
        extra: 2,
        optional_extra: Some("z".into()),
    };
    assert_serialization_matches(&VariantFieldsInternal::Created {
        some_field: 1,
        hidden: "hidden".into(),
        output_only_hidden: "output".into(),
        extra: extra(),
    });
    assert_serialization_matches(&VariantFieldsInternal::Updated { some_field: 3 });
    // `skip_deserializing` is still written, so it stays in the schema.
    assert_serialization_matches(&VariantFieldsInternal::OutputOnly { value: 4 });
    assert_serialization_matches(&VariantFieldsExternal::Named {
        hidden: "hidden".into(),
        some_field: 1,
        extra: extra(),
    });
    assert_serialization_matches(&VariantFieldsAdjacent::Named {
        hidden: "hidden".into(),
        some_field: 1,
        extra: extra(),
    });
    assert_serialization_matches(&VariantFieldsUntagged::Named {
        hidden: "hidden".into(),
        some_field: 1,
        extra: extra(),
    });
}

#[test]
fn flattened_maps_match_serde() {
    assert_serialization_matches(&FlattenMap {
        id: 1,
        extra: HashMap::from([("a".to_owned(), "b".to_owned())]),
    });
    assert_serialization_matches(&TaggedNewtypeMap::V(HashMap::from([("a".to_owned(), 2)])));
    assert_serialization_matches(&TaggedNewtypeMap::Named { id: 1 });
    assert_serialization_matches(&FlattenEnumAndMap {
        id: 1,
        kind: InternallyTagged::A { x: 2 },
        extra: HashMap::from([("a".to_owned(), "b".to_owned())]),
    });
    assert_serialization_matches(&FlattenOptionalMap {
        id: 1,
        extra: Some(HashMap::from([("a".to_owned(), "b".to_owned())])),
    });
    assert_serialization_matches(&FlattenOptionalMap { id: 1, extra: None });
    assert_serialization_matches(&FlattenIntermediateMap {
        id: 1,
        inner: MapHolder {
            extra: HashMap::from([("a".to_owned(), "b".to_owned())]),
        },
    });
    assert_serialization_matches(&WithEmptyFlat { id: 1, empty: EmptyFlat {} });
    assert_serialization_matches(&WithNamedMapWrap {
        id: 1,
        map: NamedMapWrap(HashMap::from([("a".to_owned(), "b".to_owned())])),
    });
    assert_serialization_matches(&FlattenOptionalAuditMap {
        id: 1,
        extra: Some(HashMap::from([(
            "a".to_owned(),
            Audit {
                created_by: "x".into(),
                version: 1,
            },
        )])),
    });
    assert_serialization_matches(&VariantWithMap::Mapped {
        id: 1,
        extra: HashMap::from([("a".to_owned(), "b".to_owned())]),
    });
}

#[test]
fn a_flattened_map_variant_does_not_constrain_the_parent() {
    // Loose validation on purpose: strict mode rejects the undocumented map entry, which is the
    // known limitation of flattening a map newtype variant.
    let document = schema_document::<FlattensTaggedMap>();
    let instance = serde_json::to_value(FlattensTaggedMap {
        name: "x".into(),
        map: TaggedNewtypeMap::V(HashMap::from([("a".to_owned(), 2)])),
    })
    .expect("sample serializes");
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    assert!(
        validator.validate(&instance).is_ok(),
        "the branch map constraint must not apply to the parent's fields: {instance} vs {document}"
    );
}

#[test]
fn a_flattened_untagged_map_variant_does_not_constrain_the_parent() {
    // Loose validation on purpose: strict mode rejects the undocumented map entry, which is the
    // known limitation of flattening an untagged map variant.
    let document = schema_document::<FlattensUntaggedMapVariant>();
    let instance = serde_json::to_value(FlattensUntaggedMapVariant {
        name: "x".into(),
        variant: UntaggedVariantWithMap::Mapped {
            id: 1,
            extra: HashMap::from([("a".to_owned(), 2)]),
        },
    })
    .expect("sample serializes");
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    assert!(
        validator.validate(&instance).is_ok(),
        "the untagged branch map constraint must not apply to the parent's fields: {instance} vs {document}"
    );
}

#[test]
fn a_flattened_map_variant_collects_no_component_for_its_value_type() {
    // The flattened branch's `additionalProperties` is stripped, so the map's value type is
    // never documented there. Building it anyway (the tagged newtype-variant path before #178's
    // `collect_additional` guard) registered an orphan component and, as here, forced both
    // same-named `Audit` types to be renamed. Without the flatten, only `other::Audit` and the
    // root are collected.
    let (_, components) = gotcha::gotcha_core::registry::collect(|| Outer::generate_schema());
    let names = components.keys().cloned().collect::<Vec<_>>();
    assert_eq!(names, ["Audit", "Outer"], "flattening must not collect the map's value type: {names:?}");

    let audit = components["Audit"].to_value();
    assert!(audit["properties"].get("source").is_some(), "the surviving `Audit` is `other::Audit`: {audit}");
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
fn optional_flattened_values_match_serde_with_and_without_the_value() {
    // Present: the flattened struct's fields are written next to the parent's.
    assert_serialization_matches(&OptionalPaging {
        query: "x".into(),
        paging: Some(Paging { page: 1, per_page: 20 }),
    });
    // Absent: serde writes only the parent's field, and the schema must accept that.
    assert_serialization_matches(&OptionalPaging {
        query: "x".into(),
        paging: None,
    });

    // Present: the flattened enum's tag and variant fields are written.
    assert_serialization_matches(&OptionalKind {
        query: "x".into(),
        kind: Some(InternallyTagged::A { x: 1 }),
    });
    // Absent: no tag and no variant fields.
    assert_serialization_matches(&OptionalKind { query: "x".into(), kind: None });
}

#[test]
fn a_newtype_variant_still_requires_the_option_fields_serde_requires() {
    // `Option` in a newtype variant is not a flatten field: serde deserializes the variant body
    // into `Option<Paging>` and fails when `page` is missing.
    let absent = json!({"t": "A"});
    assert!(serde_json::from_value::<OptionalNewtypeVariant>(absent.clone()).is_err());

    let document = strict(schema_document::<OptionalNewtypeVariant>());
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    assert!(
        validator.validate(&absent).is_err(),
        "the schema must require the fields serde requires: {document}"
    );

    // The value serde itself accepts carries the fields, and the schema must accept it too.
    // (serde cannot serialize an internally tagged newtype variant containing `Option`, so the
    // check is on the deserialized form.)
    let present = json!({"t": "A", "page": 1, "per_page": 20});
    assert!(serde_json::from_value::<OptionalNewtypeVariant>(present.clone()).is_ok());
    assert!(validator.validate(&present).is_ok(), "the schema must accept a value serde accepts: {document}");
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
// - A flattened externally tagged enum's unit variant: serde writes `{"A": null}` next to the
//   parent's fields, while the flattened schema documents `"A"`.
// - An externally tagged enum whose variants are all zero-field struct variants (`A {}`): it is
//   still routed to the string-enum shape (the dispatch checks emptiness, not `Style::Unit`).
// - `#[serde(skip)]` on a tuple-variant field: serde writes the remaining elements, while the
//   schema still requires one item per declared field.
// - Flattening an internally tagged enum whose newtype variant wraps a map omits the map's
//   `additionalProperties` from the composed schema: keeping it would constrain the parent's own
//   declared fields (the variant branch is composed through `allOf`), so strict validation
//   rejects the map entries.
// - Flattening an enum whose struct variant flattens a map (in any tag form) is stripped the
//   same way, for the same reason: the entries stay undocumented and strict validation rejects
//   them.
// - Flattening an untagged enum whose newtype variant wraps a map strips the branch's
//   `additionalProperties`; the entries stay undocumented and strict validation rejects them.
// - Flattening an untagged enum whose newtype variant wraps a struct that itself flattens a map
//   composes through that struct's `$ref`, so the map's `additionalProperties` still constrains
//   the parent's fields.
