//! Collect named schemas by Rust type identity and resolve their public component names.
//!
//! A collection scope prevents recursive expansion. Names are assigned only after every type
//! is known, so collisions cannot make the result depend on registration order.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::Hash;

use convert_case::{Case, Casing};
use oas::{Schema, SchemaValue};
use serde_json::Value;

use crate::{EnhancedSchema, Schematic};

thread_local! {
    static ACTIVE: RefCell<Option<Registry>> = const { RefCell::new(None) };
    /// Whether [`probing`] is looking into a value.
    static PROBING: Cell<bool> = const { Cell::new(false) };
}

#[derive(Default)]
struct Registry {
    entries: BTreeMap<String, Entry>,
}

struct Entry {
    preferred: String,
    module: String,
    explicit: bool,
    schema: Option<Schema>,
}

/// Values containing schema references that must be finalized after collection.
///
/// Implement this for custom collection results by forwarding to their schema-bearing fields.
/// Non-schema data does not need to be serialized or traversed.
pub trait SchemaReferences {
    /// Replace the temporary reference URIs in `names` with their final component URIs.
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>);
}

impl SchemaReferences for () {
    fn resolve_schema_references(&mut self, _: &BTreeMap<String, String>) {}
}

impl<T: SchemaReferences> SchemaReferences for Vec<T> {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        for value in self {
            value.resolve_schema_references(names);
        }
    }
}

impl SchemaReferences for EnhancedSchema {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        self.schema.resolve_schema_references(names);
    }
}

impl SchemaReferences for Schema {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        let mut value = self.to_value();
        resolve_json(&mut value, names, true);
        *self = serde_json::from_value(value).expect("reference replacement preserves schema structure");
    }
}

impl SchemaReferences for SchemaValue {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        if let Some(schema) = self.as_object_mut() {
            schema.resolve_schema_references(names);
        }
    }
}

fn resolve_document<T: serde::Serialize + serde::de::DeserializeOwned>(value: &mut T, names: &BTreeMap<String, String>) {
    let mut document = serde_json::to_value(&*value).expect("OpenAPI value is serializable");
    resolve_json(&mut document, names, false);
    *value = serde_json::from_value(document).expect("reference replacement preserves OpenAPI structure");
}

impl SchemaReferences for oas::Operation {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        resolve_document(self, names);
    }
}
impl SchemaReferences for oas::Parameter {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        resolve_document(self, names);
    }
}
impl SchemaReferences for oas::RequestBody {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        resolve_document(self, names);
    }
}

impl<K: Eq + Hash, V: SchemaReferences> SchemaReferences for HashMap<K, V> {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        for value in self.values_mut() {
            value.resolve_schema_references(names);
        }
    }
}
impl<K: Ord, V: SchemaReferences> SchemaReferences for BTreeMap<K, V> {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        for value in self.values_mut() {
            value.resolve_schema_references(names);
        }
    }
}

#[cfg(feature = "axum")]
impl<L: SchemaReferences, R: SchemaReferences> SchemaReferences for either::Either<L, R> {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        match self {
            Self::Left(value) => value.resolve_schema_references(names),
            Self::Right(value) => value.resolve_schema_references(names),
        }
    }
}

// Only reference-bearing document/schema fields are traversed. In particular examples/defaults
// are application data, and a property named "example" is still a schema, not an annotation.
fn resolve_json(value: &mut Value, names: &BTreeMap<String, String>, schema: bool) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(|v| resolve_json(v, names, schema)),
        Value::Object(object) => {
            if let Some(Value::String(reference)) = object.get_mut("$ref") {
                if let Some(name) = names.get(reference) {
                    *reference = name.clone();
                }
            }
            for (key, child) in object {
                if key.starts_with("x-") || matches!(key.as_str(), "example" | "examples") || (schema && matches!(key.as_str(), "default" | "enum" | "const")) {
                    continue;
                }
                if schema && matches!(key.as_str(), "properties" | "patternProperties" | "$defs" | "definitions") {
                    if let Value::Object(properties) = child {
                        for property in properties.values_mut() {
                            resolve_json(property, names, true);
                        }
                    }
                } else if schema && key == "discriminator" {
                    if let Some(Value::Object(mapping)) = child.get_mut("mapping") {
                        for reference in mapping.values_mut() {
                            if let Some(name) = reference.as_str().and_then(|s| names.get(s)) {
                                *reference = Value::String(name.clone());
                            }
                        }
                    }
                } else {
                    resolve_json(child, names, schema || matches!(key.as_str(), "schema" | "itemSchema" | "contentSchema"));
                }
            }
        }
        _ => {}
    }
}

fn reference(name: &str) -> String {
    format!("#/components/schemas/{name}")
}

fn temporary_reference(identity: &str) -> String {
    // Outside the component namespace until finalized; never emitted in the assembled spec.
    format!("gotcha:rust-type:{identity}")
}

/// Collect schemas, finalize names, and rewrite references in both `f`'s result and components.
///
/// Nested scopes and unwinding restore the enclosing scope. Types registered more than once
/// are built only once. Custom result types implement [`SchemaReferences`].
pub fn collect<R: SchemaReferences>(f: impl FnOnce() -> R) -> (R, BTreeMap<String, Schema>) {
    struct Restore(Option<Registry>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|active| *active.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(ACTIVE.with(|active| active.borrow_mut().replace(Registry::default())));
    let mut result = f();
    let registry = ACTIVE.with(|active| active.borrow_mut().take()).expect("active collection");
    let names = registry.names();
    let references = names.iter().map(|(identity, name)| (temporary_reference(identity), reference(name))).collect();
    result.resolve_schema_references(&references);
    let schemas = registry
        .entries
        .into_iter()
        .map(|(identity, entry)| {
            let mut schema = entry.schema.expect("schema construction completed");
            schema.resolve_schema_references(&references);
            (names[&identity].clone(), schema)
        })
        .collect();
    (result, schemas)
}

impl Registry {
    fn names(&self) -> BTreeMap<String, String> {
        let mut names: BTreeMap<_, _> = self.entries.iter().map(|(id, entry)| (id.clone(), entry.preferred.clone())).collect();
        let mut depth: BTreeMap<String, usize> = BTreeMap::new();
        let mut collisions = BTreeSet::new();
        loop {
            let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (id, name) in &names {
                groups.entry(name.clone()).or_default().push(id.clone());
            }
            let mut changed = false;
            for (name, ids) in groups.into_iter().filter(|(_, ids)| ids.len() > 1) {
                collisions.insert((name.clone(), ids.clone()));
                // A unique explicit name wins over generated names that happen to match it.
                let explicit: Vec<_> = ids
                    .iter()
                    .filter(|id| self.entries[*id].explicit && self.entries[*id].preferred == name)
                    .collect();
                let original: Vec<_> = ids.iter().filter(|id| self.entries[*id].preferred == name).collect();
                let reserved = if explicit.len() == 1 {
                    Some(explicit[0])
                } else if original.len() == 1 {
                    Some(original[0])
                } else {
                    None
                };
                for id in &ids {
                    if reserved == Some(id) {
                        continue;
                    }
                    let entry = &self.entries[id];
                    let level = depth.entry(id.clone()).or_default();
                    *level += 1;
                    let modules: Vec<_> = entry.module.split("::").filter(|s| !s.is_empty()).collect();
                    let candidate = if *level <= modules.len() {
                        format!("{}{}", modules[modules.len() - *level..].join("_").to_case(Case::Pascal), entry.preferred)
                    } else {
                        // PascalCase and generic shortening can themselves collide. The complete
                        // identity's bytes provide an injective, order-independent last resort.
                        let suffix: String = id.bytes().map(|b| format!("{b:02x}")).collect();
                        format!("{}_{suffix}{}", entry.preferred, "_".repeat(*level - modules.len() - 1))
                    };
                    names.insert(id.clone(), candidate);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (name, ids) in collisions {
            let resolved: Vec<_> = ids.iter().map(|id| format!("{id} => {}", names[id])).collect();
            tracing::warn!(schema_name = %name, types = ?ids, resolved_names = ?resolved,
                "OpenAPI schema name collision; using distinct component names. Use #[schematic(name = \"...\")] to choose a unique public name");
        }
        names
    }
}

/// Register a custom schema whose explicit identity is its public name.
///
/// Derived implementations use [`schema_or_ref_for`] to distinguish Rust types. Handwritten
/// implementations should use that function too when separate types may share a display name.
pub fn schema_or_ref(name: &str, required: bool, build: impl FnOnce() -> EnhancedSchema) -> EnhancedSchema {
    register(&format!("legacy:{name}"), name.to_owned(), "", true, required, build)
}

/// Register a derived type by its full Rust type name, independently of its component name.
///
/// `name` is an explicit override when present. Otherwise generic arguments are included in
/// the preferred component name; module prefixes are added only when names collide.
#[doc(hidden)]
pub fn schema_or_ref_for<T: ?Sized>(name: Option<&str>, module: &str, required: bool, build: impl FnOnce() -> EnhancedSchema) -> EnhancedSchema {
    let identity = std::any::type_name::<T>();
    let preferred = name.map(str::to_owned).unwrap_or_else(|| short_type_name(identity));
    register(identity, preferred, module, name.is_some(), required, build)
}

/// Look into a value in a scope of its own, so that nothing it registers stays in the document.
///
/// What a probe builds is thrown away apart from its answer about the value itself, which the
/// branches of an enum flattened further inside do not change. So those branches stop at
/// [`unexamined_branch`], without a probe of their own. That also ends a value leading back to
/// itself through a flattened enum, and keeps the cost at one probe per branch of the document.
fn probing<R: SchemaReferences>(probe: impl FnOnce() -> R) -> (R, BTreeMap<String, Schema>) {
    struct Finish(bool);
    impl Drop for Finish {
        fn drop(&mut self) {
            PROBING.set(self.0);
        }
    }
    let _finish = Finish(PROBING.replace(true));
    collect(probe)
}

/// What a flattened enum's branch contributes to a probe further up: see [`probing`].
fn unexamined_branch() -> EnhancedSchema {
    EnhancedSchema {
        schema: Schema::default(),
        required: true,
    }
}

/// Whether nothing is left of a map's schema without its `additionalProperties`, so nothing in it
/// can refer to a scope thrown away or hold a branch a probe stopped at.
fn is_plain_map(schema: &Schema) -> bool {
    schema._ref.is_none() && schema.content_schema.is_none() && schema.extras.is_empty()
}

/// The schema of the value a newtype variant wraps, on a branch written without a tag.
///
/// A flattened enum's branch composes with the parent's fields, so a map's
/// `additionalProperties` cannot stay on it. A plain map is probed in a scope of its own: its value
/// type would otherwise stay registered with nothing left referencing it. Any other value with the
/// keyword is hand-written and is built for real, because the probe's answer may not stand alone.
#[doc(hidden)]
pub fn wrapped_value_schema<T: Schematic>(flattening: bool) -> EnhancedSchema {
    if flattening {
        if PROBING.get() {
            return unexamined_branch();
        }
        let (mut probe, _) = probing(T::generate_schema);
        if probe.schema.extras.remove("additionalProperties").is_some() {
            if is_plain_map(&probe.schema) {
                return probe;
            }
            let mut value = T::generate_schema();
            value.schema.extras.remove("additionalProperties");
            return value;
        }
    }
    T::generate_schema()
}

/// [`wrapped_value_schema`] for an internally tagged enum, whose newtype variants merge a struct's
/// fields: a struct becomes an object of its fields, not a component of its own. What one object of
/// fields cannot say stays a reference: a value that may be absent, or that flattens an enum.
#[doc(hidden)]
pub fn wrapped_value_fields_schema<T: Schematic>(flattening: bool) -> EnhancedSchema {
    if flattening && PROBING.get() {
        return unexamined_branch();
    }
    let fields = T::fields();
    let flattens_enum = || {
        let mut flattened = None;
        probing(|| flattened = T::flatten_schema());
        flattened.is_some()
    };
    if fields.is_empty() || !T::required() || flattens_enum() {
        return wrapped_value_schema::<T>(flattening);
    }
    let mut properties = HashMap::new();
    let mut required = vec![];
    for (name, field) in fields {
        properties.insert(name.to_owned(), field.schema.to_value());
        if field.required {
            required.push(name);
        }
    }
    let mut schema = Schema {
        _type: Some("object".into()),
        description: T::doc(),
        ..Schema::default()
    };
    schema.extras.insert("properties".to_owned(), serde_json::json!(properties));
    schema.extras.insert("required".to_owned(), serde_json::json!(required));
    if !flattening {
        if let Some(additional) = T::additional_properties() {
            schema.extras.insert("additionalProperties".to_owned(), additional);
        }
    }
    EnhancedSchema { schema, required: true }
}

/// Record `length(min/max)` constraints with the keyword that matches the schema's type.
///
/// Strings get `minLength`/`maxLength`, arrays `minItems`/`maxItems`, and objects
/// `minProperties`/`maxProperties`. The `Schematic` derive emits a call to this for every
/// `#[validate(length(..))]` field, so the keyword follows the generated schema (a set or a map
/// gets the keywords of the array or object it serializes to) rather than the Rust type written
/// on the field, including when `#[schematic(as = "..")]` changes the described type.
///
/// A bound already present on the schema (for example the exact item count of `[T; N]`) is kept
/// when it is tighter: the larger lower bound and the smaller upper bound win.
///
/// The type is read through the `anyOf` union `Option<T>` produces. A schema whose type is not
/// visible (`$ref` and other composed schemas) keeps the string keywords.
#[doc(hidden)]
pub fn apply_length_constraints(schema: &mut Schema, min: Option<u64>, max: Option<u64>) {
    let (min_keyword, max_keyword) = length_keywords(schema);
    if let Some(min) = min {
        let lower = schema.extras.get(min_keyword).and_then(Value::as_u64).map_or(min, |existing| existing.max(min));
        schema.extras.insert(min_keyword.to_owned(), serde_json::json!(lower));
    }
    if let Some(max) = max {
        let upper = schema.extras.get(max_keyword).and_then(Value::as_u64).map_or(max, |existing| existing.min(max));
        schema.extras.insert(max_keyword.to_owned(), serde_json::json!(upper));
    }
}

/// The `(min, max)` keywords for a schema's serialized type.
fn length_keywords(schema: &Schema) -> (&'static str, &'static str) {
    match schema_kind(schema) {
        Some("array") => ("minItems", "maxItems"),
        Some("object") => ("minProperties", "maxProperties"),
        // Strings and schemas whose type is not visible keep the string keywords.
        _ => ("minLength", "maxLength"),
    }
}

/// The JSON type name of `schema`, looking through the `anyOf` union `Option<T>` produces.
fn schema_kind(schema: &Schema) -> Option<&'static str> {
    match &schema._type {
        Some(oas::SchemaType::Single(name)) => match name.as_str() {
            "string" => return Some("string"),
            "array" => return Some("array"),
            "object" => return Some("object"),
            _ => {}
        },
        Some(oas::SchemaType::Multiple(names)) => {
            for name in names {
                match name.as_str() {
                    "string" => return Some("string"),
                    "array" => return Some("array"),
                    "object" => return Some("object"),
                    _ => {}
                }
            }
        }
        None => {}
    }
    // `Option<T>` is `anyOf: [T, {"type": "null"}]`.
    let branches = schema.extras.get("anyOf")?.as_array()?;
    branches.iter().find_map(|branch| {
        let branch: Schema = serde_json::from_value(branch.clone()).ok()?;
        schema_kind(&branch)
    })
}

fn short_type_name(identity: &str) -> String {
    // Keep the last segment of each qualified path, including paths inside generic arguments.
    // Punctuation separates arguments. Any lossy spelling collision is resolved by names().
    let mut chars = identity.chars().peekable();
    let mut without_lifetimes = String::new();
    while let Some(character) = chars.next() {
        if character == '\'' {
            let mut word = String::new();
            while chars.peek().is_some_and(|c| c.is_alphanumeric() || *c == '_') {
                word.push(chars.next().unwrap());
            }
            if chars.peek() == Some(&'\'') {
                // A character const argument, rather than a lifetime.
                chars.next();
                without_lifetimes.push_str(&word);
            }
        } else {
            without_lifetimes.push(character);
        }
    }
    without_lifetimes
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .filter(|part| !part.is_empty())
        .map(|part| part.rsplit("::").next().unwrap_or(part))
        .collect::<Vec<_>>()
        .join("_")
}

fn register(identity: &str, preferred: String, module: &str, explicit: bool, required: bool, build: impl FnOnce() -> EnhancedSchema) -> EnhancedSchema {
    let collecting = ACTIVE.with(|active| active.borrow().is_some());
    if !collecting {
        return build();
    }
    let known = ACTIVE.with(|active| active.borrow().as_ref().unwrap().entries.contains_key(identity));
    if !known {
        ACTIVE.with(|active| {
            active.borrow_mut().as_mut().unwrap().entries.insert(
                identity.to_owned(),
                Entry {
                    preferred,
                    module: module.to_owned(),
                    explicit,
                    schema: None,
                },
            );
        });
        let built = build();
        ACTIVE.with(|active| active.borrow_mut().as_mut().unwrap().entries.get_mut(identity).unwrap().schema = Some(built.schema));
    }
    EnhancedSchema {
        schema: Schema::reference(temporary_reference(identity)),
        required,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_and_encoded_content_references_preserve_siblings_and_payloads() {
        use serde_json::json;
        let temporary = "gotcha:rust-type:Item";
        let final_ref = "#/components/schemas/Item";
        let payload = json!({"$ref": temporary});
        let mut body: oas::RequestBody = serde_json::from_value(json!({
            "content": {"application/jsonl": {
                "schema": true,
                "itemSchema": {
                    "$ref": temporary,
                    "description": "Item with encoded data",
                    "properties": {
                        "example": {"$ref": temporary},
                        "data": {
                            "type": "string", "contentMediaType": "application/json",
                            "contentSchema": {"$ref": temporary, "default": payload}
                        }
                    },
                    "examples": [payload], "default": payload, "const": payload, "enum": [payload]
                },
                "example": payload
            }}
        }))
        .unwrap();
        body.resolve_schema_references(&BTreeMap::from([(temporary.into(), final_ref.into())]));
        let value = serde_json::to_value(body).unwrap();
        let media = &value["content"]["application/jsonl"];
        let item = &media["itemSchema"];
        assert_eq!(media["schema"], true);
        assert_eq!(item["$ref"], final_ref);
        assert_eq!(item["description"], "Item with encoded data");
        assert_eq!(item["properties"]["example"]["$ref"], final_ref);
        assert_eq!(item["properties"]["data"]["contentSchema"]["$ref"], final_ref);
        assert_eq!(item["properties"]["data"]["contentSchema"]["default"], payload);
        assert_eq!(item["examples"], json!([payload]));
        assert_eq!(item["enum"], json!([payload]));
        assert_eq!(item["const"], payload);
        assert_eq!(item["default"], payload);
        assert_eq!(media["example"], payload);
        let mut boolean = SchemaValue::Boolean(false);
        boolean.resolve_schema_references(&BTreeMap::new());
        assert_eq!(serde_json::to_value(boolean).unwrap(), false);
    }

    fn object_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: Schema {
                _type: Some("object".into()),
                format: None,
                nullable: None,
                description: None,
                extras: BTreeMap::new(),
                ..Schema::default()
            },
            required: true,
        }
    }

    fn is_ref(schema: &EnhancedSchema) -> bool {
        schema.schema._ref.is_some()
    }

    #[test]
    fn separate_assemblies_do_not_share_schemas() {
        // Two apps in one process (e.g. a data port and an admin port) each assemble their own
        // spec; neither may pick up the other's components.
        let (_, data_port) = collect(|| schema_or_ref("DataModel", true, object_schema));
        let (_, admin_port) = collect(|| schema_or_ref("AdminModel", true, object_schema));

        assert_eq!(data_port.keys().collect::<Vec<_>>(), ["DataModel"]);
        assert_eq!(admin_port.keys().collect::<Vec<_>>(), ["AdminModel"]);
    }

    #[test]
    fn outside_a_scope_schemas_stay_inline() {
        let schema = schema_or_ref("Standalone", true, object_schema);
        assert!(!is_ref(&schema), "no active scope means the historical inline behavior");
        assert_eq!(schema.schema.to_value()["type"], "object");
    }

    #[test]
    fn nested_scopes_restore_the_enclosing_one() {
        let (_, outer) = collect(|| {
            let (_, inner) = collect(|| schema_or_ref("Inner", true, object_schema));
            assert_eq!(inner.keys().collect::<Vec<_>>(), ["Inner"]);
            // Still collecting into the outer scope after the inner one finished.
            assert!(is_ref(&schema_or_ref("Outer", true, object_schema)));
        });
        assert_eq!(outer.keys().collect::<Vec<_>>(), ["Outer"]);
    }

    #[test]
    fn a_panic_does_not_strand_the_scope() {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| collect::<()>(|| panic!("assembly failed"))));
        std::panic::set_hook(hook);
        assert!(result.is_err());

        // The thread is usable again: with no scope active, schemas are inline once more.
        assert!(!is_ref(&schema_or_ref("AfterPanic", true, object_schema)), "scope leaked past a panic");
    }
}
