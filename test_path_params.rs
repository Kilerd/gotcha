// Test script to verify Path parameter fix
// This tests that Path<Uuid> parameters are properly included in OpenAPI JSON

use gotcha::prelude::*;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
struct UserResponse {
    id: Uuid,
    name: String,
}

#[gotcha::api]
async fn get_user_by_id(
    Path(id): Path<Uuid>,
) -> Json<UserResponse> {
    Json(UserResponse {
        id,
        name: "Test User".to_string(),
    })
}

#[tokio::main]
async fn main() {
    // Test 1: Simple type (Uuid) path parameter
    let url = "/users/:id".to_string();
    let params = <Path<Uuid> as gotcha::ParameterProvider>::generate(url);

    match params {
        gotcha::openapi::Either::Left(params) => {
            println!("✅ Path<Uuid> generates parameters:");
            for param in &params {
                println!("  - Parameter name: {}", param.name);
                println!("    Location: {:?}", param._in);
                println!("    Required: {}", param.required);
            }

            if params.is_empty() {
                println!("❌ ERROR: No parameters generated for Path<Uuid>");
            } else {
                println!("✅ SUCCESS: Path<Uuid> correctly generates {} parameter(s)", params.len());
            }
        }
        gotcha::openapi::Either::Right(_) => {
            println!("❌ ERROR: Path<Uuid> should generate parameters, not request body");
        }
    }

    // Test 2: Tuple syntax (this should already work)
    let url2 = "/users/:id".to_string();
    let params2 = <Path<(Uuid,)> as gotcha::ParameterProvider>::generate(url2);

    match params2 {
        gotcha::openapi::Either::Left(params) => {
            println!("\n✅ Path<(Uuid,)> generates parameters:");
            for param in &params {
                println!("  - Parameter name: {}", param.name);
                println!("    Location: {:?}", param._in);
            }
        }
        gotcha::openapi::Either::Right(_) => {
            println!("❌ ERROR: Path<(Uuid,)> should generate parameters");
        }
    }

    println!("\nTest completed!");
}