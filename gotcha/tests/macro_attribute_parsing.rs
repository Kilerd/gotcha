//! The `Schematic` derive reads doc comments without parsing unrelated attributes.
//!
//! `#[validate(regex(path = *RE))]`, `#[validate(length(max = MAX_LEN))]` and
//! `#[doc = include_str!(..)]` cannot all be represented as `syn::Meta`; the derive must skip
//! the parts it cannot read instead of aborting the expansion, while still keeping the literal
//! constraints declared next to a non-literal one.

#![allow(dead_code)]

use std::sync::LazyLock;

use gotcha::{Schematic, Validate};
use regex::Regex;
use serde_json::Value;

static NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[a-z]+$").unwrap());
const MAX_LEN: u64 = 8;

#[derive(Schematic, Validate)]
#[validate(crate = "gotcha::validator")]
struct Constrained {
    /// An unmodelled validator with a non-literal argument.
    #[validate(regex(path = *NAME_RE))]
    slug: String,
    /// A modelled validator whose bounds are mixed literal and non-literal.
    #[validate(length(min = 1, max = MAX_LEN))]
    code: String,
    /// A modelled validator with only a non-literal bound.
    #[validate(length(max = MAX_LEN))]
    bounded: String,
    /// A doc value that is a macro call.
    #[doc = include_str!("fixtures/doc_comment.md")]
    described: String,
}

#[derive(Schematic)]
#[doc = include_str!("fixtures/doc_comment.md")]
struct Documented {
    value: String,
}

fn schema_of<T: Schematic>() -> Value {
    serde_json::to_value(T::generate_schema().schema).unwrap()
}

#[test]
fn literal_length_bounds_survive_a_non_literal_sibling() {
    let schema = schema_of::<Constrained>();
    let properties = &schema["properties"];
    // `min = 1` is readable, so it documents minLength; `max = MAX_LEN` is not a literal and is
    // skipped rather than dropping the whole attribute.
    assert_eq!(properties["code"]["minLength"], 1);
    assert!(properties["code"].get("maxLength").is_none());
    // A non-literal regex argument documents nothing, but does not abort the derive.
    assert!(properties["slug"].get("minLength").is_none());
    // An attribute whose only bound is non-literal also leaves no keyword behind.
    assert!(properties["bounded"].get("maxLength").is_none());
}

#[test]
fn doc_values_that_are_not_literals_are_skipped() {
    // The literal doc line is kept; the `include_str!` expression next to it has no value at
    // macro-expansion time and is skipped.
    let field = &schema_of::<Constrained>()["properties"]["described"];
    assert_eq!(field["description"], "A doc value that is a macro call.");
    // With only an `include_str!` doc, the type documents no description rather than aborting.
    let schema = schema_of::<Documented>();
    assert!(schema.get("description").is_none());
}
