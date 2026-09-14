use schema_library::schema;

// Neither gotcha nor gotcha_core is a direct dependency here. Ignoring the
// explicit path would fail instead of silently resolving the same core crate.
#[derive(schema::Schematic)]
#[schematic(crate = "schema")]
pub struct Response {
    #[schematic(example = 42, default = 0)]
    pub code: u32,
}

#[test]
fn custom_facade_resolves_the_trait_and_field_metadata() {
    let fields = <Response as schema::Schematic>::fields();
    assert_eq!(fields[0].1.schema.extras["example"], 42);
    assert_eq!(fields[0].1.schema.extras["default"], 0);
}
