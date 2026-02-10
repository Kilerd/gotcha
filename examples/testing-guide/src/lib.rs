// Business application code using Gotcha framework

use gotcha::{
    api, ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter,
    Json, Path, Query, State, Schematic
};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ========== Application State ==========
#[derive(Clone)]
pub struct AppState {
    pub users: Arc<Mutex<Vec<User>>>,
    pub counter: Arc<Mutex<i32>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            users: Arc::new(Mutex::new(vec![
                User {
                    id: Uuid::new_v4(),
                    name: "Alice".to_string(),
                    email: "alice@example.com".to_string(),
                    age: 30,
                }
            ])),
            counter: Arc::new(Mutex::new(0)),
        }
    }
}

// ========== Data Models ==========
#[derive(Clone, Debug, Serialize, Deserialize, Schematic, PartialEq)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub age: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Schematic)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    pub age: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Schematic)]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub email: Option<String>,
    pub age: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Schematic)]
pub struct QueryParams {
    pub page: Option<usize>,
    pub size: Option<usize>,
    pub sort: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Schematic, PartialEq)]
pub struct ApiResponse<T: Schematic> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Schematic)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}

// ========== Handler Functions ==========

#[api(id = "health_check", group = "system")]
pub async fn health_check(State(ctx): State<GotchaContext<AppState, AppConfig>>) -> Json<ApiResponse<String>> {
    let counter = ctx.state.counter.lock().unwrap();
    Json(ApiResponse {
        success: true,
        message: "Service is healthy".to_string(),
        data: Some(format!("Counter: {}", *counter)),
    })
}

#[api(id = "list_users", group = "users")]
pub async fn list_users(
    State(ctx): State<GotchaContext<AppState, AppConfig>>,
    Query(params): Query<QueryParams>,
) -> Json<ApiResponse<Vec<User>>> {
    let users = ctx.state.users.lock().unwrap();
    let page = params.page.unwrap_or(1);
    let size = params.size.unwrap_or(10);

    let start = (page - 1) * size;
    let end = std::cmp::min(start + size, users.len());

    let paginated_users = if start < users.len() {
        users[start..end].to_vec()
    } else {
        vec![]
    };

    Json(ApiResponse {
        success: true,
        message: format!("Found {} users", paginated_users.len()),
        data: Some(paginated_users),
    })
}

#[api(id = "get_user", group = "users")]
pub async fn get_user(
    State(ctx): State<GotchaContext<AppState, AppConfig>>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<User>>, (StatusCode, Json<ErrorResponse>)> {
    let users = ctx.state.users.lock().unwrap();

    match users.iter().find(|u| u.id == user_id) {
        Some(user) => Ok(Json(ApiResponse {
            success: true,
            message: "User found".to_string(),
            data: Some(user.clone()),
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
                code: 404,
            }),
        )),
    }
}

#[api(id = "create_user", group = "users")]
pub async fn create_user(
    State(ctx): State<GotchaContext<AppState, AppConfig>>,
    Json(payload): Json<CreateUserRequest>,
) -> Json<ApiResponse<User>> {
    let mut users = ctx.state.users.lock().unwrap();
    let mut counter = ctx.state.counter.lock().unwrap();

    let new_user = User {
        id: Uuid::new_v4(),
        name: payload.name,
        email: payload.email,
        age: payload.age,
    };

    users.push(new_user.clone());
    *counter += 1;

    Json(ApiResponse {
        success: true,
        message: "User created successfully".to_string(),
        data: Some(new_user),
    })
}

#[api(id = "update_user", group = "users")]
pub async fn update_user(
    State(ctx): State<GotchaContext<AppState, AppConfig>>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<ApiResponse<User>>, (StatusCode, Json<ErrorResponse>)> {
    let mut users = ctx.state.users.lock().unwrap();

    match users.iter_mut().find(|u| u.id == user_id) {
        Some(user) => {
            if let Some(name) = payload.name {
                user.name = name;
            }
            if let Some(email) = payload.email {
                user.email = email;
            }
            if let Some(age) = payload.age {
                user.age = age;
            }

            Ok(Json(ApiResponse {
                success: true,
                message: "User updated successfully".to_string(),
                data: Some(user.clone()),
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
                code: 404,
            }),
        )),
    }
}

#[api(id = "delete_user", group = "users")]
pub async fn delete_user(
    State(ctx): State<GotchaContext<AppState, AppConfig>>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ErrorResponse>)> {
    let mut users = ctx.state.users.lock().unwrap();

    let initial_len = users.len();
    users.retain(|u| u.id != user_id);

    if users.len() < initial_len {
        Ok(Json(ApiResponse {
            success: true,
            message: "User deleted successfully".to_string(),
            data: Some(()),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
                code: 404,
            }),
        ))
    }
}

// ========== Application Configuration ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub app_name: String,
    pub version: String,
    pub max_users: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_name: "Testing Guide".to_string(),
            version: "1.0.0".to_string(),
            max_users: 1000,
        }
    }
}

// ========== Application Setup ==========
pub struct App;

impl GotchaApp for App {
    type State = AppState;
    type Config = AppConfig;

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(AppState::default())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>)
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
            .get("/health", health_check)
            .get("/users", list_users)
            .get("/users/:id", get_user)
            .post("/users", create_user)
            .put("/users/:id", update_user)
            .delete("/users/:id", delete_user)
    }
}

// ========== Helper function for testing ==========
pub async fn create_test_app() -> axum::Router {
    use gotcha::config::BasicConfig;

    let app = App;
    let config = ConfigWrapper {
        basic: BasicConfig::default(),
        application: AppConfig::default(),
    };

    let state = app.state(&config).await.unwrap();
    let context = GotchaContext {
        state,
        config,
    };

    app.build_router(context).await.unwrap()
}