#![feature(async_await)]
use gotcha::*;


struct CORS;

impl Middleware for CORS {

}

async fn hello_world() -> impl Responder {
    String::from("hello world")
}

fn main() {
    App::new()
        .data(String::new())
        .middleware(CORS)
        .default_service(hello_world)
        .run(("127.0.0.1", 8000))
        .expect("cannot start App")
}
