use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc::Sender;

use crate::{db::DbClient, types, AppConfig};

/// the pg searcher thread accomplishes 2 tasks
///
/// 1) every <look_for_new_tasks_interval> seconds it will search the database for tasks that
///    have status = 'submitted' and execution_time <= now()+<max_seconds_to_sleep>
///
///    it will then submit use the postgres channel to notify all worker nodes of the tasks
///    (in ASC order, so the older task get completed first)
///
/// 2) once on startup, it will do step 1, to ensure all old tasks are in the work queue
///
pub async fn start_pg_searcher(
    app_config: Arc<AppConfig>,
    db_client: DbClient,
    sender: Sender<super::QueueEvent>,
    task_ids_in_queue: scc::HashSet<uuid::Uuid>,
) {
    tracing::debug!("Starting pg searcher thread");
    // execute right on startup
    let sleep_duration = Duration::from_secs(app_config.server.look_for_new_tasks_interval as u64);
    loop {
        search_and_submit_upcoming_tasks(&db_client, &sender, &app_config, &task_ids_in_queue)
            .await;
        // we always sleep for 5 minutes before talking to the db again
        tokio::time::sleep(sleep_duration).await;
    }
}

async fn search_and_submit_upcoming_tasks(
    db_client: &DbClient,
    sender: &Sender<super::QueueEvent>,
    app_config: &AppConfig,
    task_ids_in_queue: &scc::HashSet<uuid::Uuid>,
) {
    let tasks = db_client
        .fetch_task_for_pg_searcher(
            app_config.server.max_concurrent_tasks_in_memory as i64,
            app_config.server.max_seconds_to_sleep,
        )
        .await
        .unwrap();

    for task in tasks {
        if !task_ids_in_queue.contains_async(&task.id).await {
            // this will block if the tokio channel is at capacity
            // do not block for more than 100 milliseconds
            sender
                .send_timeout(
                    super::QueueEvent::Task(types::Task {
                        id: task.id,
                        task_type: task.task_type,
                        execution_time: task.execution_time,
                    }),
                    Duration::from_millis(100),
                )
                .await
                .expect("Tokio Channel should not be closed!");
        }
    }
}
