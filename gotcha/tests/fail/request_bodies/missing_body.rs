use gotcha::api;

#[api(request_body(content_type = "multipart/form-data"))]
async fn handler() -> String {
    String::new()
}

fn main() {}
