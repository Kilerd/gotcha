//! A single-field tuple struct (newtype) derives `Schematic` transparently:
//! it produces exactly the wrapped type's schema. Previously this panicked the
//! macro (`field.ident.unwrap()` on an unnamed field).

use gotcha::Schematic;

#[derive(Schematic)]
struct UserId(uuid::Uuid);

#[derive(Schematic)]
struct Account {
    id: UserId,
    balance: i64,
}

fn main() {
    // The newtype is transparent to its inner type.
    let newtype = serde_json::to_value(UserId::generate_schema().schema).unwrap();
    let inner = serde_json::to_value(<uuid::Uuid as Schematic>::generate_schema().schema).unwrap();
    assert_eq!(newtype, inner, "newtype schema should equal the inner type's schema");

    // And it composes as a struct field.
    let account = serde_json::to_value(Account::generate_schema().schema).unwrap();
    let props = &account["properties"];
    assert_eq!(props["id"]["type"], "string");
    assert_eq!(props["id"]["format"], "uuid");
    assert_eq!(props["balance"]["type"], "integer");

    let required = account["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "id"));
    assert!(required.iter().any(|v| v == "balance"));
}
