use gotcha::api;

#[api(responses(response(status = -1)))]
async fn handler() -> String {
    String::new()
}

fn main() {}
