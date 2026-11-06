use gotcha::{
    api,
    axum::{
        http::StatusCode,
        response::{IntoResponse, Response},
    },
};

struct LegacyError;

impl IntoResponse for LegacyError {
    fn into_response(self) -> Response {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

// A 0.5.0-style custom error: it documents a `default` response but predates
// `Responsible::HAS_DEFAULT`.
impl gotcha::Responsible for LegacyError {
    fn response() -> gotcha::oas::Responses {
        gotcha::response::default_response::<String>("application/json", "error")
    }
}

// `(StatusCode, T)` derives `HAS_DEFAULT` from `T`, so the hint must point at the inner
// `LegacyError`, not at the tuple.
#[api(errors(404 = "Not found"))]
async fn handler() -> Result<String, (StatusCode, LegacyError)> {
    Ok(String::new())
}

fn main() {}
