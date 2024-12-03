use std::path::PathBuf;
use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use yaac::{ConfigLoader, EnvironmentSource, FileSource};

#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigWrapper<T: DeserializeOwned + Serialize> {
    pub basic: BasicConfig,

    #[serde(bound = "", flatten)]
    pub data: T,
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
}
