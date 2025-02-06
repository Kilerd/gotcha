use std::path::PathBuf;
use std::str::FromStr;

use mofa::{ConfigLoader, EnvironmentSource, FileSource};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::GotchaError;

#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigWrapper<T: DeserializeOwned + Serialize + Default> {
    #[cfg(not(feature = "cloudflare_worker"))]
    pub basic: BasicConfig,

    #[serde(bound = "", default)]
    pub application: T,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct BasicConfig {
    pub host: String,
    pub port: u16,
}

pub struct GotchaConfigLoader;
impl GotchaConfigLoader {
    pub fn load<T: for<'de> Deserialize<'de>>(profile: Option<String>) -> T {
        let mut loader = ConfigLoader::new();

        loader.add_source(FileSource::new(PathBuf::from_str("configurations/application.toml").unwrap()));

        if let Some(profile) = profile {
            let profile_path = format!("configurations/application_{}.toml", profile);
            loader.add_source(FileSource::new(PathBuf::from_str(&profile_path).unwrap()));
        }
        loader.add_source(EnvironmentSource::new("APP"));
        loader.enable_path_variable_processor();
        loader.enable_environment_variable_processor();
        loader.construct().unwrap()
    }
    #[cfg(feature = "cloudflare_worker")]
    pub fn load_from_env<T: for<'de> Deserialize<'de>>(env: worker::Env) -> Result<T, GotchaError> {
        let mut loader = ConfigLoader::new();
        loader.enable_path_variable_processor();
        loader.construct().map_err(|e| GotchaError::ConfigError(e.to_string()))
    }
}
