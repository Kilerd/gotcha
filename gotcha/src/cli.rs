
use clap::{Args, Parser, Subcommand};


#[derive(Parser, Debug)]
#[clap(about, version, author)]
pub enum GotchaOpts {

    Run(RunOpts),

    #[clap(subcommand)]
    Migration(MigrationOpts),

}

#[derive(Subcommand, Debug)]
pub enum MigrationOpts {
}

#[derive(Args, Debug)]
pub struct RunOpts {
    pub mode: String,
}


pub struct GotchaCli<F, const HAS_SERVER_FN: bool=false> {
    server_fn: Option<F>,
}


impl<F, FR> GotchaCli<F, false> where F: (Fn() -> FR)  + Sync + 'static,
FR: std::future::Future<Output = ()>  + 'static,{
    pub fn new() -> Self {
        Self {
            server_fn: None,
        }
    }

    pub fn server(self, f: F) -> GotchaCli<F, true>   {
        GotchaCli {
            server_fn : Some(f),
        }
    }
}

impl<F, FR> GotchaCli<F, true> where F: (Fn() -> FR)  + Sync + 'static,
FR: std::future::Future<Output = ()>  + 'static,{
    
    pub async fn run(self) -> () {
        let opts = GotchaOpts::parse();
        match opts {
            GotchaOpts::Run(opts) => {
                let server_fn = self.server_fn.unwrap();
                server_fn().await;
            },
            GotchaOpts::Migration(_) => todo!(),
        }
    }
}