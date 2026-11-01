use gotcha::api;

// Every misspelled parameter attribute is reported, not only the first one.
#[api]
async fn handler(#[api(skipper)] first: String, #[api(skip = true)] second: String) -> String {
    first + &second
}

fn main() {}
