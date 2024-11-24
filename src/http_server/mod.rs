pub mod api;
mod server;

pub use server::start_server;

#[derive(Clone)]
pub struct AppState {
    // this is the name of the channel used with postgres listen/notify
    channel_name: String,
    /// this comes from AppConfig.max_seconds_to_sleep
    max_seconds_to_sleep: i64,

    db_client: crate::db::DbClient,
}
