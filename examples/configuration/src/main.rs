//! Demonstrates the configuration system: the application's own settings live at the top level of
//! `configurations/application.toml`, the framework's bind settings in the reserved `[server]`
//! section, and the server listens on whatever that section says.
//!
//! Try it:
//!
//! ```console
//! cargo run                                    # listens on the configured port (8000)
//! APP_SERVER__PORT=9000 cargo run              # `__` addresses a nested section
//! APP_WELCOME=hi cargo run                     # a single underscore stays part of the field name
//! GOTCHA_ACTIVE_PROFILE=development cargo run  # also loads application_development.toml
//! ```

use gotcha::prelude::*;

/// The application's own configuration — the top level of the file.
#[config]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Config {
    welcome: String,
}

/// The application's config extracts directly, thanks to `#[config]`.
async fn greet(State(config): State<Config>) -> impl Responder {
    config.welcome.clone()
}

/// The framework's own settings are a separate extractor, and need no attribute.
async fn where_am_i(State(server): State<ServerConfig>) -> impl Responder {
    format!("listening on {}:{}", server.host, server.port)
}

struct App;

impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/", greet).get("/server", where_am_i)
    }

    async fn state(&self, config: &ConfigWrapper<Self::Config>) -> GotchaResult<Self::State> {
        // `ConfigWrapper` derefs to the application config, so `config.welcome` reads the
        // application's own field without going through a wrapper level.
        gotcha::tracing::info!("loaded welcome message: {}", config.welcome);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `run()` reads `configurations/application.toml` and binds the address from `[server]`.
    App.run().await?;
    Ok(())
}
