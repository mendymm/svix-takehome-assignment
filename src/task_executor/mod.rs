mod executor;
mod notification_handler;
mod pg_searcher;
mod task_handlers;
mod work_queue;

// this is the main function for the executor
pub use executor::start_executor;

// this will be started by executor::start_executor
pub use notification_handler::start_pg_listener;
pub use pg_searcher::start_pg_searcher;
pub use work_queue::start_work_queue;

// this will be sent in the mpsc channel
pub enum QueueEvent {
    Task(crate::types::Task),
    // todo(production): maybe send this in a graceful shutdown
    #[allow(dead_code)]
    Stop,
}
