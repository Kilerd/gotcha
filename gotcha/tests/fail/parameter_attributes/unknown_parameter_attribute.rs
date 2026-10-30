use gotcha::api;

#[api]
async fn handler(#[api(skipper)] value: String) -> String {
    value
}

fn main() {}
