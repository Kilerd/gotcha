use config::{Config, File};
use serde::de::DeserializeOwned;

pub struct GotchaConfigLoader;
impl GotchaConfigLoader {
    pub fn load<T: DeserializeOwned>(profile: Option<String>) -> T {
        let mut s = Config::builder()
            // Start off by merging in the "default" configuration file
            .add_source(File::with_name("configurations/application"));

        if let Some(profile) = profile {
            s = s.add_source(
                File::with_name(&format!("configurations/application_{}", profile)).required(false),
            );
        }
        s = s.add_source(config::Environment::with_prefix("APP"));

        let b = s.build().unwrap();
        let ret = b.try_deserialize().unwrap();
        ret
    }
}
