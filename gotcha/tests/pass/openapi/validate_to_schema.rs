//! `#[validate(...)]` constraints (from the validator crate) are mirrored into the OpenAPI schema,
//! so a rule written once for runtime validation also documents the field:
//! `range` (incl. exclusive bounds) → minimum/maximum (+ exclusiveMinimum/Maximum),
//! `length` (min/max/equal) → minLength/maxLength (minItems/maxItems for collections),
//! `email`/`url` → format. Unmodelled validators (`regex`, `custom`, …) are silently skipped.

use gotcha::{Schematic, Validate};
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize, Validate)]
struct CreateUser {
    #[validate(length(min = 1, max = 64))]
    name: String,
    #[validate(email)]
    email: String,
    #[validate(range(min = 0, max = 150))]
    age: u8,
    #[validate(range(exclusive_min = 0, exclusive_max = 10))]
    ratio: u8,
    #[validate(length(min = 1, max = 5))]
    tags: Vec<String>,
    #[validate(length(equal = 3))]
    code: String,
}

fn main() {
    let schema = serde_json::to_value(CreateUser::generate_schema().schema).unwrap();
    let props = &schema["properties"];

    // `length` on a string → minLength / maxLength.
    assert_eq!(props["name"]["minLength"], 1);
    assert_eq!(props["name"]["maxLength"], 64);

    // `email` → format.
    assert_eq!(props["email"]["format"], "email");

    // `range` → minimum / maximum.
    assert_eq!(props["age"]["minimum"], 0.0);
    assert_eq!(props["age"]["maximum"], 150.0);

    // Exclusive bounds → minimum/maximum + exclusiveMinimum/Maximum (the OpenAPI 3.0 form).
    assert_eq!(props["ratio"]["minimum"], 0.0);
    assert_eq!(props["ratio"]["exclusiveMinimum"], true);
    assert_eq!(props["ratio"]["maximum"], 10.0);
    assert_eq!(props["ratio"]["exclusiveMaximum"], true);

    // `length` on a collection → minItems / maxItems.
    assert_eq!(props["tags"]["minItems"], 1);
    assert_eq!(props["tags"]["maxItems"], 5);

    // `length(equal = ..)` fixes both bounds.
    assert_eq!(props["code"]["minLength"], 3);
    assert_eq!(props["code"]["maxLength"], 3);
}
