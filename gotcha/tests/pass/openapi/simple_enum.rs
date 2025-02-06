use gotcha::{Schematic, oas::Schema};


#[derive(Debug, Schematic)]
pub enum MyType {
    One,
    Two,
    /// three
    Three
}

fn main() {
    let schema = MyType::generate_schema();
    assert!(MyType::name().eq("MyType"));
    assert!(MyType::type_().eq("string"));
    assert!(schema.schema.extras.get("enum").unwrap().as_array().unwrap().len() == 3);
}