//! `#[validate(...)]` constraints (from the validator crate) are mirrored into the OpenAPI schema,
//! so a rule written once for runtime validation also documents the field:
//! `range` (incl. exclusive bounds) → minimum/maximum (+ exclusiveMinimum/Maximum),
//! `length` (min/max/equal) → minLength/maxLength for strings, minItems/maxItems for arrays
//! (including sets, deques and fixed-size arrays) and minProperties/maxProperties for maps,
//! `email`/`url` → format. Unmodelled validators (`regex`, `custom`, …) are silently skipped.
//!
//! String and `Vec` keywords are unchanged from the previous release. The `openapi` example
//! contains no `length`, so the evidence for unchanged output is the assertions below (plus the
//! example's exported document compared before and after the change).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

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
    // Sets and deques serialize as arrays, so `length` documents item counts.
    #[validate(length(min = 1, max = 6))]
    tags_set: HashSet<String>,
    #[validate(length(min = 2))]
    tags_deque: VecDeque<String>,
    #[validate(length(max = 7))]
    tags_tree: BTreeSet<String>,
    // Maps serialize as objects, so `length` documents property counts.
    #[validate(length(min = 1, max = 4))]
    labels: HashMap<String, String>,
    #[validate(length(min = 1))]
    labels_tree: BTreeMap<String, String>,
    // `Option<T>` documents both branches; `length` applies to the non-null one.
    #[validate(length(min = 2, max = 8))]
    nickname: Option<String>,
    #[validate(length(min = 1))]
    optional_tags: Option<Vec<String>>,
    // A fixed-size array already documents its exact length; `length` must not loosen it.
    #[validate(length(min = 1, max = 2))]
    fixed: [String; 2],
    #[validate(length(min = 5))]
    fixed_min: [String; 3],
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

    // Exclusive bounds are numeric JSON Schema constraints.
    assert!(props["ratio"].get("minimum").is_none());
    assert_eq!(props["ratio"]["exclusiveMinimum"], 0.0);
    assert!(props["ratio"].get("maximum").is_none());
    assert_eq!(props["ratio"]["exclusiveMaximum"], 10.0);

    // `length` on a `Vec` → minItems / maxItems.
    assert_eq!(props["tags"]["minItems"], 1);
    assert_eq!(props["tags"]["maxItems"], 5);

    // Sets and deques are arrays on the wire, not strings.
    for field in ["tags_set", "tags_deque", "tags_tree"] {
        assert!(props[field].get("minLength").is_none(), "{field} must not document minLength");
        assert!(props[field].get("maxLength").is_none(), "{field} must not document maxLength");
    }
    assert_eq!(props["tags_set"]["minItems"], 1);
    assert_eq!(props["tags_set"]["maxItems"], 6);
    assert_eq!(props["tags_deque"]["minItems"], 2);
    assert_eq!(props["tags_tree"]["maxItems"], 7);

    // Maps are objects on the wire, so they document property counts.
    for field in ["labels", "labels_tree"] {
        assert!(props[field].get("minLength").is_none(), "{field} must not document minLength");
        assert!(props[field].get("maxLength").is_none(), "{field} must not document maxLength");
        assert!(props[field].get("minItems").is_none(), "{field} must not document minItems");
    }
    assert_eq!(props["labels"]["minProperties"], 1);
    assert_eq!(props["labels"]["maxProperties"], 4);
    assert_eq!(props["labels_tree"]["minProperties"], 1);

    // The keyword is read through `Option`'s anyOf union.
    assert_eq!(props["nickname"]["minLength"], 2);
    assert_eq!(props["nickname"]["maxLength"], 8);
    assert_eq!(props["optional_tags"]["minItems"], 1);

    // `[T; N]` already pins the exact item count; a looser `length` keeps the tighter bound.
    assert_eq!(props["fixed"]["minItems"], 2);
    assert_eq!(props["fixed"]["maxItems"], 2);
    assert_eq!(props["fixed_min"]["minItems"], 5);
    assert_eq!(props["fixed_min"]["maxItems"], 3);

    // `length(equal = ..)` fixes both bounds.
    assert_eq!(props["code"]["minLength"], 3);
    assert_eq!(props["code"]["maxLength"], 3);
}
