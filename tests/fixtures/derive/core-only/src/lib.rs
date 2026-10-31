pub use gotcha_core as schema;

#[path = "../../schemas.rs"]
pub mod schemas;
pub use schemas::Record;

// A library can implement the core trait for its own type without the framework.
pub struct LibraryId;

// `WireField` is `#[non_exhaustive]`, so a table is built with its `const` constructors. They
// can be used from another crate and in a `const`/`static` initializer.
const LIBRARY_ID_FIELDS: &[gotcha_core::WireField] = &[gotcha_core::WireField::new("library_id", "libraryId")];

impl gotcha_core::Schematic for LibraryId {
    fn name() -> &'static str {
        "LibraryId"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn wire_fields() -> &'static [gotcha_core::WireField] {
        LIBRARY_ID_FIELDS
    }
}

#[test]
fn a_hand_written_wire_table_can_be_a_const() {
    let table = <LibraryId as gotcha_core::Schematic>::wire_fields();
    assert_eq!((table[0].rust, table[0].wire, table[0].flatten), ("library_id", "libraryId", false));
}
