use gotcha::api;

#[api(
    request_body(body = "String", required = true),
    request_body(body = "String", content_type = "text/plain", required = false)
)]
async fn handler() -> String {
    String::new()
}

fn main() {}
