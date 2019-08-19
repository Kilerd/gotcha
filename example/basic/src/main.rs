use gotcha::App;
fn main() {
    App::new()
        .run(("127.0.0.1", 8000))
        .expect("cannot start App")
}
