//! Example demonstrating Extension<T> usage with OpenAPI generation

use gotcha::{
    api, async_trait, ConfigWrapper, Extension, GotchaApp, GotchaContext, GotchaRouter, Json,
    Responder, Schematic, State
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    pub app_name: String,
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
pub struct UserResponse {
    pub id: String,
    pub name: String,
    pub role: String,
}

/// Get current user information
#[api(id = "get_current_user", group = "users")]
pub async fn get_current_user(
    Extension(auth): Extension<AuthContext>,
    State(_config): State<ConfigWrapper<Config>>,
) -> Json<UserResponse> {
    Json(UserResponse {
        id: auth.user_id.clone(),
        name: format!("User {}", auth.user_id),
        role: auth.role,
    })
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
pub struct CreatePostRequest {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
pub struct PostResponse {
    pub id: String,
    pub title: String,
    pub content: String,
    pub author_id: String,
}

/// Create a new post
#[api(id = "create_post", group = "posts")]
pub async fn create_post(
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<CreatePostRequest>,
) -> Json<PostResponse> {
    Json(PostResponse {
        id: uuid::Uuid::new_v4().to_string(),
        title: request.title,
        content: request.content,
        author_id: auth.user_id,
    })
}

/// Health check endpoint without auth
#[api(id = "health", group = "system")]
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "healthy" }))
}

pub struct App {}

#[async_trait]
impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
            .get("/health", health)
            .get("/user/me", get_current_user)
            .post("/posts", create_post)
            // Add middleware to inject the AuthContext
            .layer(axum::middleware::from_fn(inject_auth_context))
    }

    fn state(&self, _config: &ConfigWrapper<Self::Config>) -> impl std::future::Future<Output = Result<Self::State, Box<dyn std::error::Error>>> + Send {
        async { Ok(()) }
    }
}

// Middleware to inject AuthContext into requests
async fn inject_auth_context(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl Responder {
    // In a real application, you would extract this from a JWT token or session
    let auth_context = AuthContext {
        user_id: "user123".to_string(),
        role: "admin".to_string(),
    };

    req.extensions_mut().insert(auth_context);
    next.run(req).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Extension OpenAPI example server...");
    println!("Visit http://localhost:3000/scalar for API documentation");
    println!("Visit http://localhost:3000/openapi.json for OpenAPI spec");
    println!();
    println!("Available endpoints:");
    println!("  GET  /health        - Health check (no auth)");
    println!("  GET  /user/me       - Get current user (uses Extension)");
    println!("  POST /posts         - Create post (uses Extension)");

    App {}.run().await?;
    Ok(())
}