//! Simplified configuration system built on mofa

use std::path::{Path, PathBuf};

use mofa::{ConfigLoader, EnvironmentSource, FileSource};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Simple configuration error
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration error: {0}")]
    Error(String),
}

/// Configuration result type
pub type ConfigResult<T> = Result<T, ConfigError>;

/// The loaded configuration: the application's own settings plus the framework's.
///
/// The application's settings are **flattened to the top level** of the file, so they read as the
/// primary content and the framework's own settings sit in a reserved `[server]` section:
///
/// ```toml
/// name = "my-app"
/// database_url = "postgres://localhost/app"
///
/// [server]
/// host = "0.0.0.0"
/// port = 8080
/// ```
///
/// This derefs to the application config, so `config.name` reads the application's field directly
/// rather than going through a wrapper level. Handlers usually skip the wrapper entirely and
/// extract `State<YourConfig>` — see the `#[config]` attribute.
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ConfigWrapper<T: DeserializeOwned + Serialize + Default> {
    /// Framework settings, from the reserved `[server]` section.
    #[serde(default)]
    pub server: ServerConfig,

    /// The application's own settings, living at the top level of the file.
    #[serde(bound = "", flatten)]
    pub app: T,
}

impl<T: DeserializeOwned + Serialize + Default> std::ops::Deref for ConfigWrapper<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.app
    }
}

/// Where the server binds, from the reserved `[server]` section.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }
    }
}

/// Simple configuration builder state
#[derive(Clone, Debug, Default)]
pub struct ConfigState {
    pub file_paths: Vec<PathBuf>,
    pub env_prefixes: Vec<String>,
    pub enable_vars: bool,
}

/// Simple configuration builder
pub struct ConfigBuilder {
    loader: ConfigLoader,
    state: ConfigState,
    /// Required files (added via [`ConfigBuilder::file`]) that did not exist;
    /// reported as an error at `build()` time.
    missing_required: Vec<PathBuf>,
}

impl ConfigBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            loader: ConfigLoader::new(),
            state: ConfigState::default(),
            missing_required: Vec::new(),
        }
    }

    /// Add environment source
    pub fn env(mut self, prefix: &str) -> Self {
        self.state.env_prefixes.push(prefix.to_string());
        self.loader.add_source(EnvironmentSource::new(prefix));
        self
    }

    /// Add a required file source. Unlike [`ConfigBuilder::file_optional`], a
    /// missing file here causes `build()` to fail.
    pub fn file<P: AsRef<Path>>(mut self, path: P) -> Self {
        let path = path.as_ref().to_path_buf();
        self.state.file_paths.push(path.clone());
        if path.exists() {
            self.loader.add_source(FileSource::new(path));
        } else {
            self.missing_required.push(path);
        }
        self
    }

    /// Add optional file source
    pub fn file_optional<P: AsRef<Path>>(mut self, path: P) -> Self {
        let path = path.as_ref().to_path_buf();
        self.state.file_paths.push(path.clone());
        if path.exists() {
            self.loader.add_source(FileSource::new(path));
        }
        self
    }

    /// Enable variable substitution
    pub fn enable_vars(mut self) -> Self {
        self.state.enable_vars = true;
        self.loader.enable_path_variable_processor();
        self.loader.enable_environment_variable_processor();
        self
    }

    /// Build configuration
    pub fn build<T: for<'de> Deserialize<'de>>(mut self) -> ConfigResult<T> {
        if !self.missing_required.is_empty() {
            return Err(ConfigError::Error(format!(
                "required configuration file(s) not found: {:?}",
                self.missing_required
            )));
        }
        if self.state.enable_vars {
            self.loader.enable_path_variable_processor();
            self.loader.enable_environment_variable_processor();
        }

        self.loader.construct().map_err(|e| ConfigError::Error(e.to_string()))
    }

    /// Get builder state for cloning
    pub fn state(&self) -> ConfigState {
        self.state.clone()
    }

    /// Create builder from state
    pub fn from_state(state: ConfigState) -> Self {
        let mut builder = Self::new();

        // Re-add sources
        for prefix in &state.env_prefixes {
            builder = builder.env(prefix);
        }
        for path in &state.file_paths {
            builder = builder.file_optional(path);
        }
        if state.enable_vars {
            builder = builder.enable_vars();
        }

        builder
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple config loader
pub struct Config;

impl Config {
    /// Create new builder
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    /// Load with defaults
    pub fn load_default<T: for<'de> Deserialize<'de> + Default>() -> T {
        Self::builder()
            .file_optional("configurations/application.toml")
            .env("APP")
            .enable_vars()
            .build()
            .unwrap_or_else(|_| T::default())
    }
}

/// Legacy loader for backward compatibility
pub struct GotchaConfigLoader;

impl GotchaConfigLoader {
    /// Load configuration from `configurations/application.toml`, then the
    /// profile-specific `configurations/application_{profile}.toml` (if a profile
    /// is given), then the `APP` environment prefix. Returns an error instead of
    /// panicking on failure.
    pub fn load<T: for<'de> Deserialize<'de>>(profile: Option<String>) -> ConfigResult<T> {
        let mut builder = Config::builder().file_optional("configurations/application.toml");
        if let Some(profile) = profile {
            builder = builder.file_optional(format!("configurations/application_{profile}.toml"));
        }
        builder.env("APP").enable_vars().build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Default, Debug, Clone)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    #[test]
    fn test_config_builder() {
        let _result: Result<TestConfig, _> = Config::builder().env("TEST").enable_vars().build();
        // Should not panic
    }

    #[test]
    fn test_config_wrapper() {
        let wrapper = ConfigWrapper {
            server: ServerConfig::default(),
            app: TestConfig::default(),
        };

        assert_eq!(wrapper.app.name, "");
        // Deref reaches the application config without going through a wrapper level.
        assert_eq!(wrapper.name, "");
    }

    #[test]
    fn application_settings_live_at_the_top_level() {
        // The application's own keys sit at the top level of the file; only the framework's
        // settings are nested, under the reserved `[server]` section.
        let dir = std::env::temp_dir().join("gotcha-config-shape");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("application.toml");
        std::fs::write(&path, "name = \"my-app\"\nvalue = 42\n\n[server]\nhost = \"0.0.0.0\"\nport = 9000\n").unwrap();

        let config: ConfigWrapper<TestConfig> = Config::builder().file(&path).build().expect("loads");

        assert_eq!(config.name, "my-app", "application keys read directly");
        assert_eq!(config.value, 42, "non-string types survive flattening");
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
    }

    #[test]
    fn server_section_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join("gotcha-config-noserver");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("application.toml");
        std::fs::write(&path, "name = \"only-app\"\nvalue = 1\n").unwrap();

        let config: ConfigWrapper<TestConfig> = Config::builder().file(&path).build().expect("loads");

        assert_eq!(config.name, "only-app");
        assert_eq!(config.server.port, ServerConfig::default().port, "a missing [server] uses defaults");
    }

    /// Environment overrides: `__` separates path segments, so a single underscore is free for
    /// snake_case field names, and a typed field accepts the (necessarily string) env value.
    ///
    /// Serialized because it mutates process-wide environment and working directory.
    #[test]
    fn environment_overrides_typed_and_snake_case_fields() {
        #[derive(Serialize, Deserialize, Default, Debug, Clone)]
        struct App {
            name: String,
            database_url: String,
            max_connections: u32,
        }

        let dir = std::env::temp_dir().join("gotcha-config-env");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("application.toml");
        std::fs::write(
            &path,
            "name = \"from-file\"\ndatabase_url = \"from-file\"\nmax_connections = 1\n\n[server]\nport = 3000\nhost = \"127.0.0.1\"\n",
        )
        .unwrap();

        std::env::set_var("GOTCHATEST_NAME", "from-env");
        std::env::set_var("GOTCHATEST_DATABASE_URL", "postgres://env");
        std::env::set_var("GOTCHATEST_MAX_CONNECTIONS", "99");
        std::env::set_var("GOTCHATEST_SERVER__PORT", "9090");

        let config: ConfigWrapper<App> = Config::builder().file(&path).env("GOTCHATEST").build().expect("loads");

        for key in [
            "GOTCHATEST_NAME",
            "GOTCHATEST_DATABASE_URL",
            "GOTCHATEST_MAX_CONNECTIONS",
            "GOTCHATEST_SERVER__PORT",
        ] {
            std::env::remove_var(key);
        }

        assert_eq!(config.name, "from-env");
        // A single underscore stays part of the field name rather than becoming a path separator.
        assert_eq!(config.database_url, "postgres://env");
        // A typed field accepts the env string and parses it.
        assert_eq!(config.max_connections, 99);
        // `__` addresses a nested section.
        assert_eq!(config.server.port, 9090);
    }

    #[test]
    fn required_missing_file_fails_to_build() {
        let result: ConfigResult<TestConfig> = Config::builder().file("definitely-does-not-exist-abc123.toml").build();
        assert!(result.is_err(), "a missing required file must fail the build");
    }

    #[test]
    fn optional_missing_file_is_not_a_required_error() {
        // A missing optional file must not trigger the required-file error (the config may
        // still fail to deserialize for other reasons, but not because of this file).
        let result: ConfigResult<TestConfig> = Config::builder().file_optional("definitely-does-not-exist-abc123.toml").build();
        if let Err(ConfigError::Error(msg)) = &result {
            assert!(!msg.contains("required configuration file"), "optional file wrongly treated as required: {msg}");
        }
    }
}
