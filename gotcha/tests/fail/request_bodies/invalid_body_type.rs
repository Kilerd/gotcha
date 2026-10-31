use gotcha::api;

#[api(request_body(body = "Vec<"))]
async fn handler() -> String {
    String::new()
}

fn main() {}
