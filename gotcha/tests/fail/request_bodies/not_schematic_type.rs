use gotcha::api;

#[api(request_body(body = "std::time::Duration"))]
async fn handler() -> String {
    String::new()
}

fn main() {}
