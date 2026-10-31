use ::schema;

#[path = "../../schemas.rs"]
pub mod schemas;

#[derive(schema::Schematic)]
pub struct ApplicationRecord {
    pub id: schema_library::LibraryId,
    pub data: schema_library::Record<u32>,
}

#[derive(schema::Validate)]
#[validate(crate = "schema::validator")]
pub struct Request {
    #[validate(length(min = 1))]
    name: String,
}

#[test]
fn library_types_satisfy_the_application_trait() {
    let fields = <ApplicationRecord as schema::Schematic>::fields();
    assert_eq!(fields[0].1.schema.to_value()["type"], "string");
    assert_eq!(fields[1].1.schema.to_value()["type"], "object");
    assert!(schema::Validate::validate(&Request { name: String::new() }).is_err());
}

#[test]
fn cargo_info_resolves_through_a_renamed_dependency() {
    let info = schema::cargo_info!();
    assert_eq!(info.title, "renamed-derive-application");
    assert_eq!(info.version, "0.0.0");
    assert_eq!(info.description, None);
}
