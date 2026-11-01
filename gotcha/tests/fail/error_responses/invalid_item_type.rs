use gotcha::api;

#[api(skip_response, responses(response(status = 200, content_type = "text/event-stream", item = "Vec<")))]
async fn handler() -> String {
    String::new()
}

fn main() {}
