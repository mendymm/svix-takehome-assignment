use std::str::FromStr;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretBox};
use sqlx::postgres::PgPoolOptions;
use sqlx::Executor;
use sqlx::{postgres::PgConnectOptions, PgPool};

#[derive(Debug, serde::Deserialize)]
// app config is not clone since the secrecy::SecretBox is not clone* (excluding the number types)
pub struct AppConfig {
    pub db: DbConfig,
    pub server: ServerConfig,

    #[serde(skip)]
    pub environment: Environment,
}

#[derive(Debug, serde::Deserialize)]
pub struct ServerConfig {
    // should the server print config on startup
    // useful when debugging
    pub print_config_on_startup: bool,

    pub listen_port: u16,

    /// this is max amount of time a worker thread is allowed to sleep before executing the task
    /// this allows the scheduler to send a task that is due in <max_seconds_to_sleep> to an executer
    /// and the executer will sleep until the `task.execution_time` is <= now()
    pub max_seconds_to_sleep: i64,
    /// the interval that the schedular thread should look for new tasks that need to be executed
    /// in the nex <max_seconds_to_sleep> seconds
    ///
    /// this is in seconds
    pub look_for_new_tasks_interval: i64,

    /// the max number of tasks that will be in memory before waiting their execution
    /// this is the maximum number of tasks that can wait for for <max_seconds_to_sleep>
    pub max_concurrent_tasks_in_memory: usize,

    /// after a task executer is done waiting for `task.execution_time` to be <= now()
    /// this is the max number of tasks that can do "real" work
    pub max_concurrent_executing_tasks: usize,
}

#[derive(Debug, serde::Deserialize)]
pub struct DbConfig {
    acquire_timeout: u32,
    host: String,
    port: u16,
    username: String,
    // don't want to accidentally print this secret
    password: SecretBox<String>,
    database: String,
    pub tasks_channel_name: String,
}

impl DbConfig {
    pub async fn get_conn_pool(&self) -> Result<PgPool, sqlx::Error> {
        let options = PgConnectOptions::new_without_pgpass()
            .host(&self.host)
            .port(self.port)
            .username(&self.username)
            .password(self.password.expose_secret())
            .database(&self.database);

        PgPoolOptions::new()
            .acquire_timeout(Duration::from_secs(self.acquire_timeout as u64))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    // this is the default, but since the task executer relies on this to ensure tasks are executed exactly once
                    //      i explicitly set this value to 'read committed'
                    conn.execute("SET default_transaction_isolation TO 'read committed'")
                        .await?;
                    Ok(())
                })
            })
            .connect_with(options)
            .await
    }
}

/// The possible runtime environment for our application.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::FromStr)]
pub enum Environment {
    Production,
    Local,
}

impl Default for Environment {
    fn default() -> Self {
        Self::Local
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Environment::Local => write!(f, "local"),
            Environment::Production => write!(f, "production"),
        }
    }
}

pub fn load_config() -> Result<AppConfig, config::ConfigError> {
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let configuration_directory = base_path.join("config");

    // Detect the running environment.
    // Default to `local` if unspecified.
    let env_str = std::env::var("APP_ENVIRONMENT").unwrap_or_else(|_| {
        println!("No `APP_ENVIRONMENT` specified! starting in local mode");
        "local".into()
    });
    let environment = Environment::from_str(&env_str)
        .expect("Failed to parse APP_ENVIRONMENT, expected on of `local`, `production`");

    let conf_loader = config::Config::builder()
        // Read the "default" configuration file
        .add_source(config::File::from(configuration_directory.join("base")).required(true))
        // Read the config from the current "environment"
        .add_source(
            config::File::from(configuration_directory.join(environment.to_string()))
                .required(true),
        )
        // Add in settings from environment variables (with a prefix of APP and '__' as separator)
        // E.g. `APP_APPLICATION__PORT=5001 would set `Settings.application.port`
        .add_source(config::Environment::with_prefix("app").separator("__"))
        .build()?;

    let mut app_config: AppConfig = conf_loader.try_deserialize()?;
    app_config.environment = environment;
    if app_config.server.print_config_on_startup {
        println!("{:?}", app_config);
    }
    Ok(app_config)
}
