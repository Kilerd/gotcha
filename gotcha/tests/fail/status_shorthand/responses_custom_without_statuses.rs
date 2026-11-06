use gotcha::{
    api,
    axum::response::{IntoResponse, Response},
};

struct Record;

impl IntoResponse for Record {
    fn into_response(self) -> Response {
        gotcha::axum::http::StatusCode::OK.into_response()
    }
}

// Documents a 200 response but does not list it in `Responsible::STATUSES`.
impl gotcha::Responsible for Record {
    fn response() -> gotcha::oas::Responses {
        gotcha::response::empty_response(200, "record")
    }
}

#[api(responses(200 = "The record"))]
async fn handler() -> Record {
    Record
}

fn main() {}
