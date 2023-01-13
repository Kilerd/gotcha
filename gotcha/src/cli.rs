use std::marker::PhantomData;

use clap::{Args, Parser, Subcommand};
use serde::de::DeserializeOwned;

use crate::config::GotchaConfigLoader;

#[derive(Parser, Debug)]
#[clap(about, version, author)]
pub enum GotchaOpts {
    Run(RunOpts),

    #[clap(subcommand)]
    Migration(MigrationOpts),
}

#[derive(Subcommand, Debug)]
pub enum MigrationOpts {}

#[derive(Args, Debug)]
pub struct RunOpts {
    #[clap(long, short)]
    pub profile: Option<String>,
}

pub struct GotchaCli<F, CONFIG: DeserializeOwned = (), const HAS_SERVER_FN: bool = false> {
    server_fn: Option<F>,
    config: PhantomData<CONFIG>,
}

impl<F, FR, CONFIG> GotchaCli<F, CONFIG, false>
where
    F: (Fn(CONFIG) -> FR) + Sync + 'static,
    FR: std::future::Future<Output = Result<(), std::io::Error>> + 'static,
    CONFIG: DeserializeOwned,
{
    pub fn new() -> Self {
        Self {
            server_fn: None,
            config: PhantomData,
        }
    }

    pub fn server(self, f: F) -> GotchaCli<F, CONFIG, true> {
        GotchaCli {
            server_fn: Some(f),
            config: self.config,
        }
    }
}

impl<F, FR, CONFIG> GotchaCli<F, CONFIG, true>
where
    F: (Fn(CONFIG) -> FR) + Sync + 'static,
    FR: std::future::Future<Output = Result<(), std::io::Error>> + 'static,
    CONFIG: DeserializeOwned,
{
    pub async fn run(self) -> () {
        tracing_subscriber::fmt::init();
        let opts = GotchaOpts::parse();

        match opts {
            GotchaOpts::Run(opts) => {
                let config: CONFIG = GotchaConfigLoader::load(opts.profile);
                let server_fn = self.server_fn.unwrap();
                server_fn(config).await;
            }
            GotchaOpts::Migration(_) => todo!(),
        }
    }
}
