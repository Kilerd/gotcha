#![cfg(feature = "openapi")]

#[test]
fn explicit_response_declarations_validate_streaming_options() {
    trybuild::TestCases::new().compile_fail("tests/fail/streaming_responses/*.rs");
}
