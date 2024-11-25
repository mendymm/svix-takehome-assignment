use chrono::DateTime;
use chrono::Utc;
use sqlx::postgres::types::PgInterval;
use sqlx::postgres::PgQueryResult;
use sqlx::{Executor, PgPool, Postgres, Transaction};

use uuid::Uuid;

use crate::{types, AppConfig, Result};

#[derive(Clone)]
pub struct DbClient {
    pool: PgPool,
}

/// this is the model of the tasks as they are in the database
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TaskInDb {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub status: types::TaskStatus,
    pub execution_time: DateTime<Utc>,
    pub task_type: types::TaskType,
    pub started_executing_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
}

impl DbClient {
    pub async fn new(app_config: &AppConfig) -> Self {
        let db_pool = app_config
            .db
            .get_conn_pool()
            .await
            // this will wait the default timeout of 30 seconds before resulting in this error
            .expect("Unable to connect to db, exiting!");

        Self { pool: db_pool }
    }
    /// clone and return the inner pool
    #[inline]
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }
    pub async fn notify_pg_channel_of_task(
        &self,
        task: &types::Task,
        channel_name: &str,
    ) -> Result<()> {
        // see src/task_executor/notification_handler.rs for this format
        let notification_body = format!("new_task {}", serde_json::to_string(&task)?);

        let _result = sqlx::query!(
            // I use the select pg_notify() syntax, since NOTIFY <chan>, <msg> does not support binding params
            "select pg_notify($1,$2)",
            channel_name,
            notification_body
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn fetch_task_for_pg_searcher(
        &self,
        max_concurrent_tasks_in_memory: i64,
        max_seconds_to_sleep: i64,
    ) -> Result<Vec<types::Task>> {
        let interval = PgInterval {
            days: 0,
            months: 0,
            microseconds: max_seconds_to_sleep * 1_000_000,
        };
        let results = sqlx::query_as!(
            types::Task,
            r#"
        select 
            id,
            task_type "task_type: types::TaskType",
            execution_time 
        from tasks
        -- we ignore all status = 'deleted'
        where status = 'submitted' and 
        execution_time <= (now()+ $1) limit $2;"#,
            interval,
            max_concurrent_tasks_in_memory
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(results)
    }

    pub async fn create_task(&self, task: &types::Task) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let query = sqlx::query!(
            r#"
            insert into tasks(id,status,execution_time,"task_type") values ($1,$2,$3,$4);
            "#,
            task.id,
            types::TaskStatus::Submitted as _,
            &task.execution_time,
            // if i use my custom enum I do not get compile time type checking :(
            // this will panic at runtime if task_type is not the correct type :(
            // TODO(production) find a way to compile time type check this enum
            task.task_type as _
        );

        let _res = tx.execute(query).await?;
        tx.commit().await?;

        Ok(())
    }

    pub async fn get_task(&self, task_id: Uuid) -> Result<Option<TaskInDb>> {
        let result = sqlx::query_as!(
            TaskInDb,
            // todo(production): should this api return all database fields? maybe return only a subset
            r#"SELECT 
                id,
                created_at,
                -- fun sqlx tomfoolery
                status as "status: types::TaskStatus",
                execution_time,
                task_type as "task_type: types::TaskType",
                started_executing_at,
                completed_at,
                failed_at
            from tasks where id = $1 and status != 'deleted'"#,
            task_id
        )
        // todo(production): add a limit to the result size, as well as pagination options (offset&limit/cursor)
        .fetch_optional(&self.pool)
        .await?;
        Ok(result)
    }

    pub async fn list_tasks(
        &self,
        status: Option<types::TaskStatus>,
        typ: Option<types::TaskType>,
    ) -> Result<Vec<TaskInDb>> {
        let result = sqlx::query_as!(
            TaskInDb,
            // todo(production): should this api return all database fields? maybe return only a subset
            r#"SELECT 
                id,
                created_at,
                -- fun sqlx tomfoolery
                status as "status: types::TaskStatus",
                execution_time,
                task_type as "task_type: types::TaskType",
                started_executing_at,
                completed_at,
                failed_at
            from tasks where 
            -- more fun sqlx tomfoolery
            ($1::task_status is null or status = $1) and 
            ($2::task_type is null or task_type = $2) and status != 'deleted';
            "#,
            // even more sqlx type tomfoolery
            status as Option<types::TaskStatus>,
            typ as Option<types::TaskType>,
        )
        // todo(production): add a limit to the result size, as well as pagination options (offset&limit/cursor)
        .fetch_all(&self.pool)
        .await?;
        Ok(result)
    }

    pub async fn mark_task_deleted(&self, task_id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        if let Some(task_status) = get_task_status(&mut tx, task_id).await? {
            if task_status == types::TaskStatus::Submitted {
                // we don't need to check the result since the errors returned by error::TaskNotFound and Error::UnableToDeleteTask
                // will notify the client if the task is not found, or the task can't be deleted
                let _result = sqlx::query!(
                    r#"
                update tasks set
                status = 'deleted'::task_status,
                deleted_at = current_timestamp
                where id = $1 and status != 'deleted'::task_status"#,
                    task_id
                )
                .execute(&self.pool)
                .await?;
                Ok(())
            } else {
                Err(crate::Error::UnableToDeleteTask(task_id, task_status))
            }
        } else {
            Err(crate::Error::TaskNotFound(task_id))
        }
    }
    /// when a worker thread is ready to execute a job, (task.execution <= now())
    /// it will call this function and attempt to acquire an exclusive lock on the task
    /// if successful, the worker thread will execute the task, and no other worker will start this task
    ///
    /// using the concurrency guarantees that postgresql gives us
    /// we can ensure that only 1 worker thread will start executing a task
    /// note: this system assumes that once a tasks started it will reach an "end" status (`completed` or `failed`).
    ///     todo(production): to ensure tasks are not stuck executing forever, have a robots system of timeouts.
    ///
    /// this transaction will attempt to swap the value status from `submitted` to `started_executing`
    /// since we are trying to update the same row (we are filtering on the task_id) postgres will lock the row
    /// https://www.postgresql.org/docs/current/transaction-iso.html#XACT-READ-COMMITTED.
    ///
    /// now once our thread commits the change (swap `submitted` to `started_executing`) postgres will remove the lock
    /// and any other thread also attempting to set this value will revaluate it's query status = `submitted`
    /// and see the status = `started_executing`, it will then not change anything and return with rows_modified = 0
    ///
    /// and if we see rows_modified = 1, we know that this thread was the first to acquire the lock, and we can proceed executing the task
    ///
    /// note 1: this assumes that the transaction isolation level is read committed, (the default level)
    /// note 2: this will also prevent a worker from executing a task that was deleted after the task was submitted to the in memory queue
    ///
    pub async fn acquire_exclusive_lock(&self, task_id: Uuid) -> Result<PgQueryResult> {
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query!(
            r#"update tasks set 
            status = 'started_executing',
            started_executing_at = current_timestamp
            where id = $1
            and status = 'submitted'
            "#,
            task_id
        )
        .execute(&mut *tx)
        .await?;

        // make sure to commit!
        tx.commit().await?;

        Ok(result)
    }

    pub async fn mark_task_done(&self, task_id: Uuid) -> Result<()> {
        let _result = sqlx::query!(
        "update tasks set status = 'done'::task_status, completed_at = current_timestamp where id = $1",
        task_id
    )
    .execute(&self.pool)
    .await?;
        Ok(())
    }
    pub async fn mark_task_failed(&self, task_id: Uuid) -> Result<()> {
        let _result = sqlx::query!(
        "update tasks set status = 'failed'::task_status, failed_at = current_timestamp where id = $1",
        task_id
    )
    .execute(&self.pool)
    .await?;
        Ok(())
    }
}

/// if you need to get the status during a transaction
async fn get_task_status(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
) -> Result<Option<types::TaskStatus>> {
    let record = sqlx::query!(
        r#"select status as "status: types::TaskStatus" from tasks where id = $1"#,
        task_id
    )
    .fetch_optional(tx.as_mut())
    .await?;

    if let Some(status) = record {
        Ok(Some(status.status))
    } else {
        Ok(None)
    }
}
