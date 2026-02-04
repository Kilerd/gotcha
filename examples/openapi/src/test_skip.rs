use gotcha::{api, Json, Path, Query, Schematic};
use serde::{Deserialize, Serialize};

#[derive(Schematic, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: u32,
    pub name: String,
}

#[derive(Schematic, Serialize, Deserialize)]
pub struct FilterParams {
    pub active: Option<bool>,
    pub limit: Option<u32>,
}

#[derive(Schematic, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub description: String,
}

/// Test endpoint with skipped Json body parameter - only Path parameter should appear in OpenAPI
#[api(id = "test_skip_json", group = "test_skip")]
pub async fn test_skip_json(
    Path(key_id): Path<u32>,
    #[api(skip)] Json(body): Json<CreateApiKeyRequest>,  // This will be skipped in OpenAPI
) -> Json<ApiKey> {
    // The Json(body) parameter should be skipped in OpenAPI docs
    // Only Path(key_id) should appear in the generated OpenAPI
    Json(ApiKey {
        id: key_id,
        name: body.name,
    })
}

/// Test endpoint with skipped Query parameter - only Path and Json should appear in OpenAPI
#[api(id = "test_skip_query", group = "test_skip")]
pub async fn test_skip_query(
    Path(key_id): Path<u32>,
    #[api(skip)] Query(filter): Query<FilterParams>,  // This will be skipped in OpenAPI
    Json(body): Json<CreateApiKeyRequest>,
) -> Json<ApiKey> {
    // The Query(filter) parameter should be skipped in OpenAPI docs
    // Path(key_id) and Json(body) should appear in the generated OpenAPI
    Json(ApiKey {
        id: key_id,
        name: format!("{} (limit: {:?})", body.name, filter.limit),
    })
}

/// Test endpoint without skip - all parameters visible in OpenAPI
#[api(id = "test_no_skip", group = "test_skip")]
pub async fn test_no_skip(
    Path(key_id): Path<u32>,
    Query(filter): Query<FilterParams>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Json<ApiKey> {
    // All parameters should appear in OpenAPI docs
    Json(ApiKey {
        id: key_id,
        name: format!("{} (limit: {:?})", body.name, filter.limit),
    })
}