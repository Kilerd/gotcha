use gotcha::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let app = Gotcha::new()
        .with_env_config("APP")
        .with_optional_config("config.toml")
        .get("/", || async { "Configuration example working!" });
    
    app.listen("127.0.0.1:3000").await?;
    Ok(())
}
