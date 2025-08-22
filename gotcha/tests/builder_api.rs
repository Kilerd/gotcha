//! Integration tests for the new Builder API

use gotcha::prelude::*;

// Test basic compilation and API usage
#[tokio::test]
async fn test_builder_api_compiles() {
    let _gotcha = Gotcha::new()
        .get("/", || async { "Hello World" })
        .get("/hello/:name", |Path(name): Path<String>| async move {
            format!("Hello, {}!", name)
        })
        .post("/echo", |body: String| async move {
            format!("Echo: {}", body)
        })
        .routes(|router| {
            router.get("/nested/ping", || async { "pong" })
        });
    
    // Test passes if it compiles
}

#[tokio::test]
async fn test_fluent_interface() {
    // Test that demonstrates the fluent interface
    let _gotcha = Gotcha::new()
        .host("0.0.0.0")
        .port(8080)
        .get("/", || async { "root" })
        .post("/create", || async { "created" })
        .put("/update", || async { "updated" })
        .delete("/delete", || async { "deleted" })
        .patch("/patch", || async { "patched" });
    
    // Test passes if it compiles
}

#[tokio::test]
async fn test_configuration() {
    // Test that configuration methods can be chained
    let _gotcha = Gotcha::new()
        .host("0.0.0.0")
        .port(8080);
    
    // Test passes if it compiles and chains correctly
}