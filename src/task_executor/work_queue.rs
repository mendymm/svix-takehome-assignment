use std::sync::Arc;

use tokio::sync::mpsc::Receiver;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{db::DbClient, types, AppConfig};

use super::task_handlers;

pub async fn start_work_queue(
    app_config: Arc<AppConfig>,
    db_client: DbClient,
    mut receiver: Receiver<super::QueueEvent>,
    task_ids_in_queue: scc::HashSet<uuid::Uuid>,
) {
    tracing::debug!("Started in memory work queue!");

    let max_sleeping_tasks = Arc::new(Semaphore::new(
        app_config.server.max_concurrent_tasks_in_memory,
    ));
    let max_executing_tasks = Arc::new(Semaphore::new(
        app_config.server.max_concurrent_executing_tasks,
    ));

    loop {
        if let Some(queue_event) = receiver.recv().await {
            match queue_event {
                crate::task_executor::QueueEvent::Task(task) => {
                    // do not spawn the task if there are no available permits
                    if max_sleeping_tasks.available_permits() >= 1 {
                        let db = db_client.clone();
                        let max_sleeping_tasks = max_sleeping_tasks.clone();
                        let max_executing_tasks = max_executing_tasks.clone();
                        let task_ids_in_queue = task_ids_in_queue.clone();
                        tokio::spawn(async move {
                            execute_task_from_queue(
                                db,
                                task,
                                max_sleeping_tasks,
                                max_executing_tasks,
                                task_ids_in_queue,
                            )
                            .await;
                        });
                    } else {
                        tracing::debug!(
                            "max_sleeping_tasks of {} reached, ignoring task id: {}",
                            app_config.server.max_concurrent_tasks_in_memory,
                            task.id
                        );
                    }
                }
                crate::task_executor::QueueEvent::Stop => {
                    tracing::info!("Work queue loop got a Stop event, breaking out of loop!");
                }
            }
        } else {
            tracing::info!(
                "Work queue loop got a None from receiver.recv(), breaking out of loop!"
            );
            break;
        }
    }
}

#[tracing::instrument(skip_all,fields(task_id=?task.id))]
async fn execute_task_from_queue(
    db_client: DbClient,
    task: types::Task,
    max_sleeping_tasks: Arc<Semaphore>,
    max_executing_tasks: Arc<Semaphore>,
    task_ids_in_queue: scc::HashSet<Uuid>,
) {
    let task_id = task.id;
    tracing::debug!("Acquiring sleep permit");
    let _sleep_permit = max_sleeping_tasks
        .acquire()
        .await
        // todo(production): will I close the semaphore on a graceful shutdown
        .expect("This semaphore should never be closed");

    // todo(production): maybe check in the database if this job is already complete
    // since we will are taking up space in the max_sleeping_tasks Semaphore
    sleep_until_task_is_ready(&task).await;

    let _execution_permit = max_executing_tasks
        .acquire()
        .await
        // todo(production): will I close the semaphore on a graceful shutdown
        .expect("This semaphore should never be closed");
    execute_task_once(&db_client, task).await;

    task_ids_in_queue.remove_async(&task_id).await;
}

async fn sleep_until_task_is_ready(task: &types::Task) {
    let now = chrono::Utc::now();
    if task.execution_time <= now {
        // we will not sleep at all, since it's the execution time is past due
    } else {
        let time_to_sleep = task.execution_time - now;
        if let Ok(duration) = time_to_sleep.to_std() {
            tokio::time::sleep(duration).await;
        } else {
            // to_std will return an error if duration is less than 0
            // so this means we don't need to sleep, and we can ignore this error
        }
    }
}

/// <max_sleep_seconds> is defined at `AppConfig.max_seconds_to_sleep`
///
/// this function will hold the logic guaranteeing that a task is only executed once
/// when this function is called, the `task.execution_time` should be <= (now+<max_sleep_seconds>)
/// we won't check execution_time from here on out
async fn execute_task_once(db_client: &DbClient, task: types::Task) {
    let result = db_client.acquire_exclusive_lock(task.id).await.unwrap();

    if result.rows_affected() == 0 {
        // this means that status != 'submitted'
        // so we assume a different thread got this task
    } else if result.rows_affected() == 1 {
        execute_task(task, db_client).await;
    } else {
        // we should never be able to reach this point, since the sqlx query is filtering on a primary key
        // so we could only ever affect 1 row (the row with that unique primary key) or 0 rows
        unreachable!()
    }
}

async fn execute_task(task: types::Task, db_client: &DbClient) {
    // no need to clone task, just copy 128 bit id
    let task_id = task.id;
    let task_result = match task.task_type {
        types::TaskType::Foo => task_handlers::run_foo_task(task).await,
        types::TaskType::Bar => task_handlers::run_bar_task(task).await,
        types::TaskType::Baz => task_handlers::run_baz_task(task).await,
    };

    match task_result {
        Ok(_) => db_client.mark_task_done(task_id).await.unwrap_or_else(|e| {
            tracing::error!("Unable to mark transaction as done!, err: {e}");
        }),
        Err(err) => {
            tracing::error!(
                "Error while executing task id {} err: {}, marking task as failed!",
                task_id,
                err
            );
            // todo(production): have 2 other ways of notifying me of this failure
            // 1. use email/slack webhook
            // 2. scan log lines and set up alerts that let me know if this ever happened
            db_client
                .mark_task_failed(task_id)
                .await
                .unwrap_or_else(|e| {
                    tracing::error!("Unable to mark transaction as failed!, err: {e}");
                });
        }
    }
}
