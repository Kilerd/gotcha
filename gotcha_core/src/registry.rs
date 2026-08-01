//! Schema collection for OpenAPI `components/schemas` + `$ref` emission.
//!
//! [`Schematic::generate_schema`](crate::Schematic::generate_schema) takes no arguments, so there
//! is nowhere to thread a registry through. Instead a collection scope is installed on the current
//! thread for the duration of spec assembly; derived `generate_schema` implementations consult it
//! via [`schema_or_ref`].
//!
//! Two things fall out of this:
//!
//! - **Recursive types terminate.** `struct Node { children: Vec<Node> }` used to expand forever;
//!   now the inner `Node` is already "in progress" and short-circuits to its `$ref`.
//! - **Reused types are emitted once**, under `components/schemas`, and referenced elsewhere.
//!
//! Outside a collection scope nothing changes: schemas are built inline exactly as before, so a
//! direct `T::generate_schema()` call (tests, ad-hoc use) still returns a self-contained schema.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

use oas::Schema;

use crate::EnhancedSchema;

thread_local! {
    /// The collection scope currently running on this thread, if any.
    static ACTIVE: RefCell<Option<Registry>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Registry {
    /// Schemas registered so far, keyed by `Schematic::name()`.
    schemas: BTreeMap<String, Schema>,
    /// Names whose schema is mid-construction; hitting one again means a recursive type.
    in_progress: HashSet<String>,
}

/// A schema that is nothing but a `$ref` to a registered component.
fn reference_schema(name: &str) -> Schema {
    let mut extras = BTreeMap::new();
    extras.insert("$ref".to_string(), serde_json::Value::String(format!("#/components/schemas/{name}")));
    Schema {
        _type: None,
        format: None,
        nullable: None,
        description: None,
        extras,
    }
}

/// Run `f` with schema collection enabled, returning its result along with every schema registered
/// while it ran.
pub fn collect<R>(f: impl FnOnce() -> R) -> (R, BTreeMap<String, Schema>) {
    let previous = ACTIVE.with(|active| active.borrow_mut().replace(Registry::default()));
    let result = f();
    let finished = ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let finished = active.take();
        *active = previous;
        finished
    });
    (result, finished.map(|registry| registry.schemas).unwrap_or_default())
}

/// Entry point used by `#[derive(Schematic)]`.
///
/// Outside a collection scope this simply returns `build()` — the historical inline behavior.
/// Inside one, the built schema is registered under `name` and a `$ref` to it is returned; a name
/// that is already registered (or mid-construction, i.e. recursive) skips rebuilding entirely.
pub fn schema_or_ref(name: &str, required: bool, build: impl FnOnce() -> EnhancedSchema) -> EnhancedSchema {
    let collecting = ACTIVE.with(|active| active.borrow().is_some());
    if !collecting {
        return build();
    }

    let known = ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .is_some_and(|registry| registry.schemas.contains_key(name) || registry.in_progress.contains(name))
    });

    if !known {
        ACTIVE.with(|active| {
            if let Some(registry) = active.borrow_mut().as_mut() {
                registry.in_progress.insert(name.to_string());
            }
        });
        // Called with no borrow held: `build` re-enters this function for nested field types.
        let built = build();
        ACTIVE.with(|active| {
            if let Some(registry) = active.borrow_mut().as_mut() {
                registry.in_progress.remove(name);
                registry.schemas.insert(name.to_string(), built.schema);
            }
        });
    }

    EnhancedSchema {
        schema: reference_schema(name),
        required,
    }
}
