use gotcha::prelude::*;
use gotcha::GotchaService;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
struct App;

#[derive(Clone)]
struct AppState {}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct AppConfig {}

#[derive(Debug, Serialize, Deserialize, Schematic)]
struct UsageQuery {
    start: Option<String>,
    end: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
struct UsageStats {
    user_id: Uuid,
    total_requests: u64,
    total_tokens: u64,
    total_cost: f64,
    by_model: Vec<ModelStats>,
    by_provider: Vec<ProviderStats>,
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
struct ModelStats {
    model: String,
    requests: u64,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cost: f64,
}

#[derive(Debug, Serialize, Deserialize, Schematic)]
struct ProviderStats {
    provider: String,
    requests: u64,
    total_tokens: u64,
    cost: f64,
}

#[gotcha::api]
async fn get_usage_stats(
    Path(user_id): Path<Uuid>,
    Query(_query): Query<UsageQuery>,
) -> Result<Json<UsageStats>, StatusCode> {
    // Mock implementation
    Ok(Json(UsageStats {
        user_id,
        total_requests: 100,
        total_tokens: 50000,
        total_cost: 25.0,
        by_model: vec![
            ModelStats {
                model: "gpt-4".to_string(),
                requests: 50,
                prompt_tokens: 20000,
                completion_tokens: 5000,
                total_tokens: 25000,
                cost: 20.0,
            },
        ],
        by_provider: vec![
            ProviderStats {
                provider: "openai".to_string(),
                requests: 100,
                total_tokens: 50000,
                cost: 25.0,
            },
        ],
    }))
}

#[gotcha::api]
async fn get_user_by_id(
    Path(id): Path<Uuid>,
) -> String {
    format!("User ID: {}", id)
}

// Test with tuple syntax (this should already work)
#[gotcha::api]
async fn get_user_tuple(
    Path((id,)): Path<(Uuid,)>,
) -> String {
    format!("User ID from tuple: {}", id)
}

impl GotchaApp for App {
    type State = AppState;
    type Config = AppConfig;

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(AppState {})
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
            .get("/admin/users/:user_id/usage", get_usage_stats)
            .get("/users/:id", get_user_by_id)
            .get("/users-tuple/:id", get_user_tuple)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App;
    app.run().await?;
    Ok(())
}