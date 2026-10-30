use gotcha::api;

#[api(responses(response(status = 200, item = "String")))]
async fn handler() -> String {
    "hello".into()
}

fn main() {}
