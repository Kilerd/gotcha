//! Simplified configuration system built on mofa

use std::path::{Path, PathBuf};

use mofa::{ConfigLoader, EnvironmentSource, FileSource, Source};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Simple configuration error
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration error: {0}")]
    /// A configuration source could not be read, parsed, or merged.
    Error(String),
}

/// Configuration result type
pub type ConfigResult<T> = Result<T, ConfigError>;

/// How application startup handles a failed configuration load.
/// Strict loading is the default for both the builder and [`crate::GotchaApp`].
#[derive(Default)]
pub enum ConfigErrorPolicy<C> {
    /// Return the loading error without initializing state, binding, or starting tasks.
    #[default]
    Strict,
    /// Log the loading error and use an explicitly chosen fallback configuration.
    Fallback(fn() -> ConfigWrapper<C>),
}

impl<C> ConfigErrorPolicy<C> {
    /// Opt into fallback to default server and application settings after a loading error.
    /// Only selecting this policy requires `C: Default`.
    pub fn fallback_to_default() -> Self
    where
        C: Default,
    {
        Self::Fallback(ConfigWrapper::default)
    }

    pub(crate) fn apply(self, result: crate::GotchaResult<ConfigWrapper<C>>) -> crate::GotchaResult<ConfigWrapper<C>> {
        match (result, self) {
            (Ok(config), _) => Ok(config),
            (Err(error), Self::Strict) => Err(error),
            (Err(error), Self::Fallback(fallback)) => {
                tracing::warn!("Failed to load configuration: {error}, using explicitly configured fallback");
                Ok(fallback())
            }
        }
    }
}

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
/// Merely storing or reading `T` adds no bounds. The derived serde, `Clone`, and `Default`
/// implementations require only the corresponding trait on `T`.
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ConfigWrapper<T> {
    /// Framework settings, from the reserved `[server]` section.
    #[serde(default)]
    pub server: ServerConfig,

    /// The application's own settings, living at the top level of the file.
    #[serde(flatten)]
    pub app: T,
}

impl<T> std::ops::Deref for ConfigWrapper<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.app
    }
}

/// Where the server binds, from the reserved `[server]` section.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct ServerConfig {
    /// Address the server binds to.
    pub host: String,
    /// Port the server listens on.
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

/// One configuration source, read when [`ConfigBuilder::build`] is called.
#[derive(Clone, Debug)]
pub enum ConfigSource {
    /// A TOML configuration file.
    File {
        /// Path to the file, resolved at build time.
        path: PathBuf,
        /// Whether a missing file is an error. Other read or parse errors always fail.
        required: bool,
    },
    /// Environment variables with the given prefix.
    Env {
        /// Prefix understood by mofa's environment source, such as `APP`.
        prefix: String,
    },
}

impl Source for ConfigSource {
    fn load(&self) -> Result<mofa::toml::Value, Box<dyn std::error::Error>> {
        match self {
            Self::Env { prefix } => EnvironmentSource::new(prefix).load(),
            Self::File { path, required } => match FileSource::new(path.clone()).load() {
                Ok(value) => Ok(value),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
                {
                    if *required {
                        Err(format!("required configuration file not found: {}", path.display()).into())
                    } else {
                        Ok(mofa::toml::Value::Table(Default::default()))
                    }
                }
                Err(error) => Err(format!("could not load configuration file {}: {error}", path.display()).into()),
            },
        }
    }
}

/// A reusable configuration description, not a snapshot of loaded values.
///
/// Sources are read in insertion order; later sources override earlier values. Cloning or
/// restoring this state preserves that order, file requirements, and variable substitution.
#[derive(Clone, Debug, Default)]
pub struct ConfigState {
    /// File and environment sources, in the order they were added.
    pub sources: Vec<ConfigSource>,
    /// Whether `${VAR}` substitution is enabled.
    pub enable_vars: bool,
}

/// Builds configuration from an ordered list of sources.
///
/// Registration does no file I/O. Files and environment variables are read by [`Self::build`].
#[derive(Clone)]
pub struct ConfigBuilder {
    state: ConfigState,
}

impl ConfigBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self { state: ConfigState::default() }
    }

    /// Add an environment source, overriding matching values from earlier sources.
    pub fn env(mut self, prefix: &str) -> Self {
        self.state.sources.push(ConfigSource::Env { prefix: prefix.to_string() });
        self
    }

    /// Add a required file source. Unlike [`ConfigBuilder::file_optional`], a
    /// missing file here causes `build()` to fail.
    pub fn file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.state.sources.push(ConfigSource::File {
            path: path.as_ref().to_path_buf(),
            required: true,
        });
        self
    }

    /// Add an optional file source. Only a missing file is ignored; read and parse errors fail.
    pub fn file_optional<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.state.sources.push(ConfigSource::File {
            path: path.as_ref().to_path_buf(),
            required: false,
        });
        self
    }

    /// Enable variable substitution
    pub fn enable_vars(mut self) -> Self {
        self.state.enable_vars = true;
        self
    }

    /// Read and merge sources in insertion order, then resolve variables and deserialize.
    ///
    /// Later sources override earlier matching values. File existence is checked here, so
    /// files created after registration are included and required files removed since then fail.
    pub fn build<T: for<'de> Deserialize<'de>>(self) -> ConfigResult<T> {
        let mut loader = ConfigLoader::new();
        for source in self.state.sources {
            loader.add_source(source);
        }
        if self.state.enable_vars {
            loader.enable_path_variable_processor();
            loader.enable_environment_variable_processor();
        }

        loader.construct().map_err(|e| ConfigError::Error(e.to_string()))
    }

    /// Copy the complete source description without loading any values.
    pub fn state(&self) -> ConfigState {
        self.state.clone()
    }

    /// Restore the source description without reordering or reading its sources.
    pub fn from_state(state: ConfigState) -> Self {
        Self { state }
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

    #[test]
    fn error_policy_only_constructs_a_fallback_after_a_loading_error() {
        let config = ConfigWrapper {
            server: ServerConfig::default(),
            app: String::from("loaded"),
        };
        let policy = ConfigErrorPolicy::Fallback(|| panic!("successful loading must not construct fallback values"));
        assert_eq!(policy.apply(Ok(config)).unwrap().app, "loaded");
        let failure = || Err(ConfigError::Error("invalid config".into()).into());
        assert!(ConfigErrorPolicy::<String>::Strict.apply(failure()).is_err());
        let fallback = ConfigErrorPolicy::<String>::fallback_to_default().apply(failure()).unwrap();
        assert_eq!(fallback.app, "");
        assert_eq!(fallback.server.port, 3000);
    }

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
