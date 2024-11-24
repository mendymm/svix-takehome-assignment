mod config;
mod db;
mod error;
mod http_server;
mod task_executor;
mod types;

use std::sync::Arc;

pub use config::AppConfig;
pub use error::{Error, Result};
use tracing_subscriber::EnvFilter;

fn start_tracing_subscriber() {
    let rust_log = std::env::var("RUST_LOG").unwrap_or("info".to_string());
    let env_filter = EnvFilter::new(rust_log);
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

#[tokio::main]
async fn main() {
    start_tracing_subscriber();
    let app_config = config::load_config().expect("Unable to load config, exiting!");

    let db_client = db::DbClient::new(&app_config).await;
    let args = std::env::args().collect::<Vec<String>>();
    let usage = "please provide either 'http' or 'executor' as the first argument";
    let command = args.get(1).expect(usage);

    match command.as_str() {
        "http" => http_server::start_server(app_config, db_client).await,
        "executor" => task_executor::start_executor(Arc::new(app_config), db_client).await,
        _ => {
            eprintln!("{}", usage);
            std::process::exit(1)
        }
    }
}
