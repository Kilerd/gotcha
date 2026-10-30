#![cfg(feature = "openapi")]

#[test]
fn parameter_attributes_are_validated() {
    trybuild::TestCases::new().compile_fail("tests/fail/parameter_attributes/*.rs");
}
