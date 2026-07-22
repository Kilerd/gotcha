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

/// Configuration wrapper for backward compatibility
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ConfigWrapper<T: DeserializeOwned + Serialize + Default> {
    pub basic: BasicConfig,

    #[serde(bound = "", default)]
    pub application: T,
}

/// Basic server configuration
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct BasicConfig {
    pub host: String,
    pub port: u16,
}

impl Default for BasicConfig {
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
            basic: BasicConfig::default(),
            application: TestConfig::default(),
        };

        assert_eq!(wrapper.application.name, "");
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
