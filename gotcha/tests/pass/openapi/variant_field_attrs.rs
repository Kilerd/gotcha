//! `#[schematic(..)]` on enum variant fields is applied to the value they document, in every
//! tag form: the newtype content of externally/adjacently/untagged variants and the merged
//! fields of an internally tagged variant.

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct Inner {
    id: u32,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum InternalTagged {
    V(#[schematic(description = "inner payload", example = 7)] Inner),
}

#[derive(Schematic, Serialize, Deserialize)]
enum ExternalTagged {
    V(#[schematic(description = "external payload")] Inner),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum AdjacentTagged {
    V(#[schematic(description = "adjacent payload")] Inner),
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum Untagged {
    V(#[schematic(description = "untagged payload")] Inner),
}

/// Doc comments on tuple/newtype fields are not documentation: those shapes never carried a
/// description, so only explicit `#[schematic(description)]` may add one. Named
/// struct-variant fields keep their doc comments, as they did before.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum DocFields {
    Tuple(#[doc = "tuple field docs"] Inner),
    Named {
        #[doc = "named field docs"]
        id: u32,
    },
}

/// A `title` on an internally tagged newtype field is documented on the variant branch.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum Titled {
    V(#[schematic(title = "Payload")] Inner),
}

/// Without annotations the derive must not build a throwaway schema for the inner type, which
/// would register an orphan component during collection.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum PlainInternal {
    V(Inner),
}

fn first_branch<'a>(schema: &'a serde_json::Value, keyword: &str) -> &'a serde_json::Value {
    schema[keyword].as_array().expect("branch array").first().expect("a branch")
}

fn main() {
    let internal = serde_json::to_value(InternalTagged::generate_schema().schema).unwrap();
    assert_eq!(first_branch(&internal, "oneOf")["description"], "inner payload");

    let external = serde_json::to_value(ExternalTagged::generate_schema().schema).unwrap();
    let content = &first_branch(&external, "oneOf")["properties"]["V"];
    assert_eq!(content["description"], "external payload");

    let adjacent = serde_json::to_value(AdjacentTagged::generate_schema().schema).unwrap();
    let content = &first_branch(&adjacent, "oneOf")["properties"]["data"];
    assert_eq!(content["description"], "adjacent payload");

    let untagged = serde_json::to_value(Untagged::generate_schema().schema).unwrap();
    assert_eq!(first_branch(&untagged, "oneOf")["description"], "untagged payload");

    let doc_fields = serde_json::to_value(DocFields::generate_schema().schema).unwrap();
    let branches = doc_fields["oneOf"].as_array().expect("oneOf branches");
    let tuple = branches.iter().find(|branch| branch["title"] == "Tuple").expect("Tuple branch");
    assert!(tuple["properties"]["id"].get("description").is_none(), "a tuple field's doc comment is not a description");
    let named = branches.iter().find(|branch| branch["title"] == "Named").expect("Named branch");
    assert_eq!(named["properties"]["id"]["description"], "named field docs", "a named struct-variant field keeps its doc comment");

    let titled = serde_json::to_value(Titled::generate_schema().schema).unwrap();
    assert_eq!(first_branch(&titled, "oneOf")["title"], "Payload");

    // No annotations on the tuple field: the inner type is merged from `fields()` and is never
    // registered as a component of its own during collection.
    let (_, components) = gotcha::gotcha_core::registry::collect(|| PlainInternal::generate_schema());
    assert!(
        !components.keys().any(|name| name.contains("Inner")),
        "an uncalled inner schema must not be registered: {:?}",
        components.keys().collect::<Vec<_>>()
    );
}
