// The compiler lists the types implementing `Schematic`, which depends on the data-type features.
#![cfg(all(feature = "bigdecimal", feature = "chrono", feature = "rust_decimal", feature = "uuid"))]

#[test]
fn a_proxy_for_both_directions_must_implement_schematic() {
    trybuild::TestCases::new().compile_fail("tests/fail/proxied_schematic/*.rs");
}
