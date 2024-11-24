use std::time::Duration;

use secrecy::{ExposeSecret, SecretBox};
use sqlx::postgres::PgPoolOptions;
use sqlx::Executor;
use sqlx::{postgres::PgConnectOptions, PgPool};

#[derive(Debug)]
// app config is not clone since the secrecy::SecretBox is not clone* (excluding the number types)
pub struct AppConfig {
    pub listen_port: u16,
    pub db: DbConfig,
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

#[derive(Debug)]
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

pub fn load_config() -> Result<AppConfig, ()> {
    // TODO(production) load config from file/env, for now it's statically defined
    Ok(AppConfig {
        look_for_new_tasks_interval: 30, // 30 seconds
        max_seconds_to_sleep: 100,       // 30 seconds
        max_concurrent_tasks_in_memory: 2000,
        max_concurrent_executing_tasks: 100,
        listen_port: 3000,
        db: DbConfig {
            acquire_timeout: 10, // 10 seconds
            host: "127.0.0.1".to_string(),
            port: 5432,
            username: "postgres".to_string(),
            password: SecretBox::new(Box::new("password".to_string())),
            database: "svix_tasks".to_string(),
            tasks_channel_name: "new_tasks".to_string(),
        },
    })
}
