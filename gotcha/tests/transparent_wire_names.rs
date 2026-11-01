//! The validation remapper follows serde's flattened wire shape through
//! `#[serde(transparent)]` structs: a transparent wrapper contributes no wire segment.

use gotcha::validator::{ValidationError, ValidationErrors, ValidationErrorsKind};
use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct Audit {
    #[serde(rename = "createdBy")]
    created_by: String,
}

#[derive(Schematic, Serialize, Deserialize)]
#[serde(transparent)]
struct TransparentAudit {
    audit: Audit,
}

#[derive(Schematic, Serialize, Deserialize)]
struct Payload {
    #[serde(flatten)]
    inner: TransparentAudit,
}

#[test]
fn transparent_fields_remap_to_the_serde_wire_path() {
    let mut audit = ValidationErrors::new();
    audit.add("created_by", ValidationError::new("length"));
    let mut transparent = ValidationErrors::new();
    transparent.errors_mut().insert("audit".into(), ValidationErrorsKind::Struct(Box::new(audit)));
    let mut payload = ValidationErrors::new();
    payload.errors_mut().insert("inner".into(), ValidationErrorsKind::Struct(Box::new(transparent)));

    gotcha::validation::remap_to_wire_names::<Payload>(&mut payload);

    assert!(
        payload.errors().contains_key("createdBy"),
        "transparent flatten must drop both wrapper segments: {:?}",
        payload.errors()
    );
}
