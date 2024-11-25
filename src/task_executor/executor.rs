use std::sync::Arc;

use scc::HashSet;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{db::DbClient, AppConfig};

pub async fn start_executor(app_config: Arc<AppConfig>, db_client: DbClient) {
    let task_ids_in_queue =
        HashSet::<Uuid>::with_capacity(app_config.server.max_concurrent_tasks_in_memory);
    let (tx, rx) =
        mpsc::channel::<super::QueueEvent>(app_config.server.max_concurrent_tasks_in_memory);

    // start work queue first
    let conf = app_config.clone();
    let db = db_client.clone();
    let task_id_set = task_ids_in_queue.clone();
    let work_queue_handel = tokio::spawn(async {
        super::start_work_queue(conf, db, rx, task_id_set).await;
    });

    let conf = app_config.clone();
    let db = db_client.clone();
    let task_id_set = task_ids_in_queue.clone();
    let sender = tx.clone();
    let pg_searcher_handel = tokio::spawn(async {
        super::start_pg_searcher(conf, db, sender, task_id_set).await;
    });

    let conf = app_config.clone();
    let db = db_client.clone();
    let task_id_set = task_ids_in_queue.clone();
    let notification_handel = tokio::spawn(async {
        super::start_pg_listener(conf, db, tx, task_id_set).await;
    });

    notification_handel.await.unwrap();
    tracing::info!("notification listener thread is completed!");
    work_queue_handel.await.unwrap();
    tracing::info!("work queue thread is completed!");
    pg_searcher_handel.await.unwrap();
    tracing::info!("pg searcher thread is completed!");
}
