//! Simplified configuration system built on mofa

use std::path::{Path, PathBuf};

/// TOML values held by [`ConfigSource::Defaults`], re-exported from mofa.
pub use mofa::toml;
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
    /// Typed default values, merged beneath every file and environment source.
    ///
    /// Keeping real TOML types here lets mofa coerce environment strings against them, so
    /// `APP_SERVER__PORT=9000` reaches a `u16` even when no file supplies `server.port`.
    Defaults {
        /// The default values.
        value: mofa::toml::Value,
    },
}

impl Source for ConfigSource {
    fn load(&self) -> Result<mofa::toml::Value, Box<dyn std::error::Error>> {
        match self {
            Self::Env { prefix } => EnvironmentSource::new(prefix).load(),
            Self::Defaults { value } => Ok(value.clone()),
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
/// restoring this state preserves that order, file requirements, variable substitution, and
/// typed defaults. Defaults always stay beneath files and environment variables.
#[derive(Clone, Debug, Default)]
pub struct ConfigState {
    /// Configuration sources, in the order they were added.
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

    /// Add typed default values beneath every registered source.
    ///
    /// Defaults are merged first, in the order they are added, so files and environment
    /// variables override them. Registering real types (for example a port stored as an
    /// integer) also lets environment variables override typed fields without a
    /// configuration file: `APP_SERVER__PORT=9000` coerces against the default `u16`.
    ///
    /// Returns an error when `value` cannot be represented as TOML.
    ///
    /// ```rust
    /// use gotcha::config::{ConfigBuilder, ServerConfig};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct Defaults {
    ///     database_url: String,
    ///     server: ServerConfig,
    /// }
    ///
    /// let builder = ConfigBuilder::new()
    ///     .defaults(Defaults {
    ///         database_url: "postgres://localhost/app".into(),
    ///         server: ServerConfig { host: "0.0.0.0".into(), port: 8700 },
    ///     })
    ///     .expect("defaults serialize")
    ///     .env("APP");
    /// ```
    pub fn defaults<V: Serialize>(mut self, value: V) -> ConfigResult<Self> {
        let value = mofa::toml::Value::try_from(value).map_err(|error| ConfigError::Error(format!("could not serialize configuration defaults: {error}")))?;
        self.push_defaults(value);
        Ok(self)
    }

    /// Insert a defaults source above earlier defaults and below every other source.
    fn push_defaults(&mut self, value: mofa::toml::Value) {
        let index = self
            .state
            .sources
            .iter()
            .position(|source| !matches!(source, ConfigSource::Defaults { .. }))
            .unwrap_or(self.state.sources.len());
        self.state.sources.insert(index, ConfigSource::Defaults { value });
    }

    /// Create a builder with the framework's typed server defaults beneath every other source.
    ///
    /// Framework-managed loading starts here, so files and environment variables always have
    /// typed `server.host` and `server.port` values to override and coerce against. A manually
    /// assembled loader, such as a custom [`GotchaApp::config`](crate::GotchaApp::config)
    /// implementation, should also start here: without a typed base, mofa cannot coerce
    /// `APP_SERVER__PORT` to a port number when no file supplies `server.port`.
    ///
    /// ```rust,no_run
    /// use gotcha::config::{ConfigBuilder, ConfigWrapper};
    ///
    /// let config: ConfigWrapper<gotcha::EmptyConfig> = ConfigBuilder::framework_defaults()
    ///     .file_optional("configurations/application.toml")
    ///     .env("APP")
    ///     .enable_vars()
    ///     .build()
    ///     .expect("configuration loads");
    /// ```
    pub fn framework_defaults() -> Self {
        let mut builder = Self::new();
        builder.push_defaults(framework_default_values());
        builder
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

/// The framework's own settings as typed TOML values.
fn framework_default_values() -> mofa::toml::Value {
    #[derive(Serialize)]
    struct FrameworkDefaults {
        server: ServerConfig,
    }
    mofa::toml::Value::try_from(FrameworkDefaults {
        server: ServerConfig::default(),
    })
    .expect("server defaults contain only a string and an integer")
}

/// Simple config loader
pub struct Config;

impl Config {
    /// Create new builder
    ///
    /// This does not install the framework's typed server defaults. Use
    /// [`ConfigBuilder::framework_defaults`] for a loader that also coerces environment-only
    /// server settings.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    /// Load `configurations/application.toml` and the `APP` environment prefix, with variable
    /// substitution, above the framework's typed server defaults.
    ///
    /// Returns the loading error instead of falling back to `T::default()`. For an explicit
    /// fallback, choose it at the call site or use
    /// [`ConfigErrorPolicy::fallback_to_default`](crate::ConfigErrorPolicy::fallback_to_default)
    /// in startup.
    pub fn load_default<T: for<'de> Deserialize<'de>>() -> ConfigResult<T> {
        ConfigBuilder::framework_defaults()
            .file_optional("configurations/application.toml")
            .env("APP")
            .enable_vars()
            .build()
    }
}

/// Legacy loader for backward compatibility
pub struct GotchaConfigLoader;

impl GotchaConfigLoader {
    /// Load configuration from `configurations/application.toml`, then the
    /// profile-specific `configurations/application_{profile}.toml` (if a profile
    /// is given), then the `APP` environment prefix. Returns an error instead of
    /// panicking on failure.
    ///
    /// The framework's typed server defaults sit beneath these sources, so environment-only
    /// configuration such as `APP_SERVER__PORT` is coerced to the field's type.
    pub fn load<T: for<'de> Deserialize<'de>>(profile: Option<String>) -> ConfigResult<T> {
        let mut builder = ConfigBuilder::framework_defaults().file_optional("configurations/application.toml");
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
    #[serde(default)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    /// Run `name` in a child process whose working directory is an empty temporary directory
    /// and whose `APP_*` environment is removed. Returns `true` in the child. This keeps
    /// default file paths and environment sources isolated from tests running in parallel.
    fn in_isolated_child(name: &str, variables: &[(&str, &str)]) -> bool {
        const CHILD: &str = "GOTCHA_CONFIG_TESTS_CHILD";
        if std::env::var_os(CHILD).is_some() {
            return true;
        }

        let dir = tempfile::tempdir().unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("APP_")) {
            command.env_remove(key);
        }
        let output = command
            .args(["--exact", name, "--nocapture"])
            .current_dir(dir.path())
            .env(CHILD, "1")
            .envs(variables.iter().copied())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        false
    }

    /// Both the trait's default `config()` and builder automatic loading go through this loader.
    #[test]
    fn environment_only_server_settings_load_without_a_configuration_file() {
        if !in_isolated_child(
            "config::tests::environment_only_server_settings_load_without_a_configuration_file",
            &[("APP_SERVER__PORT", "9000")],
        ) {
            return;
        }

        // No `configurations/application.toml` exists in the child's working directory.
        let config: ConfigWrapper<TestConfig> = GotchaConfigLoader::load(None).expect("environment-only configuration loads");

        assert_eq!(config.server.port, 9000, "the typed default lets the environment value coerce");
        assert_eq!(config.server.host, ServerConfig::default().host, "unset fields keep their defaults");
    }

    #[test]
    fn files_and_environment_keep_precedence_over_framework_defaults() {
        if !in_isolated_child(
            "config::tests::files_and_environment_keep_precedence_over_framework_defaults",
            &[("APP_SERVER__PORT", "9000")],
        ) {
            return;
        }

        std::fs::create_dir_all("configurations").unwrap();
        std::fs::write(
            "configurations/application.toml",
            "name = \"file\"\n\n[server]\nhost = \"0.0.0.0\"\nport = 18080\n",
        )
        .unwrap();

        let config: ConfigWrapper<TestConfig> = GotchaConfigLoader::load(None).expect("loads");

        assert_eq!(config.server.port, 9000, "the environment still wins over the file");
        assert_eq!(config.server.host, "0.0.0.0", "the file still wins over the framework default");
        assert_eq!(config.name, "file");
    }

    #[test]
    fn application_defaults_override_framework_defaults_and_yield_to_files() {
        #[derive(Serialize)]
        struct Defaults {
            name: String,
            value: i32,
            server: ServerConfig,
        }
        let defaults = || Defaults {
            name: "default".into(),
            value: 7,
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 8700,
            },
        };

        let builder = ConfigBuilder::framework_defaults().defaults(defaults()).expect("defaults serialize");
        let config: ConfigWrapper<TestConfig> = builder.clone().build().expect("loads");
        assert_eq!(config.server.port, 8700, "application defaults win over framework defaults");
        assert_eq!(config.name, "default");

        // Defaults are part of the source description, so restored builders behave the same.
        let config: ConfigWrapper<TestConfig> = ConfigBuilder::from_state(builder.state()).build().expect("loads");
        assert_eq!(config.server.port, 8700);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("application.toml");
        std::fs::write(&path, "name = \"file\"\nvalue = 1\n\n[server]\nhost = \"10.0.0.1\"\nport = 18080\n").unwrap();
        let config: ConfigWrapper<TestConfig> = builder.file(&path).build().expect("loads");
        assert_eq!(config.server.port, 18080, "a file wins over application defaults");
        assert_eq!(config.server.host, "10.0.0.1");
        assert_eq!(config.name, "file");
    }

    #[test]
    fn environment_overrides_application_defaults() {
        if !in_isolated_child("config::tests::environment_overrides_application_defaults", &[("APP_SERVER__PORT", "9000")]) {
            return;
        }

        #[derive(Serialize)]
        struct Defaults {
            name: String,
            value: i32,
            server: ServerConfig,
        }
        let config: ConfigWrapper<TestConfig> = ConfigBuilder::framework_defaults()
            .defaults(Defaults {
                name: "default".into(),
                value: 7,
                server: ServerConfig {
                    host: "0.0.0.0".into(),
                    port: 8700,
                },
            })
            .expect("defaults serialize")
            .env("APP")
            .build()
            .expect("loads");

        assert_eq!(config.server.port, 9000, "the environment wins over application defaults");
        assert_eq!(config.server.host, "0.0.0.0", "application defaults still beat framework defaults");
    }

    #[test]
    fn defaults_that_cannot_be_represented_as_toml_fail_registration() {
        // TOML table keys are strings; integer keys cannot be represented.
        let unsupported = std::collections::HashMap::from([(1u32, 2u8)]);
        assert!(ConfigBuilder::new().defaults(unsupported).is_err());
    }

    #[test]
    fn load_default_returns_loading_errors_instead_of_falling_back() {
        if !in_isolated_child("config::tests::load_default_returns_loading_errors_instead_of_falling_back", &[]) {
            return;
        }

        // With no file, the framework's typed defaults still produce a complete server section.
        let config: ConfigWrapper<TestConfig> = Config::load_default().expect("defaults load without a file");
        assert_eq!(config.server.port, ServerConfig::default().port);

        std::fs::create_dir_all("configurations").unwrap();
        std::fs::write("configurations/application.toml", "value = [").unwrap();
        assert!(
            Config::load_default::<ConfigWrapper<TestConfig>>().is_err(),
            "a loading error must not become T::default()"
        );
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
