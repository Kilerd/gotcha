//! `#[schematic(...)]` field attributes attach schema *documentation* metadata
//! (title/description/example/default/format). An explicit `description` overrides the
//! doc comment, and `example`/`default` keep their JSON type.
//!
//! Validation constraints (min/max, length, pattern, …) are intentionally NOT here — they
//! belong to request validation (issue #9) so a rule is written once.

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct Product {
    /// this doc comment must be overridden by the schematic description
    #[schematic(title = "Name", description = "The product name", example = "Widget")]
    name: String,

    #[schematic(format = "email")]
    contact: String,

    // `example` / `default` keep their JSON type instead of becoming strings.
    #[schematic(example = 42, default = 10)]
    quantity: u32,
}

// The same `#[schematic(...)]` handling must reach fields inside enum variants.
#[derive(Schematic, Serialize, Deserialize)]
enum Event {
    Created {
        /// this doc comment must be overridden too
        #[schematic(description = "when it happened", format = "date-time")]
        at: String,
    },
}

fn main() {
    let schema = serde_json::to_value(Product::generate_schema().schema).unwrap();
    let props = &schema["properties"];

    // Metadata, with description overriding the doc comment.
    let name = &props["name"];
    assert_eq!(name["title"], "Name");
    assert_eq!(name["description"], "The product name", "explicit description overrides doc comment");
    assert_eq!(name["example"], "Widget");

    // `format` lands on the schema's dedicated field.
    assert_eq!(props["contact"]["format"], "email");

    // example/default keep their numeric JSON type.
    let quantity = &props["quantity"];
    assert!(quantity["example"].is_number(), "typed example stays a number, not the string \"42\"");
    assert_eq!(quantity["example"], 42);
    assert_eq!(quantity["default"], 10);

    // Enum-variant fields get the same treatment (asserted via the serialized schema so the
    // test does not couple to the exact tagged-union nesting).
    let event = serde_json::to_string(&Event::generate_schema().schema).unwrap();
    assert!(event.contains("when it happened"), "schematic description reaches enum variant fields: {event}");
    assert!(event.contains("date-time"), "schematic format reaches enum variant fields: {event}");
}
