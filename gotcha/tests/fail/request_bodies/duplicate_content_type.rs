use gotcha::api;

#[api(
    request_body(body = "String"),
    request_body(body = "String", content_type = "application/json")
)]
async fn handler() -> String {
    String::new()
}

fn main() {}
