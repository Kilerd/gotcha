use gotcha::api;

#[api(responses(response(status = 204, content_type = "text/event-stream", item = "String")))]
async fn handler() -> String {
    "hello".into()
}

fn main() {}
