use gotcha::api;

#[api(skip_response)]
async fn handler() -> String {
    "hello".into()
}

fn main() {}
