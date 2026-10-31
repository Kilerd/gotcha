use gotcha::api;

#[api(skip_response, drop_default, responses(response(status = 200)))]
async fn handler() -> String {
    "hello".into()
}

fn main() {}
