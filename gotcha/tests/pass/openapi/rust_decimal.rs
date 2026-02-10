use gotcha::openapi::schematic::Schematic;
use gotcha_macro::Schematic;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Schematic)]
struct Product {
    name: String,
    price: Decimal,
    discount: Option<Decimal>,
}

fn main() {
    let schema = Product::generate_schema();
    let schema_value = schema.schema.to_value();

    let expected = include_str!("rust_decimal.json");
    assert_json_diff::assert_json_eq!(
        schema_value,
        serde_json::from_str::<serde_json::Value>(expected).unwrap()
    );
}