use gotcha::{App, Middleware};


struct CORS;

impl Middleware for CORS {

}

fn main() {
    App::new()
        .data(String::new())
        .middleware(CORS)
        .run(("127.0.0.1", 8000))
        .expect("cannot start App")
}
