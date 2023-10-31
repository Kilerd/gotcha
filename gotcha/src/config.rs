use std::path::PathBuf;
use std::str::FromStr;
use yaac::{ConfigLoader, EnvironmentSource, FileSource};
use serde::de::DeserializeOwned;

pub struct GotchaConfigLoader;
impl GotchaConfigLoader {
    pub fn load<T: DeserializeOwned>(profile: Option<String>) -> T {
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
