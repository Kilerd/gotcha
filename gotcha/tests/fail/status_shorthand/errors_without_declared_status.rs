use gotcha::{
    api,
    axum::response::{IntoResponse, Response},
};

struct Error;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        gotcha::axum::http::StatusCode::CONFLICT.into_response()
    }
}

// Documents one fixed status and no `default` response.
impl gotcha::Responsible for Error {
    const STATUSES: gotcha::response::Statuses = gotcha::response::Statuses::of(&[409]);

    fn response() -> gotcha::oas::Responses {
        gotcha::response::response::<String>(409, "application/json", "conflict")
    }
}

// 404 is neither declared by the error type nor covered by a default response.
#[api(errors(404 = "Not found"))]
async fn handler() -> Result<String, Error> {
    Ok(String::new())
}

fn main() {}
