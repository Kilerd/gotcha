use super::schema;

// Each consumer exposes its dependency as `schema`, without importing the trait.
// Reusing these declarations exercises the same expansion through core and gotcha.
#[derive(schema::Schematic)]
pub struct Record<T: schema::Schematic> {
    pub value: T,
    #[schematic(example = 42, default = 0, title = "Count")]
    pub count: u32,
}

#[derive(schema::Schematic)]
pub struct Id(pub u32);

#[derive(schema::Schematic)]
#[schematic(name = "NamedId")]
pub struct NamedId(pub u32);

#[derive(schema::Schematic)]
pub enum Choice {
    Empty,
    Item(Record<u32>),
    Named {
        #[schematic(example = true)]
        enabled: bool,
    },
}

#[derive(schema::Schematic)]
pub enum Status {
    Ready,
    Done,
}

#[test]
fn schemas_work_without_a_trait_import() {
    let fields = <Record<String> as schema::Schematic>::fields();
    assert_eq!(fields[0].1.schema._type.as_deref(), Some("string"));
    assert_eq!(fields[1].1.schema.extras["example"], 42);
    assert_eq!(fields[1].1.schema.extras["default"], 0);
    assert_eq!(<Id as schema::Schematic>::type_(), "integer");
    assert_eq!(<NamedId as schema::Schematic>::generate_schema().schema._type.as_deref(), Some("integer"));
    assert!(<Choice as schema::Schematic>::generate_schema().schema.extras.contains_key("oneOf"));
    assert!(<Status as schema::Schematic>::generate_schema().schema.extras.contains_key("enum"));
}
