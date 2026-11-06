use gotcha::{
    api,
    axum::response::{IntoResponse, Response},
};

struct Error;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        gotcha::axum::http::StatusCode::NO_CONTENT.into_response()
    }
}

impl gotcha::Responsible for Error {
    const HAS_DEFAULT: bool = true;

    fn response() -> gotcha::oas::Responses {
        gotcha::response::default_response::<String>("application/json", "error")
    }
}

// The shorthand would move the error's JSON body onto a status that cannot carry content; the
// long form rejects the same declaration.
#[api(errors(204 = "No content"))]
async fn handler() -> Result<String, Error> {
    Ok(String::new())
}

fn main() {}
