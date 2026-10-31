use gotcha::api;

#[api(
    skip_response,
    errors(response(status = 404)),
    responses(response(status = 200))
)]
async fn handler() -> String {
    "hello".into()
}

fn main() {}
