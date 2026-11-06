use gotcha::{api, Json};

// `Json<String>` infers only a 200 response, so a description-only override of 404 has
// nothing to reword.
#[api(responses(404 = "Not found"))]
async fn handler() -> Json<String> {
    Json(String::new())
}

fn main() {}
