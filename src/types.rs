use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, sqlx::Type, Clone, Copy)]
#[sqlx(type_name = "task_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]

pub enum TaskStatus {
    /// when a job is submitted and persisted in the db
    Submitted,
    /// when a worker thread starts executing the job
    StartedExecuting,
    /// when a worker thread completes the job
    Done,
    /// If for some reason this job did not complete
    /// there are no guarantees that the job did not start/partially complete execution
    /// only that for whatever reason the status is not 'done'
    Failed,
}

#[derive(Debug, Deserialize, Serialize, sqlx::Type, PartialEq, Eq, Clone, Copy)]
#[sqlx(type_name = "task_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    /// For "Foo" tasks, the worker should sleep for 3 seconds, and then print "Foo {task_id}".
    Foo,
    /// For "Bar" tasks, the worker should make a GET request to
    /// https://www.whattimeisitrightnow.com/ and print the response's status code
    Bar,
    /// For "Baz" tasks, the worker should generate a random number, N (0 to 343 inclusive),
    /// and print "Baz {N}"
    Baz,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: Uuid,
    pub task_type: TaskType,
    // TODO(production) make sure there is clear defined format for timestamps
    // and that the server rejects all none valid timestamps with an invalid timestamp error
    // for now chrono will automagically try to convert any string to a DateTime<Utc>
    // and for my testing i will send RFC3339 timestamps
    pub execution_time: DateTime<Utc>,
}
