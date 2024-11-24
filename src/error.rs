use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub type Result<T> = std::result::Result<T, Error>;

/// this error will sometimes be returned from the http request handler
/// so we define a IntoResponse impl for it
#[derive(Debug, thiserror::Error)]
pub enum Error {
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
        match self {
            // the http error message should not leak information
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
