use std::marker::PhantomData;

use chrono::Local;
use clap::{Args, Parser, Subcommand};
use serde::de::DeserializeOwned;
use tabled::{Table, Tabled};
use tracing::info;

use crate::config::GotchaConfigLoader;

#[derive(Tabled)]
struct MigrationItem {
    version: i64,
    date: String,
    apply: bool,
    description: String,
}

#[derive(Parser, Debug)]
#[clap(about, version, author)]
pub struct Cli {
    #[clap(long, short)]
    profile: Option<String>,

    #[command(subcommand)]
    command: GotchaCommand,
}

#[derive(Subcommand, Debug)]
pub enum GotchaCommand {
    Run(RunOpts),

    Migrate(MigrationOpts),
}

#[derive(Args, Debug)]
pub struct RunOpts {}

#[derive(Args, Debug)]
pub struct MigrationOpts {
    #[command(subcommand)]
    command: Option<MigrateSubCommand>,
}

#[derive(Subcommand, Debug)]
pub enum MigrateSubCommand {
    List,
    New{
        name: String
    },
    Run,
    Down,
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
        let opts = Cli::parse();
        let profile = opts.profile.or(std::env::var("GOTCHA_ACTIVE_PROFILE").ok());
        info!("starting gotcha");
        match profile.as_ref() {
            Some(env) => info!("gotcha is loading with profile [{}]", env),
            None => info!("gotcha is loading without any extra profile"),
        };
        let config: CONFIG = GotchaConfigLoader::load(profile);
        match opts.command {
            GotchaCommand::Run(_) => {
                let server_fn = self.server_fn.unwrap();
                server_fn(config).await.expect("error");
            }
            GotchaCommand::Migrate(migrate_opts) => {
                let cmd = migrate_opts.command.unwrap_or(MigrateSubCommand::List);
                match cmd {
                    MigrateSubCommand::List => {
                        let migrations = vec![
                            MigrationItem {
                                version: 0,
                                date: "2023-01-12".to_string(),
                                apply: true,
                                description: "init vesion of it".to_string(),
                            },
                            MigrationItem {
                                version: 1,
                                date: "2023-01-12".to_string(),
                                apply: false,
                                description: "other".to_string(),
                            },
                        ];

                        let table = Table::new(migrations).to_string();
                        println!("{}", table);
                    }
                    MigrateSubCommand::New { name } => {
                        let normalized = name.replace(" ", "_").replace("-", "_").to_lowercase();
                        let datetime = Local::now().format("%d%m%Y%H%M%S");
                        println!("{}_{}.sql", datetime, normalized);
                    },
                    _ => {
                        todo! {}
                    }
                }
            }
        }
    }
}
