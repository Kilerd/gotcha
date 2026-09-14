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
    assert_eq!(fields[0].1.schema._type.as_deref(), Some("string"));
    assert_eq!(fields[1].1.schema._type.as_deref(), Some("object"));
    assert!(schema::Validate::validate(&Request { name: String::new() }).is_err());
}
