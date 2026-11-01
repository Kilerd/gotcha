use gotcha::api;

#[api(responses(response(status = 200, body = "Vec<")))]
async fn handler() -> String {
    String::new()
}

fn main() {}
