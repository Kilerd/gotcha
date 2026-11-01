use gotcha::api;

#[api(responses(response(status = 99999)))]
async fn handler() -> String {
    String::new()
}

fn main() {}
