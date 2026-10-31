use gotcha::api;

#[api(request_body(body = "String", item = "String"))]
async fn handler() -> String {
    String::new()
}

fn main() {}
