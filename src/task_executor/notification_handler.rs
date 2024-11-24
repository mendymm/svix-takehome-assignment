use std::{sync::Arc, time::Duration};

use sqlx::postgres::PgListener;
use tokio::sync::mpsc::Sender;

use crate::{db::DbClient, types, AppConfig};

// for now there are only 2 events
// all enum variants are to be de/serialized in snake_case
#[derive(Debug, PartialEq, Eq)]
pub enum Notification {
    /// this will be sent when the server gets a SIGTERM
    /// the worker node will break out of if this event it sent
    /// this is a special case since there is no body only a type
    Stop,
    /// the event with a task
    NewTask(types::Task),
}

/// parse the raw notification text to the Notification enum
impl TryFrom<&str> for Notification {
    type Error = String;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == "stop" {
            Ok(Self::Stop)
        } else {
            // notification type will be in plain before the
            if let Some((notification_type, notification_body)) = value.split_once(' ') {
                match notification_type {
                    "new_task" => match serde_json::from_str(notification_body) {
                        Ok(task) => Ok(Self::NewTask(task)),
                        Err(err) => Err(format!(
                            "Unable to deserialized notification body. err: {err}"
                        )),
                    },
                    _ => Err(format!(
                        "Unexpected notification type: `{notification_type}`"
                    )),
                }
            } else {
                Err("Unable to split notification type from body".to_string())
            }
        }
    }
}

pub async fn start_pg_listener(
    app_config: Arc<AppConfig>,
    db_client: DbClient,
    sender: Sender<super::QueueEvent>,
    task_ids_in_queue: scc::HashSet<uuid::Uuid>,
) {
    tracing::debug!("Started pg notification listener");

    let mut listener = PgListener::connect_with(&db_client.pool()).await.unwrap();
    tracing::debug!(
        "subscribing to the `{}` postgres channel",
        &app_config.db.tasks_channel_name
    );
    listener
        .listen(&app_config.db.tasks_channel_name)
        .await
        .unwrap();

    loop {
        // ask for next notification, re-connecting (transparently) if needed
        let raw_notification = listener.recv().await.unwrap();
        tracing::debug!("Got notification!");

        // this is VERY unlikely to happen, but we will still check
        if raw_notification.channel() != app_config.db.tasks_channel_name {
            tracing::warn!(
                "Got notification intended for channel `{}` expected `{}`. ignoring",
                raw_notification.channel(),
                &app_config.db.tasks_channel_name
            );
            continue;
        }

        let notification = match Notification::try_from(raw_notification.payload()) {
            Ok(notification) => notification,
            Err(err) => {
                tracing::warn!("Err while parsing notification: {err}, ignoring");
                continue;
            }
        };
        match notification {
            Notification::Stop => {
                tracing::info!("Got stop notification, task executer will stop!");
                break;
            }
            Notification::NewTask(task) => {
                if task_ids_in_queue.contains_async(&task.id).await {
                    // we don't need to process this notification since the task is already in the queue
                    continue;
                } else {
                    process_notification(task, &app_config, &sender, &task_ids_in_queue).await;
                }
            }
        }
    }
}

/// <max_sleep_seconds> is defined at `AppConfig.max_seconds_to_sleep`
/// <look_for_new_tasks_interval> is defined at `AppConfig.look_for_new_tasks_interval`
///
///
/// this function will  ensure that `task.execution_time` is <= (now+<max_sleep_seconds>).
/// otherwise it will ignore notification. this is ok; since there is a separate thread
///     that each <look_for_new_tasks_interval> it will query the db for any tasks
///     that need to executed in the next <max_sleep_seconds> and publish a notification
///
/// If the above check is passes, the function will submit the job to an in memory queue.
/// If the server shuts down before the job was started but after it was sent to the in memory queue,
///     the next time the server will start up it will query the database for jobs the are status='submitted'
///     and add them back to this in memory queue.
///     this does mean that the same job will be in multiple server's in memory queue; but this is not an issue
///     since the `execute_task_once` will ensure that a task is only executed once
async fn process_notification(
    task: types::Task,
    app_config: &AppConfig,
    sender: &Sender<super::QueueEvent>,
    task_ids_in_queue: &scc::HashSet<uuid::Uuid>,
) {
    let now_plus_n_seconds =
        chrono::Utc::now() + chrono::TimeDelta::seconds(app_config.max_seconds_to_sleep);
    if task.execution_time <= now_plus_n_seconds {
        tracing::info!("ok");
        submit_task_to_mpsc(task, sender, task_ids_in_queue).await;
    } else {
        // we are assuming that most tasks that we get here have a `task.execution_time` that us <= (now+<max_sleep_seconds>)
        // but if there is an exception, we simply ignore that task
        // the pg_searcher thread will resubmit the task at a later time
    }
}

async fn submit_task_to_mpsc(
    task: types::Task,
    sender: &Sender<super::QueueEvent>,
    task_ids_in_queue: &scc::HashSet<uuid::Uuid>,
) {
    let task_id = task.id;
    if sender.capacity() >= 10 {
        // since there could be a world in which the pg search thread finds submits a task before we do here
        // i check that capacity >= 10 instead of capacity() >= 1
        // this is because this code will be in the super hot path
        // it will wait a maximum of 100 milliseconds, before giving up and ignoring this task

        let result = sender
            .send_timeout(super::QueueEvent::Task(task), Duration::from_millis(100))
            .await;

        if let Err(err) = result {
            match err {
                tokio::sync::mpsc::error::SendTimeoutError::Timeout(_) => (), // we ignore this error,
                tokio::sync::mpsc::error::SendTimeoutError::Closed(_) => {
                    tracing::error!("The work queue is closed!!, panicking :(");
                    panic!("The work queue is closed!!");
                }
            }
        } else {
            // returns error if the key is already in the set
            // this does not bother me
            let _ = task_ids_in_queue.insert_async(task_id).await;
        }
    } else {
        // if the channel has more tasks then the `app_config.max_concurrent_tasks_in_memory`
        // then we ignore this task.
        // how do we insure that every task will be executed eventually?
        // there is a separate thread that will search the database for any tasks that have a `task.execution_time` that us <= (now+<max_sleep_seconds>)
        // and notify the pg channel about them ordering by execution_time ASC
    }
}

#[cfg(test)]
mod test {
    use chrono::DateTime;

    use crate::types::Task;

    // each notification has a <type> and a optional space separated <payload>
    use super::Notification;

    #[test]
    pub fn test_notification_with_no_payload_is_parsed() {
        let notification_str = "stop";
        let expected_notification = Notification::Stop;
        assert_eq!(
            expected_notification,
            Notification::try_from(notification_str).unwrap()
        )
    }
    #[test]
    pub fn test_new_task_notification_is_parsed_correctly() {
        let notification_str = r#"new_task {"id":"7658bfd8-f571-4925-8316-4a8fc75d930e","task_type":"bar","execution_time":"2024-11-24T20:34:36.909592Z"}"#;
        let expected_notification = Notification::NewTask(Task {
            id: uuid::uuid!("7658bfd8-f571-4925-8316-4a8fc75d930e"),
            execution_time: DateTime::parse_from_rfc3339("2024-11-24T20:34:36.909592+00:00")
                .unwrap()
                .to_utc(),
            task_type: crate::types::TaskType::Bar,
        });
        assert_eq!(
            expected_notification,
            Notification::try_from(notification_str).unwrap()
        )
    }
}
