use gotcha::{api, Json};

#[derive(serde::Serialize)]
struct NoSchema;

#[api(errors(response(status = 404)))]
async fn handler() -> Result<Json<NoSchema>, gotcha::axum::http::StatusCode> {
    Ok(Json(NoSchema))
}

fn main() {}
