//! A Cargo-renamed `gotcha` dependency without the `openapi` feature.
//!
//! `Schematic`, its derive, `Valid`, and the wire-name remapping must all be available, and the
//! derive's generated paths must resolve through the renamed crate.

use schema::{Json, Schematic, Valid, Validate};

#[derive(serde::Deserialize, Schematic, Validate)]
#[validate(crate = "schema::validator")]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    #[validate(length(min = 1))]
    first_name: String,
}

/// Compiles without `openapi`: `Valid`'s bound is `Schematic`, which every build provides.
pub async fn create(Valid(Json(payload)): Valid<Json<Payload>>) -> String {
    payload.first_name
}

#[test]
fn the_derive_and_wire_table_work_without_openapi() {
    assert_eq!(<Payload as Schematic>::type_(), "object");
    assert_eq!(Payload::wire_fields()[0].rust, "first_name");
    assert_eq!(Payload::wire_fields()[0].wire, "firstName");
}

#[test]
fn validation_errors_remap_to_serde_names_without_openapi() {
    let mut errors = schema::validator::ValidationErrors::new();
    errors.add("first_name", schema::validator::ValidationError::new("length"));
    schema::validation::remap_to_wire_names::<Payload>(&mut errors);
    assert!(errors.errors().contains_key("firstName"));
    assert!(!errors.errors().contains_key("first_name"));
}
