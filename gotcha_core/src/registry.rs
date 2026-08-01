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
///
/// Each call installs a *fresh* scope and restores the enclosing one on the way out, so
/// independent assemblies — two apps in one process, say a data port and an admin port — each
/// collect only their own schemas.
pub fn collect<R>(f: impl FnOnce() -> R) -> (R, BTreeMap<String, Schema>) {
    /// Puts the enclosing scope back on drop, so a panic inside `f` cannot strand this thread
    /// inside a collection scope (which would make later inline schemas come out as `$ref`).
    struct Restore(Option<Registry>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|active| *active.borrow_mut() = self.0.take());
        }
    }

    let _restore = Restore(ACTIVE.with(|active| active.borrow_mut().replace(Registry::default())));
    let result = f();
    let finished = ACTIVE.with(|active| active.borrow_mut().take());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn object_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: Schema {
                _type: Some("object".to_string()),
                format: None,
                nullable: None,
                description: None,
                extras: BTreeMap::new(),
            },
            required: true,
        }
    }

    fn is_ref(schema: &EnhancedSchema) -> bool {
        schema.schema.extras.contains_key("$ref")
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
        assert_eq!(schema.schema._type.as_deref(), Some("object"));
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
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| collect(|| panic!("assembly failed"))));
        std::panic::set_hook(hook);
        assert!(result.is_err());

        // The thread is usable again: with no scope active, schemas are inline once more.
        assert!(!is_ref(&schema_or_ref("AfterPanic", true, object_schema)), "scope leaked past a panic");
    }
}
