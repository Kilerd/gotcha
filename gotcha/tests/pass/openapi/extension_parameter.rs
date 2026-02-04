//! Test that Extension parameters are properly handled in OpenAPI generation

use gotcha::{api, Extension, Json, Schematic};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AuthContext {
    user_id: String,
}

#[derive(Serialize, Deserialize, Schematic)]
struct Request {
    message: String,
}

#[derive(Serialize, Deserialize, Schematic)]
struct Response {
    message: String,
}

/// Test endpoint with Extension parameter
#[api(id = "test_extension", group = "test")]
async fn handler_with_extension(
    Extension(_auth): Extension<AuthContext>,
    Json(body): Json<Request>,
) -> Json<Response> {
    Json(Response {
        message: body.message,
    })
}

/// Test endpoint with multiple Extension parameters
#[api(id = "test_multiple_extensions", group = "test")]
async fn handler_with_multiple_extensions(
    Extension(_auth): Extension<AuthContext>,
    Extension(_config): Extension<String>,
    Json(body): Json<Request>,
) -> Json<Response> {
    Json(Response {
        message: body.message,
    })
}

fn main() {
    // This test verifies that Extension parameters compile correctly with the #[api] macro
    // The fact that this compiles is the test - Extension<T> implements ParameterProvider
    // with an empty implementation, so it doesn't generate any OpenAPI parameters
    println!("Extension parameters compile successfully with #[api] macro");
}