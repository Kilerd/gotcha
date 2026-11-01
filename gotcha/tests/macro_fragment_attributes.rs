//! Attributes written through `macro_rules!` fragments are read like the literals they expand to.
//!
//! A `$x:expr` fragment (and a `$x:literal` before another item) reaches the derive inside an
//! invisible group, which syn 2 keeps as an expression where syn 1 read the literal through it;
//! a negative number before another item is a negation rather than a literal.

#![allow(dead_code)]

use gotcha::{Schematic, Validate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

macro_rules! fragments {
    ($rename:expr, $tag:expr, $doc:expr, $expr:expr, $literal:literal, $tt:tt) => {
        #[doc = $doc]
        #[derive(Schematic, Serialize, Deserialize, Validate)]
        #[validate(crate = "gotcha::validator")]
        struct Fragments {
            #[serde(rename = $rename, default)]
            renamed: i64,
            #[schematic(example = -$expr, title = "expr")]
            negated_expr: i64,
            #[schematic(example = -$literal, title = "literal")]
            negated_literal: i64,
            #[schematic(example = -$tt, title = "tt")]
            negated_tt: i64,
            #[validate(range(min = -$expr, max = 10))]
            bounded: i64,
        }

        #[derive(Schematic, Serialize, Deserialize)]
        #[serde(tag = $tag, rename_all = "snake_case")]
        enum Tagged {
            First { value: u8 },
        }
    };
}

fragments!("renamed_by_fragment", "kind", "Documented by a fragment.", 3, 4, 5);

#[derive(Schematic, Serialize, Deserialize)]
struct NegativeLiterals {
    #[schematic(example = -1, title = "before another item")]
    not_last: i64,
    #[schematic(title = "last", example = -2)]
    last: i64,
    #[schematic(example = -1.5, default = -2)]
    float: f64,
}

fn schema_of<T: Schematic>() -> Value {
    serde_json::to_value(T::generate_schema().schema).unwrap()
}

#[test]
fn serde_attributes_from_fragments_rename_and_tag() {
    let schema = schema_of::<Fragments>();
    assert!(schema["properties"].get("renamed_by_fragment").is_some(), "{schema}");
    assert!(schema["properties"].get("renamed").is_none(), "{schema}");
    let wire = Fragments::wire_fields().iter().find(|field| field.rust == "renamed").unwrap();
    assert_eq!(wire.wire, "renamed_by_fragment");

    let tagged = schema_of::<Tagged>();
    assert_eq!(tagged["discriminator"]["propertyName"], "kind");
    assert_eq!(tagged["oneOf"][0]["properties"]["kind"]["enum"], json!(["first"]));
}

#[test]
fn doc_comments_from_fragments_are_descriptions() {
    assert_eq!(schema_of::<Fragments>()["description"], "Documented by a fragment.");
}

#[test]
fn negated_fragments_are_negative_examples() {
    let properties = &schema_of::<Fragments>()["properties"];
    assert_eq!(properties["negated_expr"]["examples"], json!([-3]));
    assert_eq!(properties["negated_literal"]["examples"], json!([-4]));
    assert_eq!(properties["negated_tt"]["examples"], json!([-5]));
    assert_eq!(properties["bounded"]["minimum"], json!(-3.0));
    assert_eq!(properties["bounded"]["maximum"], json!(10.0));
}

#[test]
fn negative_literals_are_values_in_any_position() {
    let properties = &schema_of::<NegativeLiterals>()["properties"];
    assert_eq!(properties["not_last"]["examples"], json!([-1]));
    assert_eq!(properties["not_last"]["title"], "before another item");
    assert_eq!(properties["last"]["examples"], json!([-2]));
    assert_eq!(properties["float"]["examples"], json!([-1.5]));
    assert_eq!(properties["float"]["default"], json!(-2));
}
