use gotcha::api;

#[api(
    request_body(body = "String", description = "one"),
    request_body(body = "String", content_type = "text/plain", description = "two")
)]
async fn handler() -> String {
    String::new()
}

fn main() {}
