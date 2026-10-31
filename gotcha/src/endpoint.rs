//! Validation shared by the endpoints the framework mounts itself.

use crate::{GotchaError, GotchaResult};

/// Check that `path` is a literal, absolute URI path that `Router::route` can mount.
///
/// `kind` names the endpoint in the error message (`"OpenAPI endpoint"`, `"metrics endpoint"`).
pub(crate) fn validate_path(kind: &str, path: &str) -> GotchaResult<()> {
    let parsed = path.parse::<axum::http::uri::PathAndQuery>();
    if !path.starts_with('/')
        || path.starts_with("//")
        || !path.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@%".contains(&byte))
        || path.split('/').any(|segment| {
            // Browsers normalize literal and percent-encoded dot segments before requesting a URL.
            let decoded = segment.replace("%2e", ".").replace("%2E", ".");
            segment.starts_with([':', '*']) || matches!(decoded.as_str(), "." | "..")
        })
        || !matches!(parsed, Ok(parsed) if parsed.path() == path && parsed.query().is_none())
    {
        return Err(GotchaError::message(format!("{kind} must be a literal absolute URI path: {path:?}")));
    }
    Ok(())
}
