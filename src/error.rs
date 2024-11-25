use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::types;

pub type Result<T> = std::result::Result<T, Error>;

/// this error will sometimes be returned from the http request handler
/// so we define a IntoResponse impl for it
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Unable to mark task id: {0}, with status: {1:?} as deleted since it is already started"
    )]
    UnableToDeleteTask(Uuid, types::TaskStatus),
    #[error("The task with task_id: {0} was not found in the db")]
    TaskNotFound(Uuid),
    #[error("Sqlx error, {0}")]
    SqlxError(#[from] sqlx::Error),
    #[error("reqwest error, {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("{0}")]
    SerdeJsonError(#[from] serde_json::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        // todo(production): rethink error handling. maybe log error here?

        // the http error message should not leak information
        match self {
            Error::UnableToDeleteTask(_task_id, status) => {
                let message = match status {
                    types::TaskStatus::StartedExecuting => {
                        "Unable to delete this task, since the task already started executing"
                    }
                    types::TaskStatus::Done => {
                        "Unable to delete this task, since the task is already complete"
                    }
                    types::TaskStatus::Failed => {
                        "Unable to delete this task, since the task failed"
                    }
                    _ => "Unable to delete this task",
                };
                (StatusCode::BAD_REQUEST, message).into_response()
            }
            Error::TaskNotFound(_) => (StatusCode::NOT_FOUND, "task not found").into_response(),
            Error::SerdeJsonError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "server error").into_response()
            }
            Self::ReqwestError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "http error").into_response()
            }
            Error::SqlxError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response()
            }
        }
    }
}
