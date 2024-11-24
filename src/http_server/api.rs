use axum::extract::{Json, Path, Query, State};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::{types, Result};

#[derive(Debug, Deserialize)]
pub struct CreateTaskBody {
    pub task_type: types::TaskType,
    // TODO(production) make sure there is clear defined format for timestamps
    // and that the server rejects all none valid timestamps with an invalid timestamp error
    // for now chrono will automagically try to convert any string to a DateTime<Utc>
    // and for my testing i will send RFC3339 timestamps
    pub execution_time: DateTime<Utc>,
}

// this is setup as a TryFrom, since when i deploy to production
// I want to be able to send custom error regarding the execution_time field
impl TryFrom<CreateTaskBody> for types::Task {
    type Error = crate::Error;
    fn try_from(value: CreateTaskBody) -> Result<Self> {
        let task_id = uuid::Uuid::new_v4();
        // TODO(production): do some validation here
        // ensure `execution_time` is well documented.
        // what if `execution_time` is in the past?
        // how far in the future can the client set `execution_time`?
        Ok(Self {
            id: task_id,
            execution_time: value.execution_time,
            task_type: value.task_type,
        })
    }
}

pub async fn create_task(
    State(app_state): State<AppState>,
    Json(create_task_body): Json<CreateTaskBody>,
) -> Result<Response> {
    // TODO(production): validate the execution date
    // if date is more than 1 hour in the past, return error.
    let task = types::Task::try_from(create_task_body)?;
    tracing::debug!(
        "New task submitted, Task type: `{:?}`, Execution time: `{}` ",
        task.task_type,
        task.execution_time
    );
    app_state.db_client.create_task(&task).await.map_err(|e| {
        tracing::error!(
            "Unable to submit task id: {} to database! err: {e}",
            task.id
        );
        e
    })?;

    // if a task has a execution_time that is <= (now()+<max_seconds_to_sleep>)
    // we want to execute it as soon as possible, so we notify the pg notification listener thread
    // we do this; instead of using an in memory queue
    // because we want any other worker node to pick up this job if they have free capacity
    let now_plus_n_seconds =
        chrono::Utc::now() + chrono::TimeDelta::seconds(app_state.max_seconds_to_sleep);
    if task.execution_time <= now_plus_n_seconds {
        match app_state
            .db_client
            .notify_pg_channel_of_task(&task, &app_state.channel_name)
            .await
        {
            Ok(_) => (), // all is well
            Err(err) => {
                // since the transaction committing the task to the database succeeded
                // we don't need to return an error if notifying the pg channel failed
                // since the db searcher thread will come across this task in the next <look_for_new_tasks_interval> anyways

                // todo(production): does the API guarantee that if the if the request was successful
                //      and the execution time is <= (now()+<max_seconds_to_sleep>)
                //      then the task should be executed as soon as possible?
                tracing::warn!(
                    "sending notification to pg channel failed, err: {}. ignoring error!",
                    err
                );
            }
        }
    }

    let res = Json::from(serde_json::json!({"task_id":task.id.to_string()}));

    Ok(res.into_response())
}

pub async fn get_task(
    State(app_state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> Result<Response> {
    // todo(production): is an authenticated user also authorized to view this task
    if let Some(task) = app_state.db_client.get_task(task_id).await? {
        Ok(Json::from(task).into_response())
    } else {
        Ok(StatusCode::NOT_FOUND.into_response())
    }
}

pub async fn delete_task(
    State(app_state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> Result<Response> {
    let result = app_state.db_client.delete_task(task_id).await?;

    if result.rows_affected() >= 1 {
        Ok("OK".into_response())
    } else {
        Ok(StatusCode::NOT_FOUND.into_response())
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ListTasksParams {
    status: Option<types::TaskStatus>,
    #[serde(rename = "type")]
    typ: Option<types::TaskType>,
}

pub async fn list_tasks(
    State(app_state): State<AppState>,
    Query(params): Query<ListTasksParams>,
) -> Result<Response> {
    // todo(production): is an authenticated user also authorized to view these tasks
    let results = app_state
        .db_client
        .list_tasks(params.status, params.typ)
        .await?;

    let response = serde_json::json!({"count":results.len(),"tasks":results});
    Ok(Json::from(response).into_response())
}
