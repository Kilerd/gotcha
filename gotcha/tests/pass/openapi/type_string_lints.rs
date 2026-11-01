//! A type string from a declaration is parsed into macro output whose tokens resolve at the
//! macro call site, so user-crate lints do not fire on code the user did not write.

#![deny(unused_qualifications)]

use gotcha::api;

#[api(request_body(body = "std::string::String"))]
async fn handler() -> String {
    String::new()
}

fn main() {}
