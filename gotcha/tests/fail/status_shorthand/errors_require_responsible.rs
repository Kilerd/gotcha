use gotcha::{
    api,
    axum::response::{IntoResponse, Response},
};

struct UndocumentedError;

impl IntoResponse for UndocumentedError {
    fn into_response(self) -> Response {
        gotcha::axum::http::StatusCode::NOT_FOUND.into_response()
    }
}

// The shorthand takes the body from the error type's own `Responsible` contract.
#[api(errors(404 = "Not found"))]
async fn handler() -> Result<String, UndocumentedError> {
    Ok(String::new())
}

fn main() {}
