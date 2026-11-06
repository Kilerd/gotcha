use std::convert::Infallible;

use gotcha::{api, axum::http::StatusCode};

// A runtime-status error type around an empty inner contract documents nothing at all, so the
// macro must reject the shorthand instead of panicking at document build.
#[api(errors(404 = "Not found"))]
async fn handler() -> Result<String, (StatusCode, Infallible)> {
    Ok(String::new())
}

fn main() {}
