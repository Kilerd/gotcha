//! Variant-level `#[schematic(title, description)]` is added to the variant's schema branch,
//! with a doc comment as the description fallback. Non-object branches and all-unit enums drop
//! the annotations (they still compile).

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct Inner {
    id: u32,
}

#[derive(Schematic, Serialize, Deserialize)]
enum Event {
    #[schematic(title = "Created", description = "a creation")]
    Created(Inner),
    /// removed the event
    Deleted { id: u32 },
}

/// Variant `description` beats the variant doc comment, which beats a field description.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum Precedence {
    /// variant doc
    DocBeatsField(#[schematic(description = "field description")] Inner),
    #[schematic(description = "variant description")]
    /// variant doc
    ExplicitBeatsDoc(#[schematic(description = "field description")] Inner),
}

/// All-unit enums are a plain string enum and accept but ignore variant annotations.
#[derive(Schematic, Serialize, Deserialize)]
enum Simple {
    #[schematic(description = "ignored")]
    Alpha,
}

/// Untagged newtype branches are not objects, so they drop the annotations too.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(untagged)]
enum Untagged {
    #[schematic(title = "ignored")]
    Number(u32),
}

fn first_branch<'a>(schema: &'a serde_json::Value, keyword: &str) -> &'a serde_json::Value {
    schema[keyword].as_array().expect("branch array").first().expect("a branch")
}

fn main() {
    let event = serde_json::to_value(Event::generate_schema().schema).unwrap();
    let branches = event["oneOf"].as_array().expect("oneOf branches");
    let created = branches.iter().find(|branch| branch["title"] == "Created").expect("Created branch");
    assert_eq!(created["description"], "a creation");
    let deleted = branches.iter().find(|branch| branch["title"] == "Deleted").expect("Deleted branch");
    assert_eq!(deleted["description"], "removed the event");

    let precedence = serde_json::to_value(Precedence::generate_schema().schema).unwrap();
    let branches = precedence["oneOf"].as_array().expect("oneOf branches");
    let doc = branches.iter().find(|branch| branch["title"] == "DocBeatsField").expect("DocBeatsField branch");
    assert_eq!(doc["description"], "variant doc");
    let explicit = branches.iter().find(|branch| branch["title"] == "ExplicitBeatsDoc").expect("ExplicitBeatsDoc branch");
    assert_eq!(explicit["description"], "variant description");

    // Accepted, ignored: these still compile and generate their normal shapes.
    let simple = serde_json::to_value(Simple::generate_schema().schema).unwrap();
    assert_eq!(simple["type"], "string");
    let untagged = serde_json::to_value(Untagged::generate_schema().schema).unwrap();
    assert_eq!(first_branch(&untagged, "anyOf")["type"], "integer");
}
