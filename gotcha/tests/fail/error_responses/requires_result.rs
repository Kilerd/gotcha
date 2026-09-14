use gotcha::api;

#[api(errors(response(status = 404)))]
async fn handler() -> String {
    "hello".into()
}

fn main() {}
