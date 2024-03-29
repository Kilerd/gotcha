use gotcha::{get};


#[get("/hello-world")]
async fn default_generated() -> String {
    "Hello world".to_string()
}
#[get("/hello-world", disable_openapi=false)]
async fn enabled() -> String {
    "Hello world".to_string()
}


#[get("/hello-world", disable_openapi)]
async fn disabled() -> String {
    "Hello world".to_string()
}

#[get("/hello-world", disable_openapi=true)]
async fn disabled_2() -> String {
    "Hello world".to_string()
}


fn main() {
    use gotcha::Operable;
    assert!(default_generated.should_generate_openapi_spec() == true);
    assert!(enabled.should_generate_openapi_spec() == true);
    assert!(disabled.should_generate_openapi_spec() == false);
    assert!(disabled_2.should_generate_openapi_spec() == false);
}