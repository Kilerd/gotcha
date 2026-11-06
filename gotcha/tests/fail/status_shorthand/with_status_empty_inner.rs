use std::convert::Infallible;

use gotcha::{api, WithStatus};

// `WithStatus<Infallible, 201>` documents nothing: `with_status` has no inner response to move
// to 201, so the shorthand has nothing to reword.
#[api(responses(201 = "Created"))]
async fn handler() -> WithStatus<Infallible, 201> {
    loop {}
}

fn main() {}
