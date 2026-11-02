//! Validates serde's real serialization against the schema the `Schematic` derive generates.
//!
//! Each sample value is serialized with serde and checked against the generated schema, with
//! component `$ref`s resolved exactly as the OpenAPI assembly resolves them (the type's schema
//! plus every collected component under `components/schemas`). Validation is strict: undeclared
//! properties are rejected through `unevaluatedProperties: false`, which the generated documents
//! do not set themselves.
//!
//! Add a case by defining a type and calling [`assert_serialization_matches`]. For the input
//! direction, [`assert_requests_are_read`] has serde read requests built from the schema alone.
//! Known mismatches are deliberately not asserted here; they are listed at the bottom of this file.

use std::collections::{BTreeMap, HashMap};

use gotcha::Schematic;
use serde::de::DeserializeOwned;
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

/// `T`'s own schema, looked up in the components when the document's root is a reference to it.
fn own_schema<T: Schematic>() -> Value {
    let document = schema_document::<T>();
    let Some(name) = document["$ref"].as_str().and_then(|reference| reference.strip_prefix("#/components/schemas/")) else {
        return document;
    };
    document["components"]["schemas"][name].clone()
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

/// Whether `instance` validates against `T`'s generated schema as it is, without [`strict`].
fn schema_accepts<T: Schematic>(instance: &Value) -> bool {
    let document = schema_document::<T>();
    jsonschema::validator_for(&document)
        .expect("generated schema is a valid JSON Schema")
        .validate(instance)
        .is_ok()
}

/// The requests a client could build from `T`'s schema alone: one per branch of a union, with just
/// the required properties and with every declared property.
fn requests_from_schema<T: Schematic>() -> Vec<Value> {
    let document = schema_document::<T>();
    let mut requests = build_requests(&document, &document, false);
    requests.extend(build_requests(&document, &document, true));
    requests
}

fn build_requests(schema: &Value, document: &Value, every_property: bool) -> Vec<Value> {
    if let Some(reference) = schema["$ref"].as_str() {
        let name = reference.strip_prefix("#/components/schemas/").expect("a finalized reference");
        return build_requests(&document["components"]["schemas"][name], document, every_property);
    }
    for union in ["oneOf", "anyOf"] {
        if let Some(branches) = schema[union].as_array() {
            return branches.iter().flat_map(|branch| build_requests(branch, document, every_property)).collect();
        }
    }
    if let Some(branches) = schema["allOf"].as_array() {
        let mut requests = vec![json!({})];
        for branch in branches {
            let options = build_requests(branch, document, every_property);
            requests = requests
                .iter()
                .flat_map(|request| {
                    options.iter().map(move |option| {
                        let mut merged = request.clone();
                        if let (Some(merged), Some(option)) = (merged.as_object_mut(), option.as_object()) {
                            merged.extend(option.clone());
                        }
                        merged
                    })
                })
                .collect();
        }
        return requests;
    }
    if let Some(value) = schema["enum"].get(0) {
        return vec![value.clone()];
    }
    let first = |schema: &Value| build_requests(schema, document, every_property).remove(0);
    let value = match schema["type"].as_str() {
        Some("object") => {
            let required = schema["required"].as_array().cloned().unwrap_or_default();
            let properties = schema["properties"].as_object().cloned().unwrap_or_default();
            let chosen = properties
                .iter()
                .filter(|(name, _)| every_property || required.iter().any(|entry| entry == *name));
            Value::Object(chosen.map(|(name, property)| (name.clone(), first(property))).collect())
        }
        Some("array") => match schema["prefixItems"].as_array() {
            Some(items) => Value::Array(items.iter().map(first).collect()),
            None => json!([first(&schema["items"])]),
        },
        Some("string") => json!("text"),
        Some("integer") => json!(1),
        Some("number") => json!(1.5),
        Some("boolean") => json!(true),
        _ => Value::Null,
    };
    vec![value]
}

/// Assert serde reads every request built from `T`'s schema: the schema also documents the input.
/// Requests are read from text, as a server reads them.
#[track_caller]
fn assert_requests_are_read<T>()
where
    T: Schematic + DeserializeOwned,
{
    let requests = requests_from_schema::<T>();
    assert!(!requests.is_empty());
    for request in requests {
        if let Err(error) = serde_json::from_str::<T>(&request.to_string()) {
            panic!(
                "serde rejects a request built from the schema: {error}\nrequest: {request}\nschema: {}",
                schema_document::<T>()
            );
        }
    }
}

/// Assert serde and `T`'s schema both accept `instance`, which is how the input looks when one
/// direction of a type is not what the schema documents.
#[track_caller]
fn assert_request_accepted<T>(instance: Value)
where
    T: Schematic + DeserializeOwned,
{
    if let Err(error) = serde_json::from_str::<T>(&instance.to_string()) {
        panic!("serde rejects {instance}: {error}");
    }
    assert!(
        schema_accepts::<T>(&instance),
        "the generated schema rejects {instance}: {}",
        schema_document::<T>()
    );
}

/// Assert serde and `T`'s generated schema both reject `instance`.
///
/// Validation is lenient: a variant left out must be rejected by the document as it is.
#[track_caller]
fn assert_rejected<T>(instance: Value)
where
    T: Schematic + DeserializeOwned,
{
    assert!(serde_json::from_str::<T>(&instance.to_string()).is_err(), "serde accepts {instance}");
    let document = schema_document::<T>();
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    assert!(validator.validate(&instance).is_err(), "the generated schema accepts {instance}: {document}");
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

/// Flattening the enum must strip the branch's `additionalProperties`, or it would constrain the
/// parent's own `name` field.
#[derive(Schematic, Serialize, Deserialize)]
struct FlattensTaggedStructMap {
    name: u32,
    #[serde(flatten)]
    variant: VariantWithMap,
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
fn a_flattened_tagged_struct_map_variant_does_not_constrain_the_parent() {
    // Loose validation on purpose: strict mode rejects the undocumented map entry, which is the
    // known limitation of flattening a struct-variant map.
    let document = schema_document::<FlattensTaggedStructMap>();
    let instance = serde_json::to_value(FlattensTaggedStructMap {
        name: 7,
        variant: VariantWithMap::Mapped {
            id: 1,
            extra: HashMap::from([("a".to_owned(), "b".to_owned())]),
        },
    })
    .expect("sample serializes");
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    assert!(
        validator.validate(&instance).is_ok(),
        "the tagged branch map constraint must not apply to the parent's fields: {instance} vs {document}"
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

/// Raw identifiers are written without their prefix.
#[derive(Schematic, Serialize, Deserialize)]
struct RawFields {
    r#type: String,
    r#match: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFieldsCamelCase {
    r#some_type: u8,
}

/// `snake_case` leaves field names alone, so `userID` does not collide with `user_id`.
#[allow(non_snake_case)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SnakeCaseAcronym {
    userID: u8,
    user_id: u8,
}

/// `camelCase` only joins on underscores: `userID` keeps its capitals.
#[allow(non_snake_case)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CamelCaseAcronym {
    userID: u8,
}

#[allow(non_snake_case)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SnakeCaseUnderscores {
    _leading: u8,
    trailing_: u8,
    double__underscore: u8,
}

/// `lowercase` and `UPPERCASE` keep the separators of a field name.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
struct LowercaseWords {
    user_name: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct UppercaseWords {
    user_name: u8,
}

/// Variant conventions insert a separator before every capital, so an acronym is not one word.
#[allow(clippy::upper_case_acronyms)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AcronymSimple {
    HTTPServer,
    Other,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AcronymExternal {
    HTTPServer(u8),
    Other,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum AcronymInternal {
    HTTPServer { value: u8 },
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
enum AcronymAdjacent {
    HTTPServer(u8),
}

#[allow(non_camel_case_types)]
#[derive(Schematic, Serialize, Deserialize)]
enum RawVariant {
    r#match,
    Other(u8),
}

/// Struct-variant fields take the same rules.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all_fields = "UPPERCASE")]
enum VariantFieldNames {
    Named { user_name: u8, r#type: u8 },
}

#[test]
fn raw_identifiers_lose_their_prefix() {
    assert_serialization_matches(&RawFields {
        r#type: "x".into(),
        r#match: 1,
    });
}

#[test]
fn raw_identifiers_lose_their_prefix_under_a_convention() {
    assert_serialization_matches(&RawFieldsCamelCase { r#some_type: 1 });
}

#[test]
fn snake_case_does_not_split_an_acronym_into_a_colliding_name() {
    assert_serialization_matches(&SnakeCaseAcronym { userID: 1, user_id: 2 });
}

#[test]
fn camel_case_keeps_the_capitals_of_a_field() {
    assert_serialization_matches(&CamelCaseAcronym { userID: 1 });
}

#[test]
fn snake_case_keeps_underscores() {
    assert_serialization_matches(&SnakeCaseUnderscores {
        _leading: 1,
        trailing_: 2,
        double__underscore: 3,
    });
}

#[test]
fn lowercase_keeps_separators() {
    assert_serialization_matches(&LowercaseWords { user_name: 1 });
}

#[test]
fn uppercase_keeps_separators() {
    assert_serialization_matches(&UppercaseWords { user_name: 1 });
}

#[test]
fn snake_case_variants_split_every_capital() {
    assert_serialization_matches(&AcronymSimple::HTTPServer);
}

#[test]
fn camel_case_variants_only_lower_the_first_letter() {
    assert_serialization_matches(&AcronymExternal::HTTPServer(1));
}

#[test]
fn kebab_case_variants_split_every_capital() {
    assert_serialization_matches(&AcronymInternal::HTTPServer { value: 1 });
}

#[test]
fn screaming_snake_case_variants_split_every_capital() {
    assert_serialization_matches(&AcronymAdjacent::HTTPServer(1));
}

#[test]
fn raw_variants_lose_their_prefix() {
    assert_serialization_matches(&RawVariant::r#match);
}

#[test]
fn struct_variant_fields_follow_the_same_rules() {
    assert_serialization_matches(&VariantFieldNames::Named { user_name: 1, r#type: 2 });
}

/// The same predicate written oddly: serde parses it to the same path, so the schema is the same.
#[derive(Schematic, Serialize, Deserialize)]
struct CanonicalSkipCondition {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u8>,
}

#[derive(Schematic, Serialize, Deserialize)]
struct SpacedSkipCondition {
    #[serde(skip_serializing_if = "Option ::is_none")]
    value: Option<u8>,
}

#[derive(Schematic, Serialize, Deserialize)]
struct SpacedStdSkipCondition {
    #[serde(skip_serializing_if = " std :: option :: Option :: is_none ")]
    value: Option<u8>,
}

#[derive(Schematic, Serialize, Deserialize)]
enum SpacedVariantSkipCondition {
    Named {
        #[serde(skip_serializing_if = "Option ::is_none")]
        value: Option<u8>,
    },
}

/// A `#[serde(skip)]` tuple-variant field takes no position in the array serde writes and reads.
#[derive(Schematic, Serialize, Deserialize)]
enum TupleSkipExternal {
    Skip(u8, #[serde(skip)] String),
    BothOneWaySkips(u8, #[serde(skip_serializing, skip_deserializing)] String),
    AllSkipped(#[serde(skip)] u8, #[serde(skip)] u8),
}

/// Skipped in one direction, a field keeps its position: the schema documents the other one.
#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
enum OneWayTupleSkip {
    ReadOnly(u8, #[serde(skip_serializing)] String),
    WrittenOnly(u8, #[serde(skip_deserializing)] String),
    NewtypeReadOnly(#[serde(skip_serializing)] u8),
    NewtypeWrittenOnly(#[serde(skip_deserializing)] u8),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum TupleSkipAdjacent {
    Skip(u8, #[serde(skip)] String),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum TupleSkipUntagged {
    Skip(u8, #[serde(skip)] String),
}

/// A newtype variant whose only field is skipped is written as a unit variant.
#[derive(Schematic, Serialize, Deserialize)]
enum NewtypeSkipExternal {
    Skip(#[serde(skip)] String),
    Other(u8),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum NewtypeSkipInternal {
    Skip(#[serde(skip)] Paging),
    Other { value: u8 },
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum NewtypeSkipAdjacent {
    Skip(#[serde(skip)] String),
    Other(u8),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum NewtypeSkipUntagged {
    Skip(#[serde(skip)] String),
    Other(u8),
}

impl Default for Paging {
    fn default() -> Self {
        Paging { page: 1, per_page: 20 }
    }
}

#[test]
fn a_spaced_skip_condition_is_the_same_predicate() {
    let canonical = own_schema::<CanonicalSkipCondition>();
    assert_eq!(canonical["properties"]["value"], json!({"type": "integer"}));
    assert_eq!(own_schema::<SpacedSkipCondition>()["properties"], canonical["properties"]);
}

#[test]
fn a_spaced_qualified_skip_condition_is_the_same_predicate() {
    let canonical = own_schema::<CanonicalSkipCondition>();
    assert_eq!(canonical["properties"]["value"], json!({"type": "integer"}));
    assert_eq!(own_schema::<SpacedStdSkipCondition>()["properties"], canonical["properties"]);
}

#[test]
fn a_spaced_skip_condition_on_a_variant_field_is_the_same_predicate() {
    let canonical = own_schema::<CanonicalSkipCondition>();
    assert_eq!(canonical["properties"]["value"], json!({"type": "integer"}));
    let schema = own_schema::<SpacedVariantSkipCondition>();
    assert_eq!(schema["oneOf"][0]["properties"]["Named"]["properties"], canonical["properties"]);
}

#[test]
fn a_skipped_tuple_field_takes_no_position() {
    assert_serialization_matches(&TupleSkipExternal::Skip(1, "x".into()));
}

#[test]
fn both_one_way_skips_on_a_tuple_field_take_no_position() {
    assert_serialization_matches(&TupleSkipExternal::BothOneWaySkips(1, "x".into()));
}

#[test]
fn the_input_of_a_skipped_tuple_field_is_what_serde_reads() {
    assert_requests_are_read::<TupleSkipExternal>();
    assert_requests_are_read::<TupleSkipAdjacent>();
    assert_requests_are_read::<TupleSkipUntagged>();
    assert_requests_are_read::<NewtypeSkipExternal>();
    assert_requests_are_read::<NewtypeSkipInternal>();
    assert_requests_are_read::<NewtypeSkipUntagged>();
}

#[test]
fn a_skip_serializing_tuple_field_keeps_the_position_serde_reads() {
    assert_request_accepted::<OneWayTupleSkip>(json!({"ReadOnly": [1, "x"]}));
}

#[test]
fn a_skip_serializing_newtype_field_keeps_the_value_serde_reads() {
    assert_request_accepted::<OneWayTupleSkip>(json!({"NewtypeReadOnly": 1}));
}

#[test]
fn a_skip_deserializing_newtype_field_keeps_the_value_serde_writes() {
    assert_serialization_matches(&OneWayTupleSkip::NewtypeWrittenOnly(1));
}

#[test]
fn a_tuple_variant_with_every_field_skipped_is_an_empty_array() {
    assert_serialization_matches(&TupleSkipExternal::AllSkipped(1, 2));
}

#[test]
fn a_skipped_adjacent_tuple_field_takes_no_position() {
    assert_serialization_matches(&TupleSkipAdjacent::Skip(1, "x".into()));
}

#[test]
fn a_skipped_untagged_tuple_field_takes_no_position() {
    assert_serialization_matches(&TupleSkipUntagged::Skip(1, "x".into()));
}

#[test]
fn a_skip_deserializing_tuple_field_keeps_the_position_serde_writes() {
    assert_serialization_matches(&OneWayTupleSkip::WrittenOnly(1, "x".into()));
}

#[test]
fn the_input_of_a_spaced_skip_condition_is_what_serde_reads() {
    assert_requests_are_read::<SpacedSkipCondition>();
    assert_requests_are_read::<SpacedVariantSkipCondition>();
}

#[test]
fn a_skipped_external_newtype_field_is_a_unit_variant() {
    assert_serialization_matches(&NewtypeSkipExternal::Skip("x".into()));
}

#[test]
fn a_skipped_internal_newtype_field_is_a_unit_variant() {
    assert_serialization_matches(&NewtypeSkipInternal::Skip(Paging::default()));
}

#[test]
fn a_skipped_adjacent_newtype_field_is_a_unit_variant() {
    assert_serialization_matches(&NewtypeSkipAdjacent::Skip("x".into()));
}

#[test]
fn a_skipped_untagged_newtype_field_is_a_unit_variant() {
    assert_serialization_matches(&NewtypeSkipUntagged::Skip("x".into()));
}

/// Both directions are renamed and the identifier is neither: the schema uses the name serde writes.
#[derive(Schematic, Serialize, Deserialize)]
struct RenameBothDirections {
    #[serde(rename(serialize = "written", deserialize = "read"))]
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
struct RenameSameBothDirections {
    #[serde(rename(serialize = "same", deserialize = "same"))]
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
struct ConventionBothDirections {
    some_field: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "camelCase"))]
struct ConventionSameBothDirections {
    some_field: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
enum VariantRenameBothDirections {
    #[serde(rename(serialize = "written", deserialize = "read"))]
    Unit,
    #[serde(rename(serialize = "written_newtype", deserialize = "read_newtype"))]
    Newtype(u8),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all(serialize = "kebab-case", deserialize = "SCREAMING_SNAKE_CASE"))]
enum VariantConventionBothDirections {
    SomeVariant,
    OtherVariant,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all_fields(serialize = "camelCase", deserialize = "kebab-case"))]
enum FieldConventionBothDirections {
    Named { some_field: u8 },
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum VariantOwnConventionBothDirections {
    #[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
    Named { some_field: u8 },
}

/// A one-sided rename leaves the name that is right for one direction.
#[derive(Schematic, Serialize, Deserialize)]
struct RenameSerializeOnly {
    #[serde(rename(serialize = "written"))]
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
struct RenameDeserializeOnly {
    #[serde(rename(deserialize = "read"))]
    value: u8,
}

/// The same when a two-sided form leaves one side on the identifier.
#[derive(Schematic, Serialize, Deserialize)]
struct RenameWrittenAndOwnName {
    #[serde(rename(serialize = "written", deserialize = "value"))]
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
struct ConventionSerializeOnly {
    some_field: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct ConventionDeserializeOnly {
    some_field: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
struct ConventionWrittenAndOwnName {
    some_field: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(rename_all(serialize = "kebab-case"))]
enum VariantConventionSerializeOnly {
    SomeVariant,
}

#[derive(Schematic, Serialize, Deserialize)]
enum VariantRenameSerializeOnly {
    #[serde(rename(serialize = "written"))]
    Unit,
}

#[test]
fn a_split_rename_names_the_schema_with_its_serialize_side() {
    assert_serialization_matches(&RenameBothDirections { value: 1 });
}

#[test]
fn a_list_form_rename_with_one_name_names_the_schema() {
    assert_serialization_matches(&RenameSameBothDirections { value: 1 });
}

#[test]
fn a_split_convention_names_the_schema_with_its_serialize_side() {
    assert_serialization_matches(&ConventionBothDirections { some_field: 1 });
}

#[test]
fn a_list_form_convention_with_one_rule_names_the_schema() {
    assert_serialization_matches(&ConventionSameBothDirections { some_field: 1 });
}

#[test]
fn a_split_variant_rename_names_a_unit_variant_with_its_serialize_side() {
    assert_serialization_matches(&VariantRenameBothDirections::Unit);
}

#[test]
fn a_split_variant_rename_names_a_newtype_variant_with_its_serialize_side() {
    assert_serialization_matches(&VariantRenameBothDirections::Newtype(1));
}

#[test]
fn a_split_variant_convention_names_the_variants_with_its_serialize_side() {
    assert_serialization_matches(&VariantConventionBothDirections::SomeVariant);
}

#[test]
fn a_split_field_convention_names_the_variant_fields_with_its_serialize_side() {
    assert_serialization_matches(&FieldConventionBothDirections::Named { some_field: 1 });
}

#[test]
fn a_variants_split_convention_names_its_fields_with_its_serialize_side() {
    assert_serialization_matches(&VariantOwnConventionBothDirections::Named { some_field: 1 });
}

#[test]
fn the_input_of_a_name_for_both_directions_is_what_serde_reads() {
    assert_requests_are_read::<RenameSameBothDirections>();
    assert_requests_are_read::<ConventionSameBothDirections>();
}

#[test]
fn a_one_sided_rename_keeps_the_input_serde_reads() {
    assert_requests_are_read::<RenameSerializeOnly>();
    assert_request_accepted::<RenameSerializeOnly>(json!({"value": 1}));
    assert_serialization_matches(&RenameDeserializeOnly { value: 1 });
}

#[test]
fn a_written_name_next_to_the_own_name_keeps_the_input_serde_reads() {
    assert_requests_are_read::<RenameWrittenAndOwnName>();
    assert_requests_are_read::<ConventionWrittenAndOwnName>();
}

#[test]
fn a_one_sided_convention_keeps_the_direction_it_leaves_alone() {
    assert_requests_are_read::<ConventionSerializeOnly>();
    assert_serialization_matches(&ConventionDeserializeOnly { some_field: 1 });
}

#[test]
fn a_one_sided_variant_name_keeps_the_input_serde_reads() {
    assert_requests_are_read::<VariantConventionSerializeOnly>();
    assert_requests_are_read::<VariantRenameSerializeOnly>();
}

/// `#[serde(skip)]` variants are neither written nor read.
#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
enum SkippedSimple {
    Kept,
    #[serde(skip)]
    Skipped,
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
enum SkippedExternal {
    Kept(u8),
    #[serde(skip)]
    Skipped(u8),
}

/// Without the skipped variant only unit variants are left, so this is a plain string enum.
#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
enum SkippedLeavesUnits {
    Kept,
    #[serde(skip)]
    Skipped(u8),
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum SkippedInternal {
    Kept {
        value: u8,
    },
    #[serde(skip)]
    Skipped {
        other: u8,
    },
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum SkippedAdjacent {
    Kept(u8),
    #[serde(skip)]
    Skipped(u8),
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum SkippedUntagged {
    Kept(u8),
    #[serde(skip)]
    Skipped(String),
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum AllSkippedInternal {
    #[serde(skip)]
    Skipped { other: u8 },
}

/// A variant skipped in one direction stays documented: the other direction uses that branch.
#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
enum SkippedOneDirection {
    #[serde(skip_serializing)]
    InputOnly(u8),
    #[serde(skip_deserializing)]
    OutputOnly(u8),
}

/// `#[serde(untagged)]` variants of a tagged enum are written without the tag.
#[derive(Schematic, Serialize, Deserialize)]
enum UntaggedVariantExternal {
    Tagged(u8),
    Unit,
    #[serde(untagged)]
    Newtype(String),
    #[serde(untagged)]
    Named {
        x: u8,
    },
    #[serde(untagged)]
    Tuple(u8, u8),
    #[serde(untagged)]
    Null,
}

#[derive(Schematic, Serialize, Deserialize)]
enum UntaggedVariantUnits {
    Tagged,
    #[serde(untagged)]
    Null,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum UntaggedVariantInternal {
    Tagged {
        value: u8,
    },
    Unit,
    #[serde(untagged)]
    Newtype(String),
    #[serde(untagged)]
    Named {
        x: u8,
    },
    #[serde(untagged)]
    Null,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum UntaggedVariantAdjacent {
    Tagged(u8),
    Unit,
    #[serde(untagged)]
    Newtype(String),
    #[serde(untagged)]
    Named {
        x: u8,
    },
    #[serde(untagged)]
    Null,
}

/// The tagged `Tagged` value also matches the untagged branch, so the branches are an `anyOf`.
#[derive(Schematic, Serialize, Deserialize)]
enum OverlappingUntaggedVariant {
    Tagged {
        value: u8,
    },
    #[serde(untagged)]
    Anything(HashMap<String, Value>),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum FlattenableUntaggedVariant {
    Tagged {
        value: u8,
    },
    #[serde(untagged)]
    Mapped {
        x: u8,
        #[serde(flatten)]
        extra: HashMap<String, u8>,
    },
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensUntaggedVariant {
    name: String,
    #[serde(flatten)]
    variant: FlattenableUntaggedVariant,
}

#[derive(Schematic, Serialize, Deserialize)]
enum FlattenableExternalUntaggedVariant {
    Tagged(u8),
    #[serde(untagged)]
    Mapped {
        x: u8,
        #[serde(flatten)]
        extra: HashMap<String, u8>,
    },
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensExternalUntaggedVariant {
    name: String,
    #[serde(flatten)]
    variant: FlattenableExternalUntaggedVariant,
}

/// `#[serde(other)]` writes its own name in every form; only the reader accepts any tag.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum OtherInternal {
    Known {
        value: u8,
    },
    #[serde(other)]
    Fallback,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum OtherAdjacent {
    Known(u8),
    #[serde(other)]
    Fallback,
}

/// A skipped variant leaves zero-field variants, which are not unit variants.
#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
enum ZeroFieldNextToSkipped {
    Named {},
    #[serde(skip)]
    Skipped(u8),
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
enum ZeroFieldTupleNextToSkipped {
    Tuple(),
    #[serde(skip)]
    Skipped(u8),
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
enum UnitAndZeroFieldNextToSkipped {
    Unit,
    Named {},
    #[serde(skip)]
    Skipped(u8),
}

/// A map's value type, and a type named like the map holder's own component.
#[derive(Schematic, Serialize, Deserialize)]
struct OnlyInAMap {
    inner: u8,
}

macro_rules! flattened_map_variant {
    ($enum_name:ident, $parent:ident, $($serde:tt)*) => {
        #[derive(Schematic, Serialize, Deserialize)]
        #[serde($($serde)*)]
        enum $enum_name {
            Tagged { value: u8 },
            #[serde(untagged)]
            Newtype(HashMap<String, OnlyInAMap>),
            #[serde(untagged)]
            Named {
                x: u8,
                #[serde(flatten)]
                extra: HashMap<String, OnlyInAMap>,
            },
        }

        #[derive(Schematic, Serialize, Deserialize)]
        struct $parent {
            id: u8,
            #[serde(flatten)]
            variant: $enum_name,
        }
    };
}

flattened_map_variant!(MapVariantInternal, FlattensMapVariantInternal, tag = "kind");
flattened_map_variant!(MapVariantAdjacent, FlattensMapVariantAdjacent, tag = "kind", content = "data");
flattened_map_variant!(MapVariantExternal, FlattensMapVariantExternal,);

/// The struct a variant wraps does not become a component, so it cannot collide with another type
/// of the same name.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum WrapsAuditWithoutTag {
    Tagged {
        value: u8,
    },
    #[serde(untagged)]
    Wrapped(Audit),
}

#[derive(Schematic, Serialize, Deserialize)]
struct HoldsOtherAudit {
    audit: other::Audit,
    wrapped: WrapsAuditWithoutTag,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum FlattenableAdjacentUntaggedVariant {
    Tagged(u8),
    #[serde(untagged)]
    Mapped {
        x: u8,
        #[serde(flatten)]
        extra: HashMap<String, u8>,
    },
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensAdjacentUntaggedVariant {
    name: String,
    #[serde(flatten)]
    variant: FlattenableAdjacentUntaggedVariant,
}

#[test]
fn a_zero_field_struct_variant_next_to_a_skipped_one_is_not_a_string() {
    assert_serialization_matches(&ZeroFieldNextToSkipped::Named {});
    assert_rejected::<ZeroFieldNextToSkipped>(json!("Named"));
    assert_requests_are_read::<ZeroFieldNextToSkipped>();
}

#[test]
fn a_zero_field_tuple_variant_next_to_a_skipped_one_is_not_a_string() {
    assert_serialization_matches(&ZeroFieldTupleNextToSkipped::Tuple());
    assert_rejected::<ZeroFieldTupleNextToSkipped>(json!("Tuple"));
    assert_requests_are_read::<ZeroFieldTupleNextToSkipped>();
}

#[test]
fn a_unit_and_a_zero_field_variant_next_to_a_skipped_one_are_not_strings() {
    assert_serialization_matches(&UnitAndZeroFieldNextToSkipped::Unit);
    assert_serialization_matches(&UnitAndZeroFieldNextToSkipped::Named {});
    assert_rejected::<UnitAndZeroFieldNextToSkipped>(json!("Named"));
    assert_requests_are_read::<UnitAndZeroFieldNextToSkipped>();
}

#[test]
fn flattening_a_map_variant_collects_no_component_for_its_value_type() {
    fn names<T: Schematic>() -> Vec<String> {
        let (_, components) = gotcha::gotcha_core::registry::collect(|| T::generate_schema());
        components.keys().cloned().collect()
    }
    assert_eq!(names::<FlattensMapVariantInternal>(), ["FlattensMapVariantInternal"]);
    assert_eq!(names::<FlattensMapVariantAdjacent>(), ["FlattensMapVariantAdjacent"]);
    assert_eq!(names::<FlattensMapVariantExternal>(), ["FlattensMapVariantExternal"]);
}

#[test]
fn a_wrapped_struct_does_not_rename_a_type_on_another_route() {
    let (_, components) = gotcha::gotcha_core::registry::collect(|| HoldsOtherAudit::generate_schema());
    let names = components.keys().cloned().collect::<Vec<_>>();
    assert_eq!(names, ["Audit", "HoldsOtherAudit", "WrapsAuditWithoutTag"]);
    assert!(components["Audit"].to_value()["properties"].get("source").is_some());
}

#[test]
fn a_wrapped_struct_matches_serde_without_a_component() {
    assert_serialization_matches(&WrapsAuditWithoutTag::Wrapped(Audit {
        created_by: "x".into(),
        version: 1,
    }));
    assert_requests_are_read::<WrapsAuditWithoutTag>();
}

/// `wrapped_value_fields_schema`: an empty struct and a non-struct value have no fields to merge.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum WrapsValueWithoutFields {
    Tagged {
        value: u8,
    },
    #[serde(untagged)]
    Text(String),
}

/// A value that may be absent is a reference, which makes `Audit` a component of the document.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum WrapsOptionalAudit {
    Tagged {
        value: u8,
    },
    #[serde(untagged)]
    Wrapped(Option<Audit>),
}

#[derive(Schematic, Serialize, Deserialize)]
struct HoldsOtherAuditNextToOptional {
    audit: other::Audit,
    wrapped: WrapsOptionalAudit,
}

#[derive(Schematic, Serialize, Deserialize)]
struct AuditFlatteningEnum {
    name: String,
    #[serde(flatten)]
    inner: InternallyTagged,
}

/// A value flattening an enum is a reference too, because its fields do not say everything.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum WrapsValueFlatteningEnum {
    Tagged {
        value: u8,
    },
    #[serde(untagged)]
    Wrapped(AuditFlatteningEnum),
}

#[derive(Schematic, Serialize, Deserialize)]
struct StructFlatteningMap {
    name: String,
    #[serde(flatten)]
    extra: HashMap<String, u8>,
}

/// The struct's map stays where nothing composes with the branch, and leaves when it is flattened.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum WrapsStructFlatteningMap {
    Tagged {
        value: u8,
    },
    #[serde(untagged)]
    Wrapped(StructFlatteningMap),
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensWrappedStructFlatteningMap {
    own: u8,
    #[serde(flatten)]
    inner: WrapsStructFlatteningMap,
}

/// The value type of a map that leaves when it is flattened is not registered either.
#[derive(Schematic, Serialize, Deserialize)]
struct WrappedMapValue {
    v: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
struct StructFlatteningMapOfStructs {
    name: String,
    #[serde(flatten)]
    extra: HashMap<String, WrappedMapValue>,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum WrapsStructFlatteningMapOfStructs {
    Tagged {
        value: u8,
    },
    #[serde(untagged)]
    Wrapped(StructFlatteningMapOfStructs),
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensWrappedStructFlatteningMapOfStructs {
    own: u8,
    #[serde(flatten)]
    inner: WrapsStructFlatteningMapOfStructs,
}

/// A value leading back to itself through a flattened enum: a probe must not recurse forever.
#[derive(Schematic, Serialize, Deserialize)]
struct RecursiveNode {
    next: Option<Box<RecursiveParent>>,
}

#[derive(Schematic, Serialize, Deserialize)]
struct RecursiveParent {
    #[serde(flatten)]
    branch: RecursiveBranch,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum RecursiveBranch {
    Node(RecursiveNode),
    Text(String),
}

// serde's `Serialize` for a type flattening itself does not compile: its serializer type grows.
macro_rules! self_flattening_enum {
    ($node:ident, $enum:ident, $parent:ident, $($attribute:tt)*) => {
        #[allow(dead_code)]
        #[derive(Schematic, Deserialize)]
        struct $node {
            value: u8,
            #[serde(flatten)]
            branch: Option<Box<$enum>>,
        }

        #[allow(dead_code)]
        #[derive(Schematic, Deserialize)]
        $($attribute)*
        enum $enum {
            Named { x: u8 },
            #[serde(untagged)]
            Wrapped($node),
        }

        #[allow(dead_code)]
        #[derive(Schematic, Deserialize)]
        struct $parent {
            own: u8,
            #[serde(flatten)]
            inner: $enum,
        }
    };
}

self_flattening_enum!(ExternalNode, ExternalSelfFlattening, FlattensExternalSelfFlattening,);
self_flattening_enum!(InternalNode, InternalSelfFlattening, FlattensInternalSelfFlattening, #[serde(tag = "kind")]);
self_flattening_enum!(AdjacentNode, AdjacentSelfFlattening, FlattensAdjacentSelfFlattening, #[serde(tag = "kind", content = "data")]);

#[allow(dead_code)]
#[derive(Schematic, Deserialize)]
struct UntaggedNode {
    value: u8,
    #[serde(flatten)]
    branch: Option<Box<UntaggedSelfFlattening>>,
}

#[allow(dead_code)]
#[derive(Schematic, Deserialize)]
#[serde(untagged)]
enum UntaggedSelfFlattening {
    Named { x: u8 },
    Wrapped(UntaggedNode),
}

#[allow(dead_code)]
#[derive(Schematic, Deserialize)]
struct FlattensUntaggedSelfFlattening {
    own: u8,
    #[serde(flatten)]
    inner: UntaggedSelfFlattening,
}

thread_local! {
    static BUILT_SCHEMAS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts how often its schema is built, which a time limit could not do without flaking under load.
#[derive(Serialize, Deserialize)]
struct CountsSchemas;

impl Schematic for CountsSchemas {
    fn name() -> &'static str {
        "CountsSchemas"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "null"
    }

    fn generate_schema() -> gotcha::EnhancedSchema {
        BUILT_SCHEMAS.set(BUILT_SCHEMAS.get() + 1);
        <() as Schematic>::generate_schema()
    }
}

/// An ordinary tree of nodes: every node of a flattened enum holds the type flattening it.
macro_rules! wide_recursion {
    ($expr:ident, $kind:ident, $($(#[$attribute:meta])* $variant:ident($node:ident)),* $(,)?) => {
        #[derive(Schematic, Serialize, Deserialize)]
        struct $expr {
            span: u8,
            #[serde(flatten)]
            kind: $kind,
        }

        #[derive(Schematic, Serialize, Deserialize)]
        #[serde(untagged)]
        enum $kind {
            $($variant($node),)*
        }

        $(
            #[derive(Schematic, Serialize, Deserialize)]
            $(#[$attribute])*
            struct $node {
                counted: CountsSchemas,
                next: Option<Box<$expr>>,
            }
        )*
    };
}

wide_recursion!(
    WideExpr,
    WideKind,
    A(WideA),
    B(WideB),
    C(WideC),
    D(WideD),
    E(WideE),
    F(WideF),
    G(WideG),
    H(WideH)
);

/// Looking into the same value twice below one flatten must not change the document.
#[derive(Schematic, Serialize, Deserialize)]
struct WrappedTwice {
    x: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "k")]
enum WrapsOneStructTwice {
    Named {
        y: u8,
    },
    #[serde(untagged)]
    First(WrappedTwice),
    #[serde(untagged)]
    Second(WrappedTwice),
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensWrapsOneStructTwice {
    own: u8,
    #[serde(flatten)]
    inner: WrapsOneStructTwice,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "w")]
enum WrapsFlattensWrapsOneStructTwice {
    Named {
        z: u8,
    },
    #[serde(untagged)]
    Wrapped(FlattensWrapsOneStructTwice),
}

/// A map panicking the first time it is looked into, to leave a probe through an unwind.
struct PanicsOnceInAProbe;

thread_local! {
    static PANICKED_IN_A_PROBE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Schematic for PanicsOnceInAProbe {
    fn name() -> &'static str {
        "PanicsOnceInAProbe"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn generate_schema() -> gotcha::EnhancedSchema {
        if !PANICKED_IN_A_PROBE.replace(true) {
            panic!("the first look into this value");
        }
        <HashMap<String, u8> as Schematic>::generate_schema()
    }
}

#[derive(Schematic, Serialize, Deserialize)]
struct ReferencedByHandWritten {
    v: u8,
}

/// A hand-written value with a root `additionalProperties` that inlines a reference, in each form
/// of an enum flattened into a parent: the reference must stay resolvable.
macro_rules! hand_written_reference {
    ($value:ident, $keyword:literal, $($enum:ident / $parent:ident [$($attribute:tt)*] $($variant_attribute:ident)?),* $(,)?) => {
        #[derive(Serialize, Deserialize)]
        struct $value {
            other: ReferencedByHandWritten,
        }

        impl Schematic for $value {
            fn name() -> &'static str {
                stringify!($value)
            }

            fn required() -> bool {
                true
            }

            fn type_() -> &'static str {
                "object"
            }

            fn generate_schema() -> gotcha::EnhancedSchema {
                let mut schema = gotcha::gotcha_core::oas::Schema {
                    _type: Some(Self::type_().into()),
                    ..gotcha::gotcha_core::oas::Schema::default()
                };
                schema
                    .extras
                    .insert("properties".to_owned(), json!({"other": ReferencedByHandWritten::generate_schema().schema.to_value()}));
                schema.extras.insert("additionalProperties".to_owned(), json!($keyword));
                gotcha::EnhancedSchema { schema, required: true }
            }
        }

        $(
            #[derive(Schematic, Serialize, Deserialize)]
            $($attribute)*
            enum $enum {
                Named {
                    x: u8,
                },
                #[serde(untagged)]
                Wrapped($value),
            }

            #[derive(Schematic, Serialize, Deserialize)]
            struct $parent {
                own: u8,
                #[serde(flatten)]
                inner: $enum,
            }
        )*
    };
}

hand_written_reference!(
    OpenWithReference,
    true,
    OpenExternal / FlattensOpenExternal[],
    OpenInternal / FlattensOpenInternal[#[serde(tag = "kind")]],
    OpenAdjacent / FlattensOpenAdjacent[#[serde(tag = "kind", content = "data")]],
);
hand_written_reference!(
    ClosedWithReference,
    false,
    ClosedExternal / FlattensClosedExternal[],
    ClosedInternal / FlattensClosedInternal[#[serde(tag = "kind")]],
    ClosedAdjacent / FlattensClosedAdjacent[#[serde(tag = "kind", content = "data")]],
);

#[derive(Serialize, Deserialize)]
struct UntaggedValueWithReference {
    other: ReferencedByHandWritten,
}

impl Schematic for UntaggedValueWithReference {
    fn name() -> &'static str {
        "UntaggedValueWithReference"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn generate_schema() -> gotcha::EnhancedSchema {
        OpenWithReference::generate_schema()
    }
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum UntaggedWithReference {
    Wrapped(UntaggedValueWithReference),
    Text(String),
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensUntaggedWithReference {
    own: u8,
    #[serde(flatten)]
    inner: UntaggedWithReference,
}

/// A hand-written value embedding another enum's flatten schema, which a probe stops at.
#[derive(Serialize, Deserialize)]
struct EmbedsAFlattenSchema {
    kind: String,
    x: u8,
}

impl Schematic for EmbedsAFlattenSchema {
    fn name() -> &'static str {
        "EmbedsAFlattenSchema"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn generate_schema() -> gotcha::EnhancedSchema {
        let mut schema = gotcha::gotcha_core::oas::Schema::default();
        let embedded = UntaggedWithReference::flatten_schema().expect("an enum flattens");
        schema.extras.insert("anyOf".to_owned(), json!([embedded]));
        schema.extras.insert("additionalProperties".to_owned(), json!(true));
        gotcha::EnhancedSchema { schema, required: true }
    }
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum WrapsEmbeddedFlattenSchema {
    Wrapped(EmbedsAFlattenSchema),
    Number(u8),
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensWrapsEmbeddedFlattenSchema {
    own: u8,
    #[serde(flatten)]
    inner: WrapsEmbeddedFlattenSchema,
}

/// A reference can also sit in the typed fields of a hand-written schema.
macro_rules! hand_written_typed_reference {
    ($value:ident, $enum:ident, $parent:ident, |$schema:ident, $reference:ident| $fill:expr) => {
        #[allow(dead_code)]
        #[derive(Serialize, Deserialize)]
        struct $value {
            v: u8,
        }

        impl Schematic for $value {
            fn name() -> &'static str {
                stringify!($value)
            }

            fn required() -> bool {
                true
            }

            fn type_() -> &'static str {
                "object"
            }

            fn generate_schema() -> gotcha::EnhancedSchema {
                let mut $schema = gotcha::gotcha_core::oas::Schema::default();
                let $reference = ReferencedByHandWritten::generate_schema().schema;
                $fill;
                $schema.extras.insert("additionalProperties".to_owned(), json!(true));
                gotcha::EnhancedSchema {
                    schema: $schema,
                    required: true,
                }
            }
        }

        #[derive(Schematic, Serialize, Deserialize)]
        #[serde(untagged)]
        enum $enum {
            Wrapped($value),
            Text(String),
        }

        #[derive(Schematic, Serialize, Deserialize)]
        struct $parent {
            own: u8,
            #[serde(flatten)]
            inner: $enum,
        }
    };
}

hand_written_typed_reference!(RootReference, RootReferenceEnum, FlattensRootReference, |schema, reference| schema._ref =
    reference._ref);
hand_written_typed_reference!(
    ContentSchemaReference,
    ContentSchemaReferenceEnum,
    FlattensContentSchemaReference,
    |schema, reference| schema.content_schema = Some(Box::new(reference.into()))
);

/// Every reference in `document` points at a component it holds.
#[track_caller]
fn assert_references_resolve(document: &Value) {
    fn visit(value: &Value, document: &Value) {
        match value {
            Value::Object(object) => {
                if let Some(name) = object
                    .get("$ref")
                    .and_then(Value::as_str)
                    .and_then(|reference| reference.strip_prefix("#/components/schemas/"))
                {
                    assert!(document["components"]["schemas"].get(name).is_some(), "{name} is not in {document}");
                }
                object.values().for_each(|child| visit(child, document));
            }
            Value::Array(items) => items.iter().for_each(|item| visit(item, document)),
            _ => {}
        }
    }
    visit(document, document);
}

#[test]
fn a_flattened_hand_written_value_keeps_the_components_it_refers_to() {
    assert_references_resolve(&schema_document::<FlattensOpenExternal>());
    assert_references_resolve(&schema_document::<FlattensOpenInternal>());
    assert_references_resolve(&schema_document::<FlattensOpenAdjacent>());
    assert_references_resolve(&schema_document::<FlattensClosedExternal>());
    assert_references_resolve(&schema_document::<FlattensClosedInternal>());
    assert_references_resolve(&schema_document::<FlattensClosedAdjacent>());
    assert_references_resolve(&schema_document::<FlattensUntaggedWithReference>());
    assert_references_resolve(&schema_document::<FlattensRootReference>());
    assert_references_resolve(&schema_document::<FlattensContentSchemaReference>());
    let output = json!({"own": 1, "other": {"v": 2}});
    assert!(schema_accepts::<FlattensUntaggedWithReference>(&output));
    assert!(
        schema_accepts::<FlattensClosedInternal>(&output),
        "{}",
        schema_document::<FlattensClosedInternal>()
    );
    assert!(
        schema_accepts::<FlattensClosedExternal>(&output),
        "{}",
        schema_document::<FlattensClosedExternal>()
    );
    assert!(
        schema_accepts::<FlattensClosedAdjacent>(&output),
        "{}",
        schema_document::<FlattensClosedAdjacent>()
    );
}

#[test]
fn a_flattened_hand_written_value_loses_its_root_keyword() {
    let schema = own_schema::<FlattensClosedExternal>();
    assert!(!schema.to_string().contains("additionalProperties"), "{schema}");
}

#[test]
fn a_flattened_hand_written_value_keeps_the_branches_it_embeds() {
    // The embedded branch documents an object with `other`, so it does not accept just anything.
    assert!(!schema_accepts::<FlattensWrapsEmbeddedFlattenSchema>(&json!({"own": 1, "other": 1})));
    assert!(schema_accepts::<FlattensWrapsEmbeddedFlattenSchema>(&json!({"own": 1, "other": {"v": 2}})));
    assert_references_resolve(&schema_document::<FlattensWrapsEmbeddedFlattenSchema>());
}

#[test]
fn wide_recursion_through_a_flattened_enum_builds_each_schema_a_bounded_number_of_times() {
    BUILT_SCHEMAS.set(0);
    let (_, components) = gotcha::gotcha_core::registry::collect(|| WideExpr::generate_schema());
    assert_eq!(components.len(), 9, "{:?}", components.keys());
    // One probe and one document per node, which is 16. Walking every path of nodes builds 876808,
    // which took a minute here and hours with twelve nodes.
    let built = BUILT_SCHEMAS.get();
    assert!(built <= 100, "the schema of a node was built {built} times");
}

#[test]
fn wide_recursion_through_a_flattened_enum_matches_serde() {
    // Loose validation on purpose: a branch is a reference, so strict validation rejects the
    // parent's fields there, as for the other flattened references.
    let output = serde_json::to_value(WideExpr {
        span: 1,
        kind: WideKind::C(WideC {
            counted: CountsSchemas,
            next: Some(Box::new(WideExpr {
                span: 2,
                kind: WideKind::H(WideH {
                    counted: CountsSchemas,
                    next: None,
                }),
            })),
        }),
    })
    .expect("sample serializes");
    assert!(schema_accepts::<WideExpr>(&output), "{output}");
}

#[test]
fn looking_into_a_value_twice_registers_no_component() {
    let (_, components) = gotcha::gotcha_core::registry::collect(|| WrapsFlattensWrapsOneStructTwice::generate_schema());
    let names = components.keys().cloned().collect::<Vec<_>>();
    assert_eq!(names, ["FlattensWrapsOneStructTwice", "WrapsFlattensWrapsOneStructTwice"]);
}

#[test]
fn a_probe_ends_with_an_unwind() {
    use gotcha::gotcha_core::registry::wrapped_value_schema;
    assert!(std::panic::catch_unwind(|| wrapped_value_schema::<PanicsOnceInAProbe>(true)).is_err());
    // Still probed: the map's keyword leaves, and the value is not skipped as part of a probe.
    let schema = wrapped_value_schema::<PanicsOnceInAProbe>(true).schema.to_value();
    assert_eq!(schema, json!({"type": "object"}));
}

#[test]
fn a_wrapped_value_without_fields_is_not_an_object() {
    assert_serialization_matches(&WrapsValueWithoutFields::Text("text".into()));
    assert_requests_are_read::<WrapsValueWithoutFields>();
}

#[test]
fn a_wrapped_value_that_may_be_absent_stays_a_reference() {
    assert_serialization_matches(&WrapsOptionalAudit::Wrapped(None));
    assert_serialization_matches(&WrapsOptionalAudit::Wrapped(Some(Audit {
        created_by: "x".into(),
        version: 1,
    })));
    assert_requests_are_read::<WrapsOptionalAudit>();
}

/// Disclosed in MIGRATION: a reference registers `Audit`, so a same-named type changes its name.
#[test]
fn a_wrapped_value_that_may_be_absent_renames_a_type_on_another_route() {
    let (_, components) = gotcha::gotcha_core::registry::collect(|| HoldsOtherAuditNextToOptional::generate_schema());
    let names = components.keys().cloned().collect::<Vec<_>>();
    assert_eq!(
        names,
        ["HoldsOtherAuditNextToOptional", "OtherAudit", "SchemaMatchesSerdeAudit", "WrapsOptionalAudit"]
    );
}

#[test]
fn a_wrapped_value_flattening_an_enum_keeps_the_enum() {
    assert_serialization_matches(&WrapsValueFlatteningEnum::Wrapped(AuditFlatteningEnum {
        name: "x".into(),
        inner: InternallyTagged::A { x: 1 },
    }));
    assert_requests_are_read::<WrapsValueFlatteningEnum>();
    let (_, components) = gotcha::gotcha_core::registry::collect(|| WrapsValueFlatteningEnum::generate_schema());
    assert!(components.contains_key("AuditFlatteningEnum"), "{:?}", components.keys());
}

#[test]
fn a_wrapped_struct_merges_its_fields_with_their_presence() {
    let schema = own_schema::<WrapsAuditWithoutTag>();
    let branch = &schema["anyOf"][1];
    assert_eq!(branch["type"], json!("object"), "{schema}");
    assert_eq!(branch["required"], json!(["created_by"]), "{schema}");
    assert_eq!(branch["properties"]["version"], json!({"type": "integer"}), "{schema}");
}

#[test]
fn a_wrapped_struct_keeps_its_map_unless_it_is_flattened() {
    let value = WrapsStructFlatteningMap::Wrapped(StructFlatteningMap {
        name: "x".into(),
        extra: HashMap::from([("a".to_owned(), 1)]),
    });
    assert_serialization_matches(&value);
    assert_eq!(
        own_schema::<WrapsStructFlatteningMap>()["anyOf"][1]["additionalProperties"],
        json!({"type": "integer"})
    );
    let schema = own_schema::<FlattensWrappedStructFlatteningMap>();
    assert_eq!(schema["allOf"][1]["anyOf"][1].get("additionalProperties"), None, "{schema}");
    assert_request_accepted::<FlattensWrappedStructFlatteningMap>(json!({"own": 1, "name": "x"}));
}

#[test]
fn flattening_a_wrapped_struct_collects_no_component_for_its_map_values() {
    let (_, components) = gotcha::gotcha_core::registry::collect(|| FlattensWrappedStructFlatteningMapOfStructs::generate_schema());
    assert_eq!(components.keys().cloned().collect::<Vec<_>>(), ["FlattensWrappedStructFlatteningMapOfStructs"]);
    let (_, components) = gotcha::gotcha_core::registry::collect(|| WrapsStructFlatteningMapOfStructs::generate_schema());
    assert!(components.contains_key("WrappedMapValue"), "{:?}", components.keys());
}

#[test]
fn a_value_leading_back_to_itself_through_a_flattened_enum_ends() {
    assert_serialization_matches(&RecursiveParent {
        branch: RecursiveBranch::Node(RecursiveNode {
            next: Some(Box::new(RecursiveParent {
                branch: RecursiveBranch::Node(RecursiveNode { next: None }),
            })),
        }),
    });
    assert!(schema_accepts::<RecursiveParent>(&json!({"next": null})));
    assert!(schema_accepts::<RecursiveNode>(&json!({"next": {"next": null}})));
}

#[test]
fn an_enum_flattened_by_the_value_it_wraps_ends_in_every_form() {
    fn accepts<T: Schematic>(instance: Value) {
        assert!(schema_accepts::<T>(&instance), "{instance}: {}", schema_document::<T>());
    }
    accepts::<ExternalSelfFlattening>(json!({"value": 1, "Named": {"x": 1}}));
    accepts::<InternalSelfFlattening>(json!({"value": 1, "kind": "Named", "x": 1}));
    accepts::<AdjacentSelfFlattening>(json!({"value": 1, "kind": "Named", "data": {"x": 1}}));
    accepts::<UntaggedSelfFlattening>(json!({"value": 1, "x": 1}));
    accepts::<FlattensExternalSelfFlattening>(json!({"own": 1, "Named": {"x": 1}}));
    accepts::<FlattensInternalSelfFlattening>(json!({"own": 1, "kind": "Named", "x": 1}));
    accepts::<FlattensAdjacentSelfFlattening>(json!({"own": 1, "kind": "Named", "data": {"x": 1}}));
    accepts::<FlattensUntaggedSelfFlattening>(json!({"own": 1, "x": 1}));
}

#[cfg(feature = "openapi")]
#[test]
fn parameters_of_a_value_leading_back_to_itself_end() {
    use gotcha::axum::extract::Query;
    use gotcha::{Json, ParameterProvider};
    fn generate<T: Schematic>() {
        let _ = gotcha::gotcha_core::registry::collect(|| {
            let _ = <Json<T> as ParameterProvider>::generate("/".into());
            let _ = <Query<T> as ParameterProvider>::generate("/".into());
        });
    }
    generate::<RecursiveParent>();
    generate::<RecursiveNode>();
    generate::<ExternalSelfFlattening>();
    generate::<InternalSelfFlattening>();
    generate::<AdjacentSelfFlattening>();
    generate::<UntaggedSelfFlattening>();
    generate::<InternalNode>();
}

#[test]
fn flattening_an_internal_enum_with_an_untagged_variant_has_no_discriminator() {
    let schema = own_schema::<FlattensUntaggedVariant>();
    let flattened = &schema["allOf"][1];
    assert!(flattened.get("anyOf").is_some() && flattened.get("discriminator").is_none(), "{schema}");
}

#[test]
fn flattening_an_adjacent_enum_with_an_untagged_variant_has_no_discriminator() {
    let schema = own_schema::<FlattensAdjacentSelfFlattening>();
    let flattened = &schema["allOf"][1];
    assert!(flattened.get("anyOf").is_some() && flattened.get("discriminator").is_none(), "{schema}");
}

#[test]
fn flattening_a_tagged_enum_without_an_untagged_variant_keeps_the_discriminator() {
    assert_eq!(
        Adjacent::flatten_schema().expect("an enum flattens")["discriminator"],
        json!({"propertyName": "kind"})
    );
    assert_eq!(
        InternallyTagged::flatten_schema().expect("an enum flattens")["discriminator"],
        json!({"propertyName": "kind"})
    );
}

#[test]
fn flattening_an_untagged_variant_of_an_adjacent_enum_does_not_constrain_the_parent() {
    // Loose validation on purpose, as for the other flattened map variants.
    let instance = serde_json::to_value(FlattensAdjacentUntaggedVariant {
        name: "x".into(),
        variant: FlattenableAdjacentUntaggedVariant::Mapped {
            x: 1,
            extra: HashMap::from([("a".to_owned(), 2)]),
        },
    })
    .expect("sample serializes");
    assert!(schema_accepts::<FlattensAdjacentUntaggedVariant>(&instance), "{instance}");
}

#[test]
fn the_input_of_skipped_and_untagged_variants_is_what_serde_reads() {
    assert_requests_are_read::<SkippedSimple>();
    assert_requests_are_read::<SkippedExternal>();
    assert_requests_are_read::<SkippedLeavesUnits>();
    assert_requests_are_read::<SkippedInternal>();
    assert_requests_are_read::<SkippedAdjacent>();
    assert_requests_are_read::<SkippedUntagged>();
    assert_requests_are_read::<UntaggedVariantExternal>();
    assert_requests_are_read::<UntaggedVariantUnits>();
    assert_requests_are_read::<UntaggedVariantInternal>();
    assert_requests_are_read::<UntaggedVariantAdjacent>();
}

#[test]
fn a_skipped_unit_variant_is_not_documented() {
    assert_rejected::<SkippedSimple>(json!("Skipped"));
}

#[test]
fn a_skipped_external_variant_is_not_documented() {
    assert_rejected::<SkippedExternal>(json!({"Skipped": 1}));
}

#[test]
fn a_skipped_variant_can_leave_a_plain_string_enum() {
    assert_rejected::<SkippedLeavesUnits>(json!({"Skipped": 1}));
    assert_serialization_matches(&SkippedLeavesUnits::Kept);
}

#[test]
fn a_skipped_internal_variant_is_not_documented() {
    assert_rejected::<SkippedInternal>(json!({"kind": "Skipped", "other": 1}));
}

#[test]
fn a_skipped_adjacent_variant_is_not_documented() {
    assert_rejected::<SkippedAdjacent>(json!({"kind": "Skipped", "data": 1}));
}

#[test]
fn a_skipped_untagged_variant_is_not_documented() {
    assert_rejected::<SkippedUntagged>(json!("text"));
}

#[test]
fn an_enum_with_every_variant_skipped_accepts_nothing() {
    assert_rejected::<AllSkippedInternal>(json!({"kind": "Skipped", "other": 1}));
}

#[test]
fn a_variant_skipped_in_one_direction_stays_documented() {
    assert_serialization_matches(&SkippedOneDirection::OutputOnly(1));
    let document = schema_document::<SkippedOneDirection>();
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    let input = json!({"InputOnly": 1});
    assert!(serde_json::from_value::<SkippedOneDirection>(input.clone()).is_ok());
    assert!(validator.validate(&input).is_ok(), "the input-only branch must stay: {document}");
}

#[test]
fn an_untagged_newtype_variant_of_an_external_enum_has_no_tag() {
    assert_serialization_matches(&UntaggedVariantExternal::Newtype("x".into()));
}

#[test]
fn an_untagged_struct_variant_of_an_external_enum_has_no_tag() {
    assert_serialization_matches(&UntaggedVariantExternal::Named { x: 1 });
}

#[test]
fn an_untagged_tuple_variant_of_an_external_enum_has_no_tag() {
    assert_serialization_matches(&UntaggedVariantExternal::Tuple(1, 2));
}

#[test]
fn an_untagged_unit_variant_of_an_external_enum_is_null() {
    assert_serialization_matches(&UntaggedVariantExternal::Null);
}

#[test]
fn an_untagged_unit_variant_keeps_an_enum_from_being_a_string_enum() {
    assert_serialization_matches(&UntaggedVariantUnits::Null);
}

#[test]
fn an_untagged_newtype_variant_of_an_internal_enum_has_no_tag() {
    assert_serialization_matches(&UntaggedVariantInternal::Newtype("x".into()));
}

#[test]
fn an_untagged_struct_variant_of_an_internal_enum_has_no_tag() {
    assert_serialization_matches(&UntaggedVariantInternal::Named { x: 1 });
}

#[test]
fn an_untagged_unit_variant_of_an_internal_enum_is_null() {
    assert_serialization_matches(&UntaggedVariantInternal::Null);
}

#[test]
fn an_untagged_newtype_variant_of_an_adjacent_enum_has_no_tag() {
    assert_serialization_matches(&UntaggedVariantAdjacent::Newtype("x".into()));
}

#[test]
fn an_untagged_struct_variant_of_an_adjacent_enum_has_no_tag() {
    assert_serialization_matches(&UntaggedVariantAdjacent::Named { x: 1 });
}

#[test]
fn an_untagged_unit_variant_of_an_adjacent_enum_is_null() {
    assert_serialization_matches(&UntaggedVariantAdjacent::Null);
}

#[test]
fn an_untagged_variant_turns_the_branches_into_an_any_of() {
    let schema = own_schema::<OverlappingUntaggedVariant>();
    assert!(schema.get("oneOf").is_none() && schema.get("anyOf").is_some(), "{schema}");
    assert_serialization_matches(&OverlappingUntaggedVariant::Tagged { value: 1 });
}

#[test]
fn an_untagged_variant_removes_the_internal_discriminator() {
    let schema = own_schema::<UntaggedVariantInternal>();
    assert!(schema.get("anyOf").is_some() && schema.get("discriminator").is_none(), "{schema}");
}

#[test]
fn an_untagged_variant_removes_the_adjacent_discriminator() {
    let schema = own_schema::<UntaggedVariantAdjacent>();
    assert!(schema.get("anyOf").is_some() && schema.get("discriminator").is_none(), "{schema}");
}

#[test]
fn flattening_an_untagged_variant_of_an_internal_enum_has_no_tag() {
    let document = schema_document::<FlattensUntaggedVariant>();
    let instance = serde_json::to_value(FlattensUntaggedVariant {
        name: "x".into(),
        variant: FlattenableUntaggedVariant::Mapped {
            x: 1,
            extra: HashMap::from([("a".to_owned(), 2)]),
        },
    })
    .expect("sample serializes");
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    assert!(validator.validate(&instance).is_ok(), "{instance} vs {document}");
}

#[test]
fn flattening_an_untagged_variant_of_an_external_enum_does_not_constrain_the_parent() {
    // Loose validation on purpose, as for the other flattened map variants.
    let document = schema_document::<FlattensExternalUntaggedVariant>();
    let instance = serde_json::to_value(FlattensExternalUntaggedVariant {
        name: "x".into(),
        variant: FlattenableExternalUntaggedVariant::Mapped {
            x: 1,
            extra: HashMap::from([("a".to_owned(), 2)]),
        },
    })
    .expect("sample serializes");
    let validator = jsonschema::validator_for(&document).expect("generated schema is a valid JSON Schema");
    assert!(validator.validate(&instance).is_ok(), "{instance} vs {document}");
}

#[test]
fn other_variants_are_documented_as_serde_writes_them() {
    assert_serialization_matches(&OtherInternal::Fallback);
    assert_serialization_matches(&OtherAdjacent::Fallback);
    assert!(own_schema::<OtherInternal>().get("discriminator").is_some());
    assert!(own_schema::<OtherAdjacent>().get("discriminator").is_some());
}

/// `#[serde(tag = "..")]` on a struct: serde writes the tag with the struct's serialize name.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TaggedStruct {
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", rename = "renamed", rename_all = "camelCase")]
struct TaggedRenamedStruct {
    some_field: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", rename(serialize = "written", deserialize = "read"))]
struct TaggedSplitRenameStruct {
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
struct HoldsTaggedStruct {
    field: TaggedStruct,
    list: Vec<TaggedStruct>,
}

/// A wrapper is its value on the wire, so it documents the value's tag.
#[derive(Schematic, Serialize, Deserialize)]
struct NewtypeOfTaggedStruct(TaggedStruct);

#[derive(Schematic, Serialize, Deserialize)]
#[serde(transparent)]
struct TransparentTaggedStruct {
    inner: TaggedStruct,
}

/// A field neither written nor read is not on the wire, so its name does not collide with the tag.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TagNamedLikeSkippedField {
    #[serde(skip)]
    kind: u8,
    value: u8,
}

/// The tag next to a flattened type, whose fields a map can read or a branch can declare, is not
/// documented.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TaggedFlattenedStruct {
    value: u8,
    #[serde(flatten)]
    paging: Paging,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "struct_kind")]
struct TaggedFlattenedEnum {
    value: u8,
    #[serde(flatten)]
    inner: InternallyTagged,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TaggedFlattenedMap {
    value: u8,
    #[serde(flatten)]
    extra: HashMap<String, u8>,
}

/// A parent that flattens a tagged struct stays what it is without the tag.
#[derive(Schematic, Serialize, Deserialize)]
struct FlattensTaggedStruct {
    parent: u8,
    #[serde(flatten)]
    tagged: TaggedStruct,
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensOptionalTaggedStruct {
    parent: u8,
    #[serde(flatten)]
    tagged: Option<TaggedStruct>,
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensNewtypeOfTaggedStruct {
    parent: u8,
    #[serde(flatten)]
    tagged: NewtypeOfTaggedStruct,
}

#[derive(Schematic, Serialize, Deserialize)]
struct ParentFieldNamedLikeTag {
    kind: u8,
    #[serde(flatten)]
    tagged: TaggedStruct,
}

#[derive(Schematic, Serialize, Deserialize)]
struct ParentFieldNamedLikeTagOfNewtype {
    kind: u8,
    #[serde(flatten)]
    tagged: NewtypeOfTaggedStruct,
}

#[derive(Schematic, Serialize, Deserialize)]
enum VariantFlattensTaggedStruct {
    Named {
        parent: u8,
        #[serde(flatten)]
        tagged: TaggedStruct,
    },
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum VariantTagNamedLikeStructTag {
    Named {
        #[serde(flatten)]
        tagged: TaggedStruct,
        other: u8,
    },
}

/// An internally tagged newtype variant merges the struct's fields, and those have no tag.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "variant")]
enum WrapsTaggedStruct {
    Wrapped(TaggedStruct),
}

/// With no fields, parameters come from the schema's properties, and the tag must not be one.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TaggedEmptyStruct {}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TaggedStructWithOnlySkippedFields {
    #[serde(skip)]
    skipped: u8,
    #[serde(skip_serializing, default)]
    read: u8,
}

/// Every field below is written or read under the tag's name, so a tag would reject it.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "value")]
struct TagNamedLikeField {
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensTagNamedLikeField {
    parent: u8,
    #[serde(flatten)]
    inner: TagNamedLikeField,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "someKind", rename_all = "camelCase")]
struct TagNamedLikeRenamedField {
    some_kind: u8,
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TagNamedLikeReadOnlyField {
    #[serde(skip_serializing)]
    kind: u8,
    value: u8,
}

#[allow(dead_code)]
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TagNamedLikeWriteOnlyField {
    #[serde(skip_deserializing)]
    kind: u8,
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TagNamedLikeAlias {
    #[serde(alias = "kind")]
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TagNamedLikeReadName {
    #[serde(rename(deserialize = "kind"))]
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
struct TagNamedLikeWrittenName {
    #[serde(rename(serialize = "kind"))]
    value: u8,
}

/// serde writes this tag and rejects a request carrying it, so the schema stays without the tag.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
struct TaggedStructDenyingUnknownFields {
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize)]
struct FlattensTaggedStructDenyingUnknownFields {
    parent: u8,
    #[serde(flatten)]
    tagged: TaggedStructDenyingUnknownFields,
}

/// A conversion replaces what serde writes or reads, so the tag is not necessarily on the wire.
#[derive(Serialize, Deserialize, Clone)]
struct TaggedStructProxy {
    value: u8,
}

#[derive(Schematic, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", into = "TaggedStructProxy")]
struct TaggedStructInto {
    value: u8,
}

impl From<TaggedStructInto> for TaggedStructProxy {
    fn from(tagged: TaggedStructInto) -> Self {
        TaggedStructProxy { value: tagged.value }
    }
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", from = "TaggedStructProxy")]
struct TaggedStructFrom {
    value: u8,
}

impl From<TaggedStructProxy> for TaggedStructFrom {
    fn from(proxy: TaggedStructProxy) -> Self {
        TaggedStructFrom { value: proxy.value }
    }
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", try_from = "TaggedStructProxy")]
struct TaggedStructTryFrom {
    value: u8,
}

impl TryFrom<TaggedStructProxy> for TaggedStructTryFrom {
    type Error = String;

    fn try_from(proxy: TaggedStructProxy) -> Result<Self, String> {
        Ok(TaggedStructTryFrom { value: proxy.value })
    }
}

#[test]
fn a_struct_tag_is_documented() {
    assert_serialization_matches(&TaggedStruct { value: 1 });
    assert_eq!(
        own_schema::<TaggedStruct>()["properties"]["kind"],
        json!({"type": "string", "enum": ["TaggedStruct"]})
    );
}

#[test]
fn a_struct_tag_carries_the_renamed_name() {
    assert_serialization_matches(&TaggedRenamedStruct { some_field: 1 });
    assert_eq!(own_schema::<TaggedRenamedStruct>()["properties"]["kind"]["enum"], json!(["renamed"]));
}

#[test]
fn a_struct_tag_carries_the_serialize_name() {
    assert_serialization_matches(&TaggedSplitRenameStruct { value: 1 });
    assert_eq!(own_schema::<TaggedSplitRenameStruct>()["properties"]["kind"]["enum"], json!(["written"]));
}

#[test]
fn a_struct_tag_is_documented_in_a_referenced_component() {
    assert_serialization_matches(&HoldsTaggedStruct {
        field: TaggedStruct { value: 1 },
        list: vec![TaggedStruct { value: 2 }],
    });
}

#[test]
fn a_struct_tag_is_documented_through_a_wrapper() {
    assert_serialization_matches(&NewtypeOfTaggedStruct(TaggedStruct { value: 1 }));
    assert_serialization_matches(&TransparentTaggedStruct {
        inner: TaggedStruct { value: 1 },
    });
}

#[test]
fn a_skipped_field_named_like_a_struct_tag_does_not_hide_it() {
    assert_serialization_matches(&TagNamedLikeSkippedField { kind: 0, value: 1 });
    assert_eq!(
        own_schema::<TagNamedLikeSkippedField>()["properties"]["kind"],
        json!({"type": "string", "enum": ["TagNamedLikeSkippedField"]})
    );
}

#[test]
fn a_struct_tag_is_not_required_because_serde_ignores_it_when_reading() {
    assert_request_accepted::<TaggedStruct>(json!({"value": 1}));
}

/// serde ignores the value it reads, so this is the one request the document is stricter about.
#[test]
fn a_struct_tag_rejects_a_value_serde_ignores() {
    let instance = json!({"kind": "other", "value": 1});
    assert!(serde_json::from_value::<TaggedStruct>(instance.clone()).is_ok());
    assert!(!schema_accepts::<TaggedStruct>(&instance));
}

#[test]
fn the_input_of_a_tagged_struct_is_what_serde_reads() {
    assert_requests_are_read::<TaggedStruct>();
    assert_requests_are_read::<TaggedRenamedStruct>();
    assert_requests_are_read::<TaggedSplitRenameStruct>();
    assert_requests_are_read::<HoldsTaggedStruct>();
    assert_requests_are_read::<NewtypeOfTaggedStruct>();
    assert_requests_are_read::<TransparentTaggedStruct>();
    assert_requests_are_read::<TagNamedLikeSkippedField>();
}

#[test]
fn a_struct_tag_is_not_a_parameter() {
    let names: Vec<&str> = TaggedStruct::fields().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["value"]);
    let names: Vec<&str> = NewtypeOfTaggedStruct::fields().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["value"]);
}

#[cfg(feature = "openapi")]
#[test]
fn a_fieldless_tagged_struct_has_no_parameters() {
    use gotcha::{Either, ParameterProvider};
    let Either::Left(parameters) = <gotcha::axum::extract::Query<TaggedEmptyStruct> as ParameterProvider>::generate("/".into()) else {
        panic!("a query has no body");
    };
    assert!(parameters.is_empty(), "{parameters:?}");
    let Either::Left(parameters) = <gotcha::axum::extract::Query<TaggedStructWithOnlySkippedFields> as ParameterProvider>::generate("/".into()) else {
        panic!("a query has no body");
    };
    assert!(parameters.is_empty(), "{parameters:?}");
}

#[test]
fn a_fieldless_struct_leaves_its_tag_undocumented() {
    assert_eq!(own_schema::<TaggedEmptyStruct>()["properties"], json!({}));
    assert_eq!(own_schema::<TaggedStructWithOnlySkippedFields>()["properties"], json!({}));
    assert_request_accepted::<TaggedEmptyStruct>(json!({"kind": "other"}));
    assert_request_accepted::<TaggedStructWithOnlySkippedFields>(json!({"kind": "other"}));
}

#[test]
fn a_flattened_field_leaves_the_struct_tag_undocumented() {
    assert_eq!(own_schema::<TaggedFlattenedStruct>()["properties"].get("kind"), None);
    assert_request_accepted::<TaggedFlattenedStruct>(json!({"kind": "other", "value": 1, "page": 1, "per_page": 2}));
    let schema = own_schema::<TaggedFlattenedEnum>();
    assert_eq!(schema["allOf"][0]["properties"].get("struct_kind"), None, "{schema}");
    assert_requests_are_read::<TaggedFlattenedStruct>();
    assert_requests_are_read::<TaggedFlattenedEnum>();
}

/// serde hands the tag's name to the map, whose values are not strings, so a request carrying the
/// tag is rejected. A document declaring the tag would accept it.
#[test]
fn a_flattened_map_leaves_the_struct_tag_undocumented() {
    let instance = json!({"value": 1, "kind": "TaggedFlattenedMap"});
    assert!(serde_json::from_value::<TaggedFlattenedMap>(instance.clone()).is_err());
    assert!(!schema_accepts::<TaggedFlattenedMap>(&instance));
    assert_eq!(own_schema::<TaggedFlattenedMap>()["properties"].get("kind"), None);
}

#[test]
fn flattening_a_tagged_struct_leaves_the_tag_out_of_the_parent() {
    for parent in [
        own_schema::<FlattensTaggedStruct>(),
        own_schema::<FlattensOptionalTaggedStruct>(),
        own_schema::<FlattensNewtypeOfTaggedStruct>(),
        own_schema::<FlattensTaggedStructDenyingUnknownFields>(),
    ] {
        assert_eq!(parent["type"], json!("object"));
        assert_eq!(parent["properties"].get("kind"), None, "{parent}");
        assert!(parent.get("allOf").is_none(), "{parent}");
    }
    let variant = own_schema::<VariantFlattensTaggedStruct>();
    assert_eq!(variant["oneOf"][0]["properties"]["Named"]["properties"].get("kind"), None, "{variant}");
    assert_requests_are_read::<FlattensTaggedStruct>();
    assert_requests_are_read::<FlattensOptionalTaggedStruct>();
    assert_requests_are_read::<FlattensNewtypeOfTaggedStruct>();
    assert_requests_are_read::<VariantFlattensTaggedStruct>();
}

#[test]
fn a_parent_field_named_like_a_flattened_tag_keeps_its_schema() {
    assert_eq!(own_schema::<ParentFieldNamedLikeTag>()["properties"]["kind"], json!({"type": "integer"}));
    assert_request_accepted::<ParentFieldNamedLikeTag>(json!({"kind": 1, "value": 2}));
    assert_requests_are_read::<ParentFieldNamedLikeTag>();
    assert_request_accepted::<ParentFieldNamedLikeTagOfNewtype>(json!({"kind": 1, "value": 2}));
}

#[test]
fn an_enum_tag_named_like_a_flattened_tag_keeps_its_schema() {
    assert_request_accepted::<VariantTagNamedLikeStructTag>(json!({"kind": "Named", "value": 1, "other": 2}));
    assert_requests_are_read::<VariantTagNamedLikeStructTag>();
}

#[test]
fn an_internally_tagged_newtype_variant_leaves_a_struct_tag_out() {
    let schema = own_schema::<WrapsTaggedStruct>();
    assert_eq!(schema["oneOf"][0]["properties"].get("kind"), None, "{schema}");
    assert_request_accepted::<WrapsTaggedStruct>(json!({"variant": "Wrapped", "kind": "other", "value": 1}));
    assert_requests_are_read::<WrapsTaggedStruct>();
}

#[test]
fn a_field_named_like_the_tag_keeps_its_schema() {
    assert_serialization_matches(&TagNamedLikeField { value: 1 });
    let schema = own_schema::<TagNamedLikeField>();
    assert_eq!(schema["properties"]["value"], json!({"type": "integer"}));
    assert_eq!(schema["required"], json!(["value"]));
    assert_serialization_matches(&FlattensTagNamedLikeField {
        parent: 1,
        inner: TagNamedLikeField { value: 2 },
    });
    assert_requests_are_read::<TagNamedLikeField>();
    assert_eq!(own_schema::<TagNamedLikeRenamedField>()["properties"]["someKind"], json!({"type": "integer"}));
    assert_requests_are_read::<TagNamedLikeRenamedField>();
}

#[test]
fn a_tag_named_like_a_field_read_only_stays_undocumented() {
    assert_eq!(own_schema::<TagNamedLikeReadOnlyField>()["properties"].get("kind"), None);
    assert_request_accepted::<TagNamedLikeReadOnlyField>(json!({"kind": 1, "value": 1}));
}

#[test]
fn a_tag_named_like_a_field_written_only_stays_undocumented() {
    assert_eq!(own_schema::<TagNamedLikeWriteOnlyField>()["properties"].get("kind"), None);
    assert_request_accepted::<TagNamedLikeWriteOnlyField>(json!({"kind": 1, "value": 1}));
}

#[test]
fn a_tag_named_like_an_alias_stays_undocumented() {
    assert_eq!(own_schema::<TagNamedLikeAlias>()["properties"].get("kind"), None);
    assert!(serde_json::from_str::<TagNamedLikeAlias>(r#"{"kind": 1}"#).is_ok());
    assert!(schema_accepts::<TagNamedLikeAlias>(&json!({"kind": 1, "value": 1})));
}

#[test]
fn a_tag_named_like_a_read_name_stays_undocumented() {
    assert_eq!(own_schema::<TagNamedLikeReadName>()["properties"].get("kind"), None);
    assert_request_accepted::<TagNamedLikeReadName>(json!({"kind": 1, "value": 1}));
}

#[test]
fn a_tag_named_like_a_written_name_stays_undocumented() {
    assert_eq!(own_schema::<TagNamedLikeWrittenName>()["properties"].get("kind"), None);
    assert_request_accepted::<TagNamedLikeWrittenName>(json!({"kind": 1, "value": 1}));
}

#[test]
fn a_struct_tag_serde_rejects_reading_stays_undocumented() {
    assert_eq!(own_schema::<TaggedStructDenyingUnknownFields>()["properties"].get("kind"), None);
    assert!(serde_json::from_str::<TaggedStructDenyingUnknownFields>(r#"{"kind": "TaggedStructDenyingUnknownFields", "value": 1}"#).is_err());
    assert_request_accepted::<TaggedStructDenyingUnknownFields>(json!({"value": 1}));
    assert_requests_are_read::<TaggedStructDenyingUnknownFields>();
}

#[test]
fn a_struct_tag_serde_rejects_reading_stays_out_of_a_flattening_parent() {
    assert_request_accepted::<FlattensTaggedStructDenyingUnknownFields>(json!({"parent": 1, "value": 2}));
    assert_requests_are_read::<FlattensTaggedStructDenyingUnknownFields>();
}

#[test]
fn a_conversion_leaves_the_struct_tag_undocumented() {
    assert_eq!(own_schema::<TaggedStructInto>()["properties"].get("kind"), None);
    assert_eq!(own_schema::<TaggedStructFrom>()["properties"].get("kind"), None);
    assert_eq!(own_schema::<TaggedStructTryFrom>()["properties"].get("kind"), None);
    // `into` writes the proxy, which has no tag.
    assert_eq!(
        serde_json::to_value(TaggedStructInto { value: 1 }).expect("sample serializes"),
        json!({"value": 1})
    );
}

// Known mismatches, deliberately not asserted (follow-up work; arch review A4/A14):
//
// - Names that need per-direction schemas. `rename(serialize = "out")` and
//   `rename_all(serialize = "camelCase")` (with no other side, or with a deserialize side that
//   leaves the identifier alone) document the name serde reads, so the output does not match. Split
//   names document the name serde writes, so a request body does not match. `Valid` errors and
//   parameters use the name serde reads.
// - An enum `Path<T>` parameter: an enum has no `fields()`, so its name comes from the route.
// - One-way skips on a tuple-variant field need per-direction schemas. serde writes
//   `P(u8, #[serde(skip_serializing)] String)` as `{"P":[1]}` and reads `{"P":[1,"x"]}`;
//   `skip_deserializing` is the reverse, and a newtype variant is written or read as a unit variant
//   in the skipped direction. The schema documents the direction the field is in.
// - An adjacently tagged newtype variant whose only field is `#[serde(skip)]`: serde writes
//   `{"kind":"V"}` and reads `{"kind":"V","data":null}`. The schema documents what it writes.
// - `P(u8, #[serde(skip_serializing_if = "Option::is_none")] Option<String>)`: serde writes
//   `{"P":[1]}` for `None`, while the schema requires one item per field.
// - A `#[serde(tag = "..")]` struct documents its tag only in its own schema. serde also writes the
//   tag through `#[serde(flatten)]` and an internally tagged newtype variant, and those stay
//   without it, so strict validation rejects the output.
// - A `#[serde(tag = "..")]` struct that leaves its tag undocumented, so strict validation rejects
//   the output. Next to a `#[serde(flatten)]` field, a flattened map can read the tag's name. A
//   field may be written or read under that name, through an `alias` too. `deny_unknown_fields`
//   needs per-direction schemas: serde writes the tag and rejects a request carrying it. A
//   conversion may not write it at all. A struct with no documented field has its parameters taken
//   from the schema's properties, where the tag must not appear.
// - A `#[serde(tag = "..")]` struct documents one value, while serde ignores the value it reads:
//   `{"kind":"other","value":1}` is read by serde and rejected by the document.
// - The same through an untagged newtype variant, whose branch is a `$ref` to the struct's own
//   schema. Flattening that enum composes the tag with the parent, so the tag's value also conflicts
//   with a parent's field or a sibling's property of that name: `{"kind":1,"v":1}` for
//   `struct P { kind: u8, #[serde(flatten)] e: E }` is read by serde and rejected by the document.
// - The same `$ref` also composes the tag with a parent in the other ways it can use that name.
//   serde's output `{"kind":1,"value":1}` fails when the parent's `kind` comes after the flatten or
//   is `skip_deserializing`. `{"own":1,"value":1,"kind":"T1"}` is accepted by the document and
//   rejected by serde when the parent also flattens a `HashMap<String, u8>`. A
//   `#[serde(alias = "kind")] k: u8` beside the enum makes `{"k":1,"kind":"T1","value":1}` a duplicate
//   field for serde, which the document accepts.
// - A flattened externally tagged enum's unit variant: serde writes `{"A": null}` next to the
//   parent's fields, while the flattened schema documents `"A"`.
// - An externally tagged enum whose variants are all zero-field struct or tuple variants (`A {}`,
//   `A()`): it is still routed to the string-enum shape (the dispatch checks emptiness, not
//   `Style::Unit`).
// - Flattening an enum with an `#[serde(untagged)]` unit variant: serde writes only the parent's
//   fields, while the branch documents `null`, so the parent's object fails validation.
// - Flattening an enum with an `#[serde(untagged)]` newtype variant that wraps a map: the branch is
//   a bare `{"type": "object"}`, which lets `anyOf` accept nearly anything, and strict validation
//   rejects the entries.
// - Flattening an enum with an `#[serde(untagged)]` newtype variant that wraps `Option<map>` or a
//   map with `#[schematic(name)]`: the branch keeps the map's `additionalProperties`, so it also
//   constrains the parent's fields, and serde's output fails lenient validation when one of them
//   has another type (on `main` too).
// - Flattening an internally tagged enum whose newtype variant wraps a map omits the map's
//   `additionalProperties` from the composed schema: keeping it would constrain the parent's own
//   declared fields (the variant branch is composed through `allOf`), so strict validation
//   rejects the map entries.
// - Flattening an internally tagged or untagged enum whose struct variant flattens a map strips
//   the branch's `additionalProperties` for the same reason: the entries stay undocumented and
//   strict validation rejects them. The external and adjacent forms keep it nested under
//   `properties.<Variant>` / `properties.data`, so they document the entries and pass both
//   validations.
// - Flattening an untagged enum whose newtype variant wraps a map strips the branch's
//   `additionalProperties`; the entries stay undocumented and strict validation rejects them.
// - Flattening an untagged enum whose newtype variant wraps a struct that itself flattens a map
//   composes through that struct's `$ref`, so the map's `additionalProperties` still constrains
//   the parent's fields.
