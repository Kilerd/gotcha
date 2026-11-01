//! `#[schematic(..)]` on a newtype struct's inner field is applied to the schema the newtype
//! delegates to (it used to be dropped).

use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct Url(#[schematic(description = "absolute url", example = "https://example.com", format = "uri")] String);

/// Inner's own documentation.
#[derive(Schematic, Serialize, Deserialize)]
struct Inner {
    id: u32,
}

/// No field documentation: the inner type's own doc comment survives (a doc comment on the
/// field itself is not documentation, matching the tuple/newtype decision).
#[derive(Schematic, Serialize, Deserialize)]
struct Undocumented(/// field doc must not be used
    Inner);

/// An explicit field description overrides the inner type's doc comment.
#[derive(Schematic, Serialize, Deserialize)]
struct Explicit(#[schematic(description = "explicit description")] Inner);

fn main() {
    let url = serde_json::to_value(Url::generate_schema().schema).unwrap();
    assert_eq!(url["description"], "absolute url");
    assert_eq!(url["format"], "uri");
    assert_eq!(url["examples"][0], "https://example.com");

    let inner = serde_json::to_value(Undocumented::generate_schema().schema).unwrap();
    assert_eq!(inner["type"], "object");
    assert_eq!(inner["description"], "Inner's own documentation.", "the inner doc comment must survive");
    let explicit = serde_json::to_value(Explicit::generate_schema().schema).unwrap();
    assert_eq!(explicit["description"], "explicit description");
}
