pub use gotcha_core as schema;

#[path = "../../schemas.rs"]
pub mod schemas;
pub use schemas::Record;

// A library can implement the core trait for its own type without the framework.
pub struct LibraryId;

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
}
