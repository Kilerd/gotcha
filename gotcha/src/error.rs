use thiserror::Error;

#[derive(Error, Debug)]
pub enum GotchaError {
    #[error("Config error: {0}")]
    ConfigError(String),
}
