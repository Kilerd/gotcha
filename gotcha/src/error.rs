//! The unified error type for the Gotcha framework.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

use crate::config::ConfigError;

/// The error type returned by framework operations (`run`, `listen`, config
/// loading, …). It implements [`IntoResponse`], so it can also be returned
/// directly from handlers, where it renders as a `500 Internal Server Error`.
#[derive(Debug, Error)]
pub enum GotchaError {
    /// Configuration could not be loaded or parsed.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// The configured listen address was not a valid socket address.
    #[error("invalid listen address: {0}")]
    InvalidAddress(String),

    /// The server failed to bind to the given address.
    #[error("failed to bind server to {addr}: {source}")]
    Bind {
        /// The address the server tried to bind to.
        addr: String,
        /// Why the bind failed.
        source: std::io::Error,
    },

    /// An I/O error from the runtime (e.g. `axum::serve`).
    #[error(transparent)]
    Io(std::io::Error),

    /// A generic, framework-level error message.
    #[error("{0}")]
    Message(String),

    /// Any other error — typically produced by a user-provided hook.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl GotchaError {
    /// Build a [`GotchaError::Message`] from anything `Display`.
    pub fn message(msg: impl std::fmt::Display) -> Self {
        Self::Message(msg.to_string())
    }
}

/// Convenient result alias for framework operations.
pub type GotchaResult<T> = Result<T, GotchaError>;

impl IntoResponse for GotchaError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self, "request failed with a gotcha error");
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_as_internal_server_error() {
        let response = GotchaError::message("boom").into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn config_error_converts() {
        let err: GotchaError = ConfigError::Error("bad".into()).into();
        assert!(matches!(err, GotchaError::Config(_)));
    }
}
