use gotcha::api;

// `skip_response` has no inferred response for the shorthand to reword.
#[api(skip_response, responses(200 = "hello"))]
async fn handler() -> String {
    String::new()
}

fn main() {}
