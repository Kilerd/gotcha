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

#[test]
fn test_decimal_schema_generation() {
    let schema = Product::generate_schema();
    let json = serde_json::to_string_pretty(&schema).unwrap();

    let expected = include_str!("rust_decimal.json");
    assert_json_diff::assert_json_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap(),
        serde_json::from_str::<serde_json::Value>(expected).unwrap()
    );
}