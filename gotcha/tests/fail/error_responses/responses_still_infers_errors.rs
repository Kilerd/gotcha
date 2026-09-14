use gotcha::{api, axum::response::{IntoResponse, Response}};

struct UndocumentedError;

impl IntoResponse for UndocumentedError {
    fn into_response(self) -> Response {
        gotcha::axum::http::StatusCode::NOT_FOUND.into_response()
    }
}

#[api(responses(response(status = 404)), drop_default)]
async fn handler() -> Result<String, UndocumentedError> {
    Err(UndocumentedError)
}

fn main() {}
